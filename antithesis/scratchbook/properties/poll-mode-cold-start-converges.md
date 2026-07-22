# poll-mode-cold-start-converges

Focus: lifecycle transitions — startup ordering: poll-mode `interval: 0`
one-shot racing the daemon's first cache write ("wrong forever").

All suggested assertions are **net-new**; no Antithesis instrumentation exists
anywhere in this codebase (see `existing-assertions.md`).

## Claim

A poll-mode tile (`exec` set, `stream: false`) eventually reflects backend
state written after the bar started. With the **default** config
(`interval: 0` — `src/config.rs:160-161`), this is **violated by
construction**: the exec runs exactly once at init and never again, so a tile
that starts before the daemon's first cache write renders the empty/erroneous
first result *forever*. This property exists to demonstrate that gap under
Antithesis and to pin the invariant if the design changes (e.g. retry-once-
empty, or a minimum interval).

## Code paths (verified)

- `src/content.rs:206-216` — poll mode: detached thread runs the command,
  publishes, then `if interval == 0 { break; }` — one shot, thread exits, no
  retry ever.
- `src/config.rs:160-161` — defaults: `interval: 0`, `stream: false`. Any user
  config that sets `exec` and forgets `stream: true` / `interval` silently
  lands here. The claude preset (`tiles/claude/tile.json`) does **not** set
  `exec`/`stream` — the wiring lives in the user's waybar block
  (`tiles/claude/README.md:25-31` documents `"stream": true`), so one missing
  line in the waybar config is the trigger.
- One-shot against the backend: `claude-status tile <idx>` (RunTile,
  `/home/chussenot/agentic-db/internal/tile/tile.go:470-483`) emits
  `emptyPayload` on a missing cache key and `BuildLive` (a niri+DB query) when
  the cache file is absent — so the one-shot's output depends entirely on
  whether the daemon has written yet.
- Worse sibling (same config cell): pointing the *streaming* producer
  (`tile-watch`, never exits) at poll mode buffers its stdout forever inside
  `Command::output()` — unbounded memory, tile permanently blank (sut-analysis
  §7; memory-boundedness itself is `stream-ingest-memory-bounded` territory —
  this file only claims the never-converges leg).

## Failure scenario

1. waybar starts at login; a poll-mode tile's one-shot runs at init.
2. The daemon hasn't written `tiles.json` yet → the one-shot emits
   `emptyPayload` (or a template-error card if output is empty).
3. Daemon comes up, sessions go `prompt`, cache updates every second.
4. The tile never re-runs the command. It shows "idle" forever. Severity S2:
   the user misses the waiting prompt, and nothing looks broken.

## Suggested assertions (net-new)

- Workload `Sometimes`: message **"poll-mode tile re-published content
  reflecting a cache write that happened after module init"** — with a
  dedicated poll-mode tile in the workload config. For `interval > 0` runs
  this fires within one interval; for `interval: 0` (the default) it can
  never fire → the run reports the gap as a failed Sometimes. `Sometimes` is
  the right type: it is a progress property, and its non-firing under the
  default config *is* the finding.
- Workload `Sometimes`: message **"poll-mode one-shot ran before the daemon's
  first cache write"** — ordering coverage anchor; without it, a lucky
  daemon-first ordering makes the property meaningless (the one-shot would
  capture correct content and stay correct by coincidence).

**Config note:** this property needs its own tile instance(s) configured in
poll mode; the shipped claude deployment is stream-mode and never exercises
this path. Run both an `interval: 0` variant (expected finding) and an
`interval: 2` variant (expected pass — validates the assertion machinery
itself).

## Fault / harness requirements

- Same start-order control as `cold-start-stream-tile-converges` (bar before
  daemon); no special faults.
- One extra workload config axis (poll-mode tiles). No SUT change strictly
  required — convergence is observable at the `ContentStore::set` hook
  proposed in `cold-start-stream-tile-converges`, or by asserting the thread
  re-ran the exec (SUT-side `Sometimes` in the poll loop after the first
  iteration: message **"poll-mode exec re-ran after its first publish"**).

## Key observations

- This is a *designed-in* liveness hole ("interval 0 = run once" is
  documented behavior, `src/config.rs:50-53`), not an accident — but its
  interaction with startup ordering converts a config shorthand into a
  permanent wrong-state display. The property gives the owner a concrete
  artifact to decide: accept (document "don't use interval 0 with live
  data") or fix (retry until first non-empty result).
- Distinct from `cold-start-stream-tile-converges` because the invariant
  outcome differs by config mode: stream converges by mechanism, poll:0
  cannot. Keeping them separate keeps each assertion message unique and each
  verdict crisp.

## Open questions

- Is `interval: 0` + `exec` a configuration the owner considers supported for
  live data, or explicitly out-of-contract? `(needs human input)` — decides
  whether a failed run is a defect finding or documentation-gap finding. The
  code comment (`src/config.rs:50-53`) documents the mechanics but not the
  intent; nothing in the README warns against it, and the default value makes
  it the path of least resistance, which argues "supported in practice".
- Should the never-exiting-producer-under-poll-mode variant (unbounded
  `Command::output()` buffering) get its own workload weight here, or is it
  fully covered by `stream-ingest-memory-bounded`? Check that property's
  scope during synthesis to avoid a coverage hole between the two.

### Investigation Log

#### Is `interval: 0` + `exec` supported for live data?

- Examined: `src/config.rs:50-53,160-161` (interval docs + default),
  `src/content.rs:206-216` (one-shot break), `tiles/claude/README.md`
  (documents stream wiring only), repo README sections on content sources.
- Found: mechanics documented ("0 = run once"); default is 0; no warning
  against pairing with live data sources.
- Not found: any statement of intent about one-shot mode's role for live
  backends.
- Conclusion: tagged `(needs human input)` — intent is undocumented; both
  readings are consistent with the code.
