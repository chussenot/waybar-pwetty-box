# stream-ingest-memory-bounded — evidence

No Antithesis instrumentation exists anywhere in this codebase (see
`existing-assertions.md`); every assertion suggested here is net-new.

## Claim

The plugin's ingestion of producer output is memory-bounded: no producer
behavior (missing newlines, never exiting, hostile volume) can grow the
waybar process's memory without bound. At f87ec19 this is violated in two
independent ingestion paths.

## Code paths (all verified at f87ec19)

1. **Stream mode, newline-less producer** — `src/content.rs:270`:
   `for line in BufReader::new(out).lines()`. `Lines`/`read_line` appends
   into a `String` until it sees `\n`, with **no length cap**. A producer
   that emits bytes without a newline grows that String — and waybar's RSS —
   at the producer's write rate, forever. The claude deployment runs stream
   mode (`stream: true` in the user's waybar config; `tiles/claude/tile.json`
   itself carries no exec).
2. **Poll mode, never-exiting producer** — `src/content.rs:288-296`:
   `run_command` uses `Command::output()`, which buffers the child's entire
   stdout into a `Vec` until EOF, with no timeout and no cap. Pointing a
   streaming producer (e.g. `tile-watch` itself) at poll mode — the default,
   since `stream` defaults to `false` (`src/config.rs:59`) — buffers all
   output forever: unbounded memory plus a permanently blank tile (the
   publish after `output()` never runs). A one-character config mistake
   (omitting `stream: true`) reaches this path.

Slower monotonic-growth siblings feeding the same RSS observable:

- `INK_CACHE` (`src/lib.rs:591-610`): thread-local `HashMap` keyed by the
  full run-markup string; never evicted. Every distinct title/ago-label
  string adds an entry permanently. Real-world churn (window titles change
  constantly) grows it monotonically, but entries are small — slow burn.
- `ICON_CACHE` (`src/lib.rs:1195-1203, 1300-1315`): keyed by
  `(source, px, tint)` with `Option<Vec<u8>>` values — failures cached as
  permanent `None`. The `src` path comes from the data stream (`app_icon` in
  the claude template), so a stream feeding ever-changing icon paths grows
  the cache (and does a synchronous `fs::read` per new path in the draw
  callback, `src/lib.rs:1248-1251`) without bound.

## Failure scenario

Fault-injected or buggy producer (e.g. tile-watch replaced by / corrupted
into something that writes without newlines, or a partial-write storm under
disk faults) → waybar RSS climbs until the host OOM-killer takes down the
entire bar (S1 severity via resource exhaustion rather than crash). In poll
mode the same happens from a config-level mistake with zero diagnostics.

## Suggested assertions (net-new)

Workload-observable (primary):

- `Always`: "waybar RSS stays below ceiling while a newline-less stream
  producer runs" — workload swaps the tile exec for an adversarial producer
  (`tr -d '\n' < /dev/urandom` shaped) and samples `/proc/<pid>/status`
  VmRSS; assert below a calibrated ceiling.
- `Always`: "waybar RSS stays below ceiling with a non-terminating poll-mode
  exec" — one harness tile configured `stream: false` + a never-exiting
  producer; same RSS sampling. Distinct message because it is a distinct
  code path (`Command::output()` vs `BufReader::lines`).
- `Sometimes(line_len > 4096)`: "stream reader consumed a line larger than
  PIPE_BUF" — exploration hint that the atomicity-free large-line regime was
  actually reached (needs SUT-side counter or a producer-side marker).

SUT-side alternative (sharper): a capped read loop would be the fix; until
then, an instrumented byte-counter on the pending line buffer with
`Always(pending_line_bytes < LIMIT)` turns the OOM into a first-class
property signal instead of a container death.

## Key observations

- A related but distinct hazard shares this code path: an **invalid UTF-8**
  line makes `lines()` yield `Err` → `break` → the reader thread blocks in
  `child.wait()` while the child keeps writing; once the 64KB pipe fills, the
  *producer* stalls mid-write. That is a staleness/liveness failure (tile
  frozen), not memory growth — noted here because a triage of this property
  may surface it first; it deserves its own property in the staleness focus.
- RSS ceiling must be calibrated: baseline waybar + 10 tiles with llvmpipe
  rendering is not small, and shader readback buffers scale with tile size.
  Calibrate on a healthy run before setting the assert threshold.

## Open questions

- What RSS ceiling is fair for the harness topology (waybar + cage + niri +
  llvmpipe + 10 tiles)? Matters: too tight → false positives from Mesa/Pango
  allocation noise; too loose → slow leaks (INK_CACHE/ICON_CACHE) never
  breach within a run. Needs one calibration run.
- How fast does `tile-watch`-shaped real traffic grow INK_CACHE — is the
  slow-burn cache growth observable inside an Antithesis run's compressed
  timeline at all? Matters: if not, the caches stay documentation-only here
  and the property's triggers are purely the two adversarial-producer paths.
- Does `BufReader`'s 8KB internal buffer plus `String` growth double-count
  memory or is growth linear in producer bytes? Matters only for choosing the
  ceiling/slope in the workload check.
