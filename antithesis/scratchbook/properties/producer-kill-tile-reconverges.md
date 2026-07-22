# producer-kill-tile-reconverges

Focus: failure recovery — producer kill/respawn at arbitrary timing (SUT analysis claim L2/S12).

All suggested assertions are **net-new**; the codebase has no Antithesis instrumentation (see `existing-assertions.md`).

## Code paths

- `src/content.rs:256-284` — `spawn_stream`: reads lines until EOF/error, `child.wait()`, unconditional `sleep(RESPAWN_BACKOFF)` (1s, `content.rs:258`), respawn forever. On EOF the last content is kept (stale-by-design).
- `/home/chussenot/agentic-db/internal/tile/tile.go:499-538` — `RunWatch`: per-process `last string` dedupe state starts empty, and `emit()` is called once immediately at startup (`tile.go:533`), so a **freshly respawned tile-watch always emits the current cache state as its first line**, regardless of whether it changed. This is the reconvergence mechanism.
- `src/lib.rs:366-374` — 150ms dirty poll turns the published content into a repaint.
- `tile.go:529` — one `os.Stdout.Write` of the full line (single write syscall).

## Failure scenario

Kill `tile-watch` (or its `sh` parent) at arbitrary timing:

1. **Between writes** — clean EOF; reader loop ends; wait; 1s backoff; respawn; immediate re-emit; tile converges. Bound ≈ 1s + spawn + 75ms-poll-free initial emit + 150ms dirty poll + 1 frame ≈ **1.3-1.5s**.
2. **Mid-write (torn line)** — a partial JSON line reaches `parse_data`, falls back to a string value; rendering the bundled claude template against a string payload was **empirically confirmed** to produce a minijinja error (`ERR: undefined value`, probe run at f87ec19 against `tiles/claude/tile.json` — the template's `{% set prompting = sessions[0].state == ... %}` hits undefined). `build_markup` (`src/content.rs:156-170`) renders the red "template error" card. Recovery is the same respawn path — the error card must be replaced by true state on the respawned producer's initial emit.
3. **Kill during the backoff window / repeated kills** — each cycle costs ≥1s; convergence still expected once kills stop (fixed backoff, no state carried across cycles).

Note on torn-line reachability: `tile-watch` emits each line via a single write; pipe writes ≤ `PIPE_BUF` (4096) are atomic, so a *kernel-level* torn line requires a payload line > 4096 bytes (long niri window titles across two sessions) or a non-atomic producer. Measured 2026-07-22 (see Investigation Log): realistic payloads are ~550 bytes — an order of magnitude under 4096 — but nothing in agentic-db truncates titles (a 6099-byte line passed through the real tile-watch whole), and JSON HTML-escaping inflates `<`/`>`/`&` 6×, so ~650 such chars in one title cross PIPE_BUF. Against the real producer with normal titles it is effectively unreachable; the workload makes it real with long/escape-heavy titles, a crafted cache entry, or a wrapper producer.

## Suggested assertions (net-new)

- SUT-side `Sometimes` in `spawn_stream` after a successful respawn *following* a completed reader loop: message **"tile content reconverged to cache truth after producer kill"** — fires when a respawned producer's first line is published. Marks the recovery subphase as a replay anchor.
- Workload `Always` (deadline check): after killing a tile's producer, the tile's published markup matches the markup rendered from the current `tiles.json` payload within **5s** (generous vs the ~1.5s mechanism bound): message **"tile matches cache within 5s of producer kill"**.
- Optional workload `Sometimes`: **"producer was killed mid-line and tile later recovered from the template-error card"** (requires the wrapper producer to make torn lines reachable).

## Fault requirements

Process-level kill of the `sh`/`tile-watch` child inside the container — the workload can do this itself (`pkill -f 'tile-watch'`); **no node-termination faults required**.

## Key observations

- The reconvergence guarantee is entirely carried by tile-watch's fresh-`last` initial emit. If the backend ever "optimizes" the initial emit away, kill-recovery silently degrades to "recover on next payload change" — the property would catch exactly that regression.
- During the ~1s downtime the plugin serves stale content with no staleness indicator; state changes during that window are invisible until respawn (accepted-by-design per SUT analysis §7).

## Open questions

None remaining (see Investigation Log).

### Investigation Log

#### Max realistic tile-watch line size vs PIPE_BUF 4096?

2026-07-22:

- Examined: `/home/chussenot/agentic-db/internal/tile/tile.go` (full read);
  live measurements against the installed `claude-status tile-watch` with a
  scratch `--db`/`tiles.json` (RunWatch reads only the cache file — crafted
  cache entries exercise the exact production marshal + single-write path);
  `getconf PIPE_BUF /` = 4096.
- Found: realistic maximal payload (2 sessions, ~150-char titles, long folder
  basenames, all optional fields) marshals to **548 bytes**; empty placeholder
  59 bytes. No truncation anywhere: 2×3000-char titles produced a **6099-byte
  line emitted whole** (titles copied verbatim, tile.go:121,203; only cap is
  `maxSessionsPerTile=2`). `json.Marshal` HTML-escapes `<`,`>`,`&` to 6-byte
  `\uXXXX` forms — 800 `<` in one title → 4858 bytes, so ~650 such chars
  suffice to cross PIPE_BUF.
- Not found: niri-side/Wayland-side title caps (niri source out of scope) —
  whether a real compositor delivers multi-KB titles end-to-end is unverified.
- Conclusion: resolved for this property. Real titles sit ~an order of
  magnitude under 4KB, so the torn-line mid-write variant stays
  **workload-weighted, not production-dominant**: keep the mid-line
  `Sometimes` gated on the workload lever (long/escape-heavy titles or a
  crafted cache entry — a wrapper producer is no longer strictly required,
  since the workload can write tiles.json directly). Invariant, bounds, and
  assertion types unchanged.

#### Does minijinja's `undefined value` error for a plain-string payload depend on minijinja version defaults (UndefinedBehavior)?

2026-07-22:

- Examined: `Cargo.lock` (minijinja 2.21.0), `Cargo.toml` (`minijinja = "2"` —
  floats within 2.x), vendored minijinja 2.21.0 source
  (`src/utils.rs`: `UndefinedBehavior` enum with `#[default] Lenient`;
  `handle_undefined` returns `UndefinedError` when the *parent* is undefined
  under Lenient, but returns undefined silently under Chainable),
  `src/markup.rs:109-117` (`Environment::new()`, never calls
  `set_undefined_behavior` → the compiled-in default applies).
- Found: yes, the error depends on the default staying `Lenient`. Mechanism: a
  plain-string payload binds only `value`, so `sessions` is undefined and
  `sessions[0]` is an index into an undefined parent → `UndefinedError` under
  Lenient. Under `Chainable` the same template would render silently (undefined
  `== 'prompt'` → false, `sessions | length` → 0) — an empty-ish card instead of
  the red error card. Empirically re-confirmed at 2.21.0 both via a probe crate
  pinning `=2.21.0` (ERR `undefined value (in <string>:2)`) and via the real
  compose path (`echo '"not json"' | pwetty render claude --data -` → PNG shows
  the red "template error: undefined value (in <string>:2)" card).
- Not found: nothing missing.
- Conclusion: resolved — the error card is stable as long as the crate keeps the
  default behavior (Lenient is the `#[default]` in 2.x; changing it within 2.x
  would be semver-breaking). A future default change would alter only the
  intermediate observable (empty card vs error card) and the optional mid-line
  `Sometimes` message text; the reconvergence invariant and assertion types are
  unaffected.
