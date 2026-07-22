# torn-ndjson-frame-rendered — the plugin never renders content derived from a partial NDJSON line

All suggested assertions here are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Claim under test

S6 in the sut-analysis ("a non-JSON stream line never blanks the tile") holds —
the string fallback keeps *something* on screen. But the fidelity question is
stronger: does the pixel state always correspond to a payload the producer
actually **emitted as a complete line**? A torn final line violates that: the
tile renders content derived from a byte prefix no producer ever meant as a
frame.

## Code paths

- Producer framing: `/home/chussenot/agentic-db/internal/tile/tile.go:526-530` —
  `RunWatch.emit` does one `os.Stdout.Write(append(b, '\n'))` per changed
  payload ("One Write of the full line so pwetty's line reader sees it whole").
- Consumer: `/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/antithesis-research/src/content.rs:256-285`
  (`spawn_stream`): `BufReader::new(out).lines()`; each `Ok(line)` goes through
  `parse_data` (content.rs:126-130) — invalid JSON falls back to
  `Value::String(whole_line)` — then `publish.set(build.content(...))`
  (content.rs:275) renders it.
- Rust `BufRead::lines()` yields a **final partial line without a trailing
  newline** as `Ok(prefix)` at EOF — so a producer killed mid-line hands the
  prefix straight to the render path.
- Template consequence: the claude template
  (`/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/antithesis-research/tiles/claude/tile.json:7`)
  indexes `sessions[0]` — against a string value this raises a minijinja error
  → red inline "template error" card (content.rs:156-163) for ~1s until the
  respawned tile-watch re-emits (RESPAWN_BACKOFF, content.rs:258).

## When can a line actually tear?

The single `Write` protects against *interleaving*, not partial delivery:

- Line ≤ PIPE_BUF (4096 on Linux): the pipe write is all-or-nothing; killing
  the producer between syscalls cannot tear it. Framing holds.
- Line > PIPE_BUF: the kernel may accept a partial write; Go's `File.Write`
  loops on short writes, and a SIGKILL between loop iterations leaves a torn
  prefix in the pipe followed by EOF.
- Payload size is title-dependent: two sessions × niri window titles (app- and
  webpage-controlled — a browser tab title is attacker-influenced text) plus
  fields. Measured against the real binary 2026-07-22 (scratch cache →
  `tile-watch` → first emitted line; see Investigation Log):
  - empty placeholder: **59 bytes**; realistic maximal (2 sessions, ~150-char
    browser-tab-style titles, ~35-char folder basenames, all optional fields):
    **548 bytes** — an order of magnitude under PIPE_BUF.
  - **nothing truncates**: 2×3000-char titles produced a **6099-byte** line
    emitted whole (`sessionTile`/`PayloadFor` copy `w.Title` verbatim,
    tile.go:121,203; the only cap is `maxSessionsPerTile = 2`).
  - JSON escaping inflates hostile text 6×: `json.Marshal` HTML-escapes
    `<`,`>`,`&` to `<` etc. — 800 `<` in ONE title yielded a **4858-byte**
    line, so ~650 such characters in a single title cross PIPE_BUF.

So the invariant "rendered content derives only from complete producer lines"
holds *conditionally* on line size, and process-kill timing is exactly the
fault that exposes the condition. >4096-byte lines are unreachable in normal
operation (~550B ceiling with realistic titles) but reachable in the deployed
chain whenever niri delivers page-controlled titles totaling ~3.9KB raw — or
just ~650 escapable chars in one title; whether niri/Wayland caps title length
below that is the residual unknown.

## Suggested assertion (net-new, SUT-side, Rust plugin)

At the `parse_data` call in the stream path (content.rs:275): when the harness
producer is the structured tile-watch (which emits only JSON objects), assert
the parsed value is an object — i.e. the string-fallback branch is the torn-
frame detector.

- Type: **AlwaysOrUnreachable** — the check runs only when a stream line
  arrives; a run whose workload never streams is acceptable, but every line
  that does arrive must parse whole.
- Message: `"stream reader: NDJSON line from the structured producer parsed as a complete JSON object"`

Gating: the plugin cannot know the producer's contract in general (plain-text
streams are legitimate). In the Antithesis harness the config is ours, so gate
the assertion behind a config flag (e.g. `stream_expect_json: true` set in the
harness module config) rather than asserting for arbitrary users.

