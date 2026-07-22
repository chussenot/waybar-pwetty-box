# stream-recovery-after-framing-violation

Found independently by 3 discovery focuses (protocol-contracts + concurrency + failure-recovery) — merged during synthesis; independent rediscovery is a confidence signal.

Focus of the merged property: the NDJSON framing contract ("one complete JSON
doc per line, valid UTF-8, newline-terminated") and what the plugin's stream
reader does when a **live, still-running** producer violates it — specifically
the invalid-UTF-8 line → reader `break` → blocked `child.wait()` stall, and its
payload-dependent recovery. Distinct from `producer-kill-tile-reconverges.md`,
which covers producer death/EOF: here the child stays alive, so the respawn
machinery never engages — that is precisely the bug. Also distinct from generic
garbage-line robustness (S6 covers non-JSON *valid-UTF-8* lines, which fall
back to a string value and are fine). The defect is specifically the
decode-error → `break` → blocking-`wait()` ordering: recovery is **ordered
behind the child's own progress** — a hidden ordering dependency between the
reader thread and an external process's write schedule.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

- `src/content.rs:256-285` — `spawn_stream`: the respawn loop (loop top at
  content.rs:260).
- `src/content.rs:270-278` — the reader loop:
  ```rust
  for line in BufReader::new(out).lines() {
      let Ok(line) = line else { break };
      ...
  }
  ...
  let _ = child.wait();
  ```
  `src/content.rs:270` — the `Lines` iterator owns the `BufReader`, which owns
  the child's stdout fd. `lines()` validates UTF-8 and yields
  `Err(InvalidData)` for any line containing invalid UTF-8; the `else break`
  at `src/content.rs:271` **silently** exits the loop (no log, no counter) —
  any I/O error takes this arm. The `for` statement ends → the
  `Lines`/`BufReader` drops → the pipe's read end **closes** before the next
  statement. Then `src/content.rs:278` — `child.wait()` **blocks with no
  timeout** until the child exits.
- Child exit mechanism: `tile-watch`
  (`/home/chussenot/agentic-db/internal/tile/tile.go:526-530`) only writes when
  the payload *changes* (string dedupe against `last`; error return ignored).
  After the reader closed the pipe, the child dies only on its NEXT write: its
  next `os.Stdout.Write` (`tile.go:529`) hits EPIPE, and Go's runtime default
  for EPIPE/SIGPIPE on fd 1 is fatal — **verified by repro 2026-07-22** (see
  Investigation Log: reader closed the pipe, a forced cache change made
  tile-watch write, and the process chain exited with wait status 141 =
  128+SIGPIPE, **26ms** after the cache rename). On a quiet desktop that next
  write may be **hours away or never**.
- Process topology of the child (probed 2026-07-22): `sh -c '<cmd>'` on this
  system (`/bin/sh` → dash 0.5.12-12ubuntu3) does **NOT** exec-replace — it
  vforks the Go process and waits (strace: `vfork()` + child `execve`), so the
  plugin's `child` handle is the intermediate `sh`, and the pipe write end is
  held by both `sh` (its inherited stdout, never written) and the Go process
  (the only writer). This does not change the recovery ordering: EPIPE/SIGPIPE
  semantics depend on the read end only, and dash exits immediately after
  reaping its dead child (observed: the 141 status propagated through `sh`
  within the same 26ms window), so `child.wait()` unblocks as soon as the Go
  process dies. The 90s changing-payload deadline needs no adjustment.
- Only after the child dies does `child.wait()` return, the 1s
  `RESPAWN_BACKOFF` (`src/content.rs:258,283`) runs, and the respawned
  producer's immediate initial emit (`tile.go:533`) reconverges the tile.
- Write frequency is data-dependent (gated by the byte-dedupe at
  `tile.go:526-530`):
  - Desktop with an idle session: `idle_ago` ("12m" → "13m") changes
    ~once/minute (`tile.go:126-129`, `fmtAgo`) → child writes, dies via EPIPE,
    respawn → **bounded ~60-70s** stall.
  - Empty desktop (`emptyPayload`, no `idle_ago`): payload **never changes** →
    the child never writes, never dies → `wait()` blocks **forever**; the tile
    is permanently frozen and the producer chain is wedged with zero
    diagnostics. "Hung recovery", not a crash.
- Contrast paths: a **partial line** (no trailing newline, e.g. producer killed
  mid-write) is returned by `lines()` as `Ok` at EOF → `parse_data` string
  fallback → template-error card → covered by the producer-kill property. A
  **non-JSON but valid-UTF-8 garbage line** → string fallback → template-error
  card, next valid line repairs it immediately — degraded but live. Every
  other loop-exit path (EOF, child death) reaches `wait()` with a dead/dying
  child and recovers fine; only the mid-stream decode error leaves a *live,
  quiet* child gating the respawn.
- There is also no line-length cap in the reader (sut-analysis §7): a
  newline-less producer grows host (waybar) memory without bound. Noted here
  as an adjacent framing violation; boundedness is another focus's territory.
- `src/content.rs:250-255` — the comments claim "a producer crash recovers" —
  true only for crashes, not for framing violations. The recovery mechanism
  that safety claim S12 and liveness claim L2 advertise ("producer crash
  recovers", 1s backoff) never engages on this path.

## Failure scenario

1. Workload wraps or replaces the producer so the stream emits one line
   containing a raw `0xFF` byte (or any invalid UTF-8), then continues running
   quietly (mirrors a corrupted write, a locale-mangled title, a producer bug,
   binary garbage from a wrapped command, or fault-injected corruption on the
   pipe).
2. Reader thread breaks out and parks in `child.wait()`: no respawn, no log,
   tile frozen at the last-good content with zero visual indication.
3. A session then transitions to `prompt`. The daemon updates the cache;
   tile-watch writes the new line → SIGPIPE → dies → respawn → recovery. BUT
   if the workload's wrapper does not write (or the desktop stays unchanged),
   the stall is **unbounded** — and even in the lucky case, the recovery
   latency is gated on the next data change rather than on the fixed 1s
   backoff the comments promise. The recovery guarantee is real but
   **data-dependent with an unbounded hole** — exactly the kind of
   partial-failure gap Antithesis exposes.
4. Severity: S2 (silent stale state masking a prompt) with a plausible-looking
   tile — the sut-analysis's dominant risk shape, triggered by a single byte.

Three silent layers stack: the break is unlogged, `wait()` has no timeout, and
the frozen tile is visually indistinguishable from a healthy quiet one (S2,
"silent staleness").

## Reachability note

The real producer cannot emit invalid UTF-8 (`json.Marshal` output is always
valid UTF-8, replacing bad runes with U+FFFD). Triggering this path needs a
corrupting wrapper producer in the workload config (`exec` is arbitrary
`sh -c`), or byte corruption between producer and consumer — which Antithesis
provides. That makes this a plugin-robustness property about *any* configured
stream producer, not a claude-chain property: the plugin's contract is with
ANY `exec` producer, not just tile-watch — the README documents `stream: true`
as a general facility.

## Suggested assertions (net-new) — competing designs kept side by side

All three focuses proposed a marker at the abort site, a recovery marker, and
a workload end-to-end deadline; the types and bounds differ. The evaluation
phase picks.

**Abort-site marker at the `else break` arm (`src/content.rs:271`)** — two
proposed assertion types for the same site:

- Design A (protocol-contracts): SUT `Sometimes`, message **"stream reader
  abandoned the pipe on an unreadable line"** — makes the otherwise-invisible
  stall entry observable, guides exploration, and gives triage a replay
  anchor.
- Design B (concurrency + failure-recovery — proposed independently by both):
  `Reachable`, message **"stream reader exited its line loop on a decode
  error"** / **"stream reader entered line-error abort path"** — requires
  splitting the arm to distinguish `Err` from normal EOF loop-end. This branch
  is currently invisible (no log); marking it gives Antithesis an exploration
  anchor for a state otherwise indistinguishable from a quiet stream.

**Recovery marker** — two placements:

- Design A: SUT `Sometimes` after `child.wait()` returns following an
  abandoned-pipe break: message **"stream reader unwedged after producer
  exit"** — distinguishes "recovered via child death" from "never recovered"
  in timelines.
- Design B: SUT `Sometimes` at the loop top when the respawn iteration begins
  with a flag set by the abort-site marker: message **"stream producer
  respawned after a decode-error loop exit"** / **"stream reader respawned
  producer after line decode error"** — confirms the EPIPE-mediated unblock
  actually happens under fault timing. A run where the abort marker fires and
  this one never does is the hang, discoverable in triage without any timeout
  machinery in the SUT.

**Workload end-to-end deadline `Always`** — two variants with different bounds
and preconditions:

- 10s variant (protocol-contracts): after injecting a framing-violation line
  followed ≥1s later by a valid JSON line, the tile's published markup
  reflects the valid line within 10s; message **"stream applies a valid line
  within 10s of a framing-violation line"**. Expected to FAIL at f87ec19 for
  the invalid-UTF-8 case whenever the wrapper stays alive and silent — this
  property is a first-run bug-finder, not a regression guard.
- 90s changing-payload variant (failure-recovery): only valid when the
  workload guarantees a changing payload (e.g. an idle session so `idle_ago`
  ticks): after injecting a garbage line, the tile republishes fresh content
  within **90s**; message **"producer respawn after decode error completed
  within deadline under changing payload"**. On a never-changing payload this
  bound is provably unattainable at f87ec19 — run that variant only to
  document the wedge, or add it as a separate expected-failure check.

## Fault requirements

None beyond the workload's own corrupting producer wrapper. Antithesis
**pause/resume (SIGSTOP) process faults** on `tile-watch` exercise the
adjacent hung-but-alive case (pipe open, no EOF, no respawn trigger);
resume-reconvergence works via the poll loop, but a permanent hang has **no
recovery path at all** in this design — the plugin's only recovery trigger is
EOF (`content.rs`: no read timeout anywhere).

## Key observations

- The framing contract has three distinct violation classes with three
  distinct behaviors: invalid UTF-8 (silent unbounded stall — worst),
  valid-UTF-8 garbage (visible error card, self-heals — acceptable), missing
  newline at EOF (error card until respawn ~1s — acceptable). Only the first
  violates the liveness the code comments claim.
- The SUT analysis (§7) describes this stall; the mechanism was re-verified at
  the code level by two focuses independently, including the drop-order detail
  (BufReader closes the read end before `wait()`, so the child *will* die on
  its next write — the hang is unbounded only while the producer stays quiet).
- The lazy fix is small — read bytes with `read_until(b'\n')` + lossy
  conversion, or replace `break` with `continue` on `Err` (skip the bad line),
  or drop `child.wait()` in favor of `kill()` + `wait()`, or a read timeout /
  `try_wait` + kill after backoff — which makes this a high-value property to
  land before the fix: it will confirm the defect, then pin the fix. The
  assertions above are written so they survive that fix (after a fix, the
  recovery marker fires reliably and the hang gap disappears; a fix would
  change this property from "documents a wedge" to a clean bounded-recovery
  invariant).
- Antithesis angle: this needs no exotic interleaving — inject one bad byte
  into the stream (workload-side producer wrapper) and then *stop producing*.
  Fault injection on the pipe plus a paused producer is the exact adversarial
  schedule. The assertion pair turns "recovered vs hung" into a visible gap in
  triage.
- Likely overlap with the stream-seam robustness focus (attack surface #5).
  This property claims the *reader-thread progress/ordering* facet: recovery
  must not be gated on the child's write schedule. If the seam agent files a
  garbage-line property, keep both — different invariant (content correctness
  vs thread liveness).

## Open questions

- Can invalid UTF-8 reach the stream in production (niri window titles are the
  only externally-influenced strings, and Go's json.Marshal escapes/replaces
  invalid sequences)? `(partial: json.Marshal replaces invalid UTF-8 with
  U+FFFD, so the REAL producer likely cannot emit it; the vector requires a
  non-Go producer, pipe corruption, or a fault injector — which Antithesis
  provides.)` If the real producer provably can't emit it, the property is
  still valid (general `exec`/`stream: true` contract, per the reachability
  note); triggering then requires a workload wrapper between tile-watch and
  the plugin — an environment-design constraint, not a property change.
- Should the never-changing-payload wedge be filed as a bug (bd) rather than
  only encoded as an expected-failure property? `(needs human input)` — design
  intent per SUT analysis §7 is "no timeouts anywhere, by posture"; whether
  that posture is accepted for the stream reader is an owner call. If
  accepted, the property stays scoped to changing payloads; if not, add a
  bounded-recovery Always after the fix.

### Investigation Log

#### Does a Go binary really die on its first write after the reader closes the pipe (SIGPIPE on fd 1)?

2026-07-22.

- Examined: live probe against the installed `claude-status` binary
  (`~/.local/bin/claude-status tile-watch --db <scratch> 3` with a
  probe-controlled scratch `tiles.json` — `RunWatch` reads only
  `dir(--db)/tiles.json`, never niri or the DB, so the probe is isolated from
  the user's real state). Sequence: FIFO reader consumed the initial emit and
  closed the read end; producer confirmed still alive 0.5s later (payload
  unchanged → no write); scratch `tiles.json` atomically renamed to a changed
  payload.
- Found: the producer chain died with wait status **141 = 128+SIGPIPE**,
  **26ms** after the cache rename (within one 75ms `watchPoll`). It survived
  the read-end close itself and died only on the next write, exactly as the
  property's mechanism describes.
- Not found: no SIGPIPE handler anywhere in ~/agentic-db (grep re-confirmed);
  nothing overrides the Go runtime default.
- Conclusion: resolved. The "child loops writing EPIPE forever" alternative is
  ruled out for the real producer; the stall is bounded by the next payload
  change (unbounded only on a never-changing payload, as the body states).

#### Does `sh -c '<single command>'` exec-replace, so the Go process owns fd 1 directly?

2026-07-22.

- Examined: `/bin/sh` identity (`/bin/sh` and `/usr/bin/sh` → dash
  0.5.12-12ubuntu3); `ps` of the spawned chain during the SIGPIPE probe;
  `strace -f -e trace=fork,vfork,clone,execve /bin/dash -c 'sleep 0.1'`.
- Found: dash does **not** exec-replace even a single simple command — strace
  shows `vfork()` + child `execve`, and ps showed `sh` (pid P) with child
  `claude-status` (pid P+2). The Go process holds an inherited copy of fd 1;
  `sh` holds another but never writes. When the Go child died of SIGPIPE, dash
  exited at once with status 141, so the plugin-side `child.wait()` unblock is
  not delayed by the intermediate.
- Not found: any condition under which dash would linger after its child's
  death in this topology.
- Conclusion: resolved. The common "dash execs simple commands" assumption is
  false on this system, but the only consequence is process-counting (two
  processes per chain — relevant to `orphaned-tile-watch-bounded` and
  `respawn-backoff-floor-holds`), not recovery timing. No bound changes here.
