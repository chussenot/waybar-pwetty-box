# contentstore-mutex-never-poisoned

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## Code paths

Three sites silently swallow a poisoned `ContentStore` mutex:

- `ContentStore::set`, `src/content.rs:88` — `if let Ok(mut guard) = ...`:
  on poison the update is **dropped on the floor**, but `dirty` is still set
  (content.rs:91), so the main thread redraws the *old* content.
- `ContentStore::markup`, `src/content.rs:100-106` —
  `.map(...).unwrap_or_default()`: on poison returns `""` — the tile renders
  permanently, silently blank.
- `ContentStore::uniforms`, `src/content.rs:109-115` — same pattern, returns
  an empty uniform vec.

## Failure scenario

Mutex poisoning requires a panic while the lock is held. If it ever happened,
the failure mode is the worst S2 shape in the system: `markup()` returns `""`
forever, every reader silently degrades, no log line, no error card — a live
`prompt` becomes an empty tile until waybar restarts. The SUT analysis (§3)
calls this "practically unreachable; ideal Unreachable instrumentation point".

## Why it is genuinely unreachable (the argument the assertion encodes)

The critical sections are tiny and panic-free by construction:

- `set()` holds the lock for `*guard = content` — a move assignment whose only
  side effect is dropping the old `TileContent` (`String` + `Vec` drops; no
  user `Drop` impls; drop of these types cannot panic).
- `markup()`/`uniforms()` hold it for a `String`/`Vec<(String, f32)>` clone.
  Allocation failure in default Rust **aborts** (`handle_alloc_error`), it
  does not unwind — so even OOM cannot poison the lock.

No other code takes this mutex (single `Mutex` in the plugin; grep-verified).
So poisoning is impossible at f87ec19 unless a future change puts panicking
code inside a guard scope — exactly what `Unreachable` is for: a tripwire that
converts "silent permanent blank tile" into a reported property violation the
instant a refactor makes it possible.

## Suggested instrumentation (net-new)

Replace the silent-swallow arms with explicit `Err` branches carrying
`Unreachable` assertions (behavior unchanged — still degrade the same way,
just report first). Three unique messages, one per site:

1. `Unreachable("ContentStore::set dropped an update on a poisoned content mutex")` — content.rs:88 else-arm.
2. `Unreachable("ContentStore::markup read a poisoned content mutex")` — content.rs:100-106 err-arm.
3. `Unreachable("ContentStore::uniforms read a poisoned content mutex")` — content.rs:109-115 err-arm.

Note `take_dirty()` (content.rs:118-120) and `animating()` (content.rs:95-97)
are atomics, not mutex users — no sites there.

An alternative worth recording: switching the swallow to
`.unwrap_or_else(|p| p.into_inner())` (lock-poisoning recovery) would make the
degradation disappear entirely; but that changes semantics, and the
instrument-first approach preserves the existing behavior while making it
observable. Either way the assertion documents the invariant.

## Open questions

- None. The panic-freedom argument covers all three critical sections; the
  property is a pure tripwire.