Companion: **Sometimes** on the producer respawn completing after an EOF
(`"stream reader: producer respawned and re-emitted after EOF"`) — a replay
anchor proving the kill/recover cycle is exercised, since the torn frame only
occurs inside that cycle.

## Antithesis angle

- SIGKILL tile-watch at random points while the workload drives payload churn
  (title changes) — the kill must land mid-`Write` of a >4KB line to tear.
- Workload lever: create windows with very long titles to push the marshaled
  payload over PIPE_BUF, turning a probabilistic race into a reachable one.
- Process-pause the producer between write-loop iterations (SIGSTOP → SIGKILL)
  to widen the torn window deterministically.

## Key observations

- Recovery from the torn frame is bounded and comes for free: EOF → respawn
  after 1s → tile-watch's initial emit (tile.go:533) restores real state. The
  defect is the ~1s+150ms window rendering non-emitted content, not a wedge.
- Blank lines are skipped (content.rs:272-274) and an *interleaved* line cannot
  occur (single producer per pipe), so the torn-final-line is the only framing
  corruption in this topology.
- The adjacent invalid-UTF-8 path (`Err` from `lines()` → `break` →
  `child.wait()`, content.rs:271, 278) was examined and left uncataloged: Go's
  `json.Marshal` replaces invalid UTF-8, so the stock producer cannot emit it,
  and recovery still occurs at the producer's next write (SIGPIPE → exit →
  respawn → initial re-emit). It matters only for arbitrary non-JSON producers,
  which are outside the harness data chain.

## Open questions

- Does niri (or its Wayland transport) cap window-title length below the
  ~2-4KB/title needed to cross PIPE_BUF? `(partial: the agentic-db side is
  settled — measured 2026-07-22, no truncation anywhere, a 6099-byte line
  passes through the real tile-watch whole, and JSON escape inflation (6× for
  `<`,`>`,`&`) lowers the hostile-title threshold to ~650 chars; only the
  niri-side limit is unchecked — no niri source in scope.)` If niri caps
  titles short, the torn frame needs the workload's wrapper producer or a
  synthetic cache entry (the workload writes tiles.json directly — same
  effect, no wrapper); if not, it is reachable end-to-end via a hostile page
  title. Either way the render-side assertion stays live: the workload can
  make >4KB lines real without touching niri.

### Investigation Log

#### Max realistic tile-watch line size vs PIPE_BUF (4096)?

2026-07-22.

- Examined: `/home/chussenot/agentic-db/internal/tile/tile.go` (full read —
  `SessionTile`, `PayloadFor`, `RunWatch.emit`); live measurements against the
  installed `claude-status tile-watch` with a scratch `--db`/`tiles.json`
  (RunWatch reads only the cache file — no niri/DB — so crafted cache entries
  measure the exact production marshal path, `json.Marshal` + single
  `os.Stdout.Write`).
- Found: empty placeholder 59B; realistic maximal (2 sessions, ~150-char
  titles, long folders, `active`, `unpushed`, idle fields) **548B**; 2×3000-char
  titles → **6099B emitted whole** (no truncation: titles are copied verbatim
  at tile.go:121 and tile.go:203, folders are `filepath.Base(cwd)`, the only
  cap is `maxSessionsPerTile=2`); 800 `<` in one title → **4858B** (HTML
  escaping in `json.Marshal` inflates `<`,`>`,`&` to 6-byte `<` forms).
  Also confirmed `getconf PIPE_BUF /` = 4096.
- Not found: niri-side or Wayland-side title length limits (niri source not in
  scope) — the residual open question above.
- Conclusion: mostly resolved. Normal operation sits ~550B, far below 4096, so
  torn frames are NOT a realistic steady-state risk; but the line length is
  unbounded through this repo's code, so the property is live (not mere
  ceiling documentation) — the workload lever (long/escape-heavy titles, or a
  crafted cache entry) makes >PIPE_BUF lines deterministically reachable. Do
  not switch to a producer-side `Always: line ≤ 4096` — that assertion would
  fail by design under the workload lever; keep the render-side
  AlwaysOrUnreachable as filed.
