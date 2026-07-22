# idle-level-gate-clamp-divergence

Focus: protocol contracts — schema-invalid-but-plausible `idle_level` values and
the two independent consumers of that field inside the plugin, which disagree on
out-of-range input. This is the known "idle_level: 7 → 30fps animation runaway"
lead; both code legs are validated at f87ec19.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

- `tiles/claude/schema.json:58` — contract: `idle_level` integer, 0..=6. Enforced
  by nothing at runtime anywhere (sut-analysis §6: no JSON-Schema validation
  exists at runtime or in CI).
- `tiles/claude/tile.json:7` — template interpolates the raw value verbatim:
  `level='{{ s.idle_level | default(0) }}'`.
- Consumer 1 (renderer): `src/lib.rs:1048-1051` — `draw_status` parses and
  **clamps**: `.parse::<usize>().ok().unwrap_or(0).min(IDLE_LEVELS.len() - 1)`.
  Level 7 renders as level 6 (dimmest), and `src/lib.rs:1055-1063` computes
  `glow_a = 0.0` for the clamped last level → **visually static pixels**.
- Consumer 2 (animation gate): `src/content.rs:59-61` — `content_animates` does a
  **literal string match**: idle animates unless the markup contains `level='6'`
  or `level="6"`. `level='7'` is not the literal `'6'` → `animating = true`.
- `src/lib.rs:340-362` — the tick callback queues a draw at `DEFAULT_ANIM_FPS`
  (30) on every frame-clock tick while `store.animating()` is true. There is no
  other check; the gate IS the throttle.
- Producer today can't emit >6: `/home/chussenot/agentic-db/internal/state/state.go:160-166`
  (`DecayLevel` clamps to `DecayLevels-1`, negative → 0) and `state.go:26`
  (`DecayLevels = 7`). But the coupling is a bare cross-repo constant: bump
  `DecayLevels` to 8 in the backend and level-7 payloads become routine while
  the plugin's `IDLE_LEVELS` array and the `'6'` literal stay at 7 levels.

## Failure scenario

Any idle session payload whose `idle_level` formats to something other than the
literal `6` while semantically being the terminal level:

- `idle_level: 7` (or 100) → renderer clamps to 6 (static), gate says animating
  → the tile redraws at 30fps **forever** doing nothing. One tile is a warm bar;
  the deployment is 10 instances — a data-driven reintroduction of the dsl heat
  bug (highest-churn regression cluster, sut-analysis §5), invisible on screen.
- `idle_level: 6.0` (JSON float — schema says integer, `additionalProperties:
  true` and no runtime validation let it through) → template renders the literal
  `"6.0"` (no integer normalization; empirically confirmed at f87ec19, see
  Investigation Log) → renderer's `parse::<usize>()` fails → `unwrap_or(0)` → renders as
  **freshly idle, bright, glowing** (misrender: 60+ minutes idle shown as
  just-idled) AND gate animates forever. Two violations from one value.
- `idle_level: -1` → parse fails → level 0 (bright). Gate animates (level 0
  legitimately glows), so no runaway — but still a misrender of the dimmest
  intent as brightest.

## Suggested assertions (net-new)

- SUT-side Rust `Always` in `ContentStore::set` (`src/content.rs:84-92`), guarded
  on the markup containing an idle status and no other animated element: recompute
  the semantic level the way `draw_status` will (parse + clamp) and compare with
  the gate's verdict; message **"idle animation gate agrees with the renderer's
  clamped level"**. This is exactly the invariant whose absence is the bug.
- SUT-side Rust `Sometimes`, same site, fired when the raw `level` attribute
  parses out of 0..=6 or fails to parse: message **"out-of-range idle_level
  reached the renderer"** — exploration hint pushing Antithesis toward
  schema-invalid payloads.
- Workload check (needs observability): feed `idle_level: 7` on an otherwise
  static tile, wait 2s for settle, then assert the tile's draw counter advances
  at ~0 fps over the next 5s; message **"static idle tile does not redraw at
  animation rate"**. Requires exposing a per-tile frame counter (Engine already
  keeps one, `src/lib.rs:41-52`) to the workload — see open questions.

## Key observations

- The two consumers were written against different representations (string
  markup vs parsed attribute) — a classic contract-split inside one process.
  Every future divergence (e.g. renderer clamp changes, template formatting
  changes) silently re-breaks the gate; the `Always` assertion turns that class
  of regression into a first-run failure.
- The fix direction matters for the assertion site: clamping in the template
  (producer-side of the markup) fixes both consumers at once; fixing
  `content_animates` alone leaves the misrender.

## Open questions

- How should the workload observe redraw rate? Engine's frame counter is not
  exported; without SUT-side help the runaway is only observable as CPU load,
  which is a poor Antithesis signal. If a counter/log line per draw is added,
  the workload assertion becomes cheap; if not, this property is SUT-assertion
  only. Determines instrumentation plan.
- Is the intended contract "plugin clamps and stays static" or "plugin rejects
  out-of-range"? `(needs human input)` — the schema says 0..=6; the renderer
  chose clamping; the gate chose neither. The answer decides whether the
  `Sometimes` marker above should instead be an `Unreachable` ("schema-invalid
  idle_level must never reach the renderer" after producer-side validation is
  added).

### Investigation Log

#### What does minijinja actually render for JSON `6.0` — `"6.0"` or `"6"`?

2026-07-22:

- Examined/probed: throwaway crate pinning `minijinja = "=2.21.0"` (the exact
  Cargo.lock version) replicating `render_template` (`src/markup.rs:109-117`)
  verbatim, run against the real `tiles/claude/tile.json` template; plus the
  live compose path via `pwetty render claude --data -` (offscreen PNG).
- Found: `level='{{ x }}'` with `x: 6.0` renders `level='6.0'` — no integer
  normalization (`6.5` renders `6.5`). Full template with
  `{"sessions":[{"state":"idle","idle_level":6.0,...}]}` emits
  `<status state='idle' level='6.0' .../>`; the rendered PNG shows the
  bright fresh-idle glyph (renderer's `parse::<usize>("6.0")` failed → level 0),
  vs the dim glyph for integer `6` in a control render — the misrender leg is
  confirmed end-to-end. The gate leg follows by substring logic:
  `"level='6.0'"` does not contain the literal `"level='6'"`, so
  `content_animates` returns true.
- Not found: nothing missing.
- Conclusion: resolved — floats do NOT normalize, so the float vector is real
  and does not collapse into the in-range case. Invariant and assertion types
  unchanged; this confirms the property's premise.
