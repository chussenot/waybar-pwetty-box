# stream-reader-thread-liveness

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## The failure shape no existing property owns

The stream reader thread (src/content.rs:256-285) runs
`publish.set(build.content(&parse_data(&line)))` per line (content.rs:275).
Order of evaluation matters: `build.content(...)` — minijinja template
render, markup composition, serde parse, uniform resolution — executes
**before** `set()` is entered, i.e. **outside the content mutex**
(`set` takes the lock only after the content is built, content.rs:88). A
panic anywhere in that expression therefore:

- kills only the detached reader thread (no `extern "C"` boundary here — no
  host abort, unlike the draw path);
- poisons **no** mutex (the lock isn't held), so
  `contentstore-mutex-never-poisoned` never fires;
- drops the `BufReader`/pipe read end → the producer SIGPIPEs and dies on
  its next write (exit 141, live-repro-confirmed in the catalog);
- and — the killer — the respawn loop **lived inside the dead thread**
  (`thread::spawn(move || loop { … })`, content.rs:260). Nothing respawns
  anything, ever.

End state: tile frozen at its last content forever, producer chain gone,
all locks healthy, dirty flag quiet. The only trace is Rust's default
panic-hook line on waybar's stderr at the moment of death — after that,
total silence. This is the S2 shape with the least evidence of any in the
catalog.

Distinguish from `stream-recovery-after-framing-violation`: that property
owns the **non-panic** freeze (invalid-UTF-8 `break` → blocked in
`child.wait()`), which is bounded by the producer's next write. This
property owns the **panic** freeze, which nothing bounds.

## Is a panic reachable at f87ec19?

Honestly: no confirmed panic source on this path today. `render_template`
errors are Result-handled into the error card (content.rs:156-163);
serde_json returns Err on garbage and depth limits rather than panicking.
Like `contentstore-mutex-never-poisoned`, this is a **tripwire property**:
the panic-freedom argument is implicit, unenforced, and one refactor away
from false (any `unwrap`/index/slice in a future template filter, markup
pass, or uniform resolver lands exactly here). The liveness half of the
property (thread-count check + heartbeat) additionally catches freeze modes
that aren't panics at all — an accidental blocking call added to the loop,
a future `break` path without respawn.

## Two-sided counting note (applies to sibling properties)

A dead reader eventually drags its producer chain down (SIGPIPE on next
write), so the existing chain-count invariants
(`reload-conserves-producer-chains`, `orphaned-tile-watch-bounded`) detect
it **only after the next payload change** — never on a quiet desktop — and
only because they assert equality (`==`), not `≤`. Any weakening of those
assertions to upper bounds would silently lose the dead-chain signal; this
property records that the equality is load-bearing, and adds detection that
does not wait for a payload change.

## Suggested assertions (net-new)

SUT-side (Rust, in the plugin):

1. `Sometimes("stream reader iterated a line")` — inside the per-line loop
   (content.rs:270-276), details carrying the module/tile id. The per-module
   heartbeat: in a healthy run with N stream tiles and a churning workload,
   this fires continuously; its silence per-module in triage is the freeze
   fingerprint. Also an exploration hint toward the ingest path.
2. `Unreachable("stream reader thread died by panic")` — a drop-guard at
   the top of the thread closure whose `Drop` checks
   `std::thread::panicking()` and asserts only then. Instrument-first:
   behavior unchanged (thread still dies at f87ec19); the tripwire converts
   "silent forever-freeze" into a reported violation. If the fix
   (catch_unwind + continue the respawn loop) lands later, the guard moves
   inside the loop body and the message survives as the regression
   assertion.
3. Name the reader threads at spawn (`thread::Builder::new().name(
   format!("pwetty-stream-{idx}"))`) — not an assertion, but the enabler
   for workload-side counting below; also improves every future triage.

Workload-side:

4. `Always("live stream reader thread count equals stream module count at checkpoints")`
   — two-sided (`==`): count `/proc/<waybar_pid>/task/*/comm` matching the
   thread-name prefix at quiescent checkpoints; compare to the live
   stream-module count (post-reload settle, same bookkeeping as
   `reload-conserves-producer-chains`, which owns the reload-leak
   direction — this property owns the deficit direction).
5. `Sometimes("a stream tile updated after every fault window")` — cheap
   end-to-end companion: after faults stop and the workload forces a cache
   change, every stream tile's content advances (catches a dead reader even
   if thread counting proves brittle in the container).

## Failure scenario

A refactor adds `payload["sessions"][0]` indexing to a template helper. A
crafted (or merely unusual) payload panics the reader on desktop 3. The
producer dies 26ms later on its next write. For the rest of the session,
desktop 3 shows a stale working dot; a prompt there never surfaces. Thread
counts, locks, waybar, and 9 other tiles are all healthy. Only assertion 2
(instantly) and assertion 4 (next checkpoint) see it.

## Antithesis angle

At f87ec19 the panic leg is a tripwire (no injector reaches it); the
liveness legs are exercised by the existing producer-kill / garbage-stream
/ reload drivers for free. If the owner wants the panic leg *provable* in a
run, a `#[cfg(feature = "antithesis")]` panic-on-magic-line seam in the
build path would make assertion 2's plumbing demonstrable — worth doing
once to validate the detection chain, then leaving the seam out.

## Open questions

- Reader threads are currently unnamed; is the thread-name approach
  acceptable to the owner, or should liveness be exposed as a per-store
  heartbeat timestamp readable by the dirty-poll (and asserted SUT-side)?
  Either satisfies assertion 4; the name approach is 1 line and also helps
  non-Antithesis debugging.
- Should the eventual fix be catch_unwind-and-respawn or let-it-die-loudly
  (abort the host)? For a personal tool, a loud crash may be preferable to
  a silent freeze; the assertion design survives either, but the
  `Unreachable` placement differs. `(needs human input)`

### Investigation Log

#### Does a reader-thread panic really leave zero recovery path and no poisoning?

- Examined: src/content.rs:256-285 (spawn_stream), content.rs:84-92
  (ContentStore::set lock scope), content.rs:270-276 (per-line path).
- Found: content build executes before `set` acquires the mutex — panic
  unwinds with no lock held; the respawn loop is inside the spawned
  closure; the thread handle is dropped (detached), so the panic is also
  never observed via join. Producer death follows from the dropped pipe
  read end (SIGPIPE chain already live-repro-confirmed in the catalog).
- Not found: any panic source on the path at f87ec19 (render errors are
  Result-handled; serde_json is non-panicking on malformed input) —
  recorded honestly as tripwire-only for the panic leg.
