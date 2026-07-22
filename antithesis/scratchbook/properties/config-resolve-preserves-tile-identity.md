# config-resolve-preserves-tile-identity

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## What this is

Config resolution (`config::resolve`, src/config.rs:131-143) has three silent
degradation arms, each of which replaces the *identity* of the configured tile
with a different tile — permanently, because resolve runs exactly once per
module init (src/lib.rs:165) and its result is never revisited. None of the
ten ensemble focuses owns config/preset degradation (SUT analysis ranked
attack surface #7), and the sharpest trigger is a plain filesystem fault, which
makes this Antithesis-injectable rather than "user typo" territory.

## Code paths

- `preset_for` tile_file read error → returns `None`, logs to stderr, raw
  config used alone: src/config.rs:95-102.
- `preset_for` unknown bundled preset name → same silent `None`:
  src/config.rs:104-111. `parse_preset` JSON error → same: config.rs:116-124.
- Hard typed-deserialize error → **wholesale** `Config::default()`:
  src/config.rs:139-142. One wrong-typed field collapses every field.
- `#[serde(default)]` at container level (src/config.rs:12-13) means missing
  fields take `Config::default()` values — notably `fps: 60`
  (config.rs:150) — while the claude preset explicitly sets `fps: 0`
  (tiles/claude/tile.json:4).
- The claude preset carries **no** `exec`/`stream` (tiles/claude/tile.json has
  only width/height/fps/font/format); the producer command lives in the raw
  waybar module config. So preset loss and producer loss are independent.
- Missing template → `"{{ value }}"` default: src/content.rs:184-187.
- No content at all (exec+text both lost) → `has_content = false` → femtovg
  demo tile rendered per frame: src/lib.rs:248, 267-277; `forced = fps > 0`
  (fps 60 from defaults) → 60fps tick callback: src/lib.rs:332-346.

## Failure scenarios

1. **fs fault at init/reload on a `tile_file` path** (transient EIO/ENOENT
   injected exactly while waybar instantiates modules): preset is dropped, raw
   config (exec + stream + geometry) deserializes fine. Result: the producer
   still runs, but the template collapses to `{{ value }}` — the tile renders
   the raw NDJSON payload (or a template-undefined error) instead of session
   state — **and** `fps` jumps from the preset's 0 to the default 60, so a tile
   designed to be cool and gated now force-redraws at 60fps. Wrong content and
   a heat regression from one transient read fault, silently, until the next
   reload.
2. **Typed-deserialize collapse** (one field with a wrong JSON type, e.g.
   `"fps": "30"`): entire config → `Config::default()` → exec lost → 60fps GL
   demo tile. The user's session monitor is replaced by an animated demo that
   looks intentional. This is the SUT analysis's "heat regression via config
   typo" (§7).
3. **Reload multiplies exposure**: every SIGUSR2 reload re-runs init and
   therefore re-resolves configs (lifecycle × fs-fault cross-cut). A fault
   window that misses at boot gets retried at every reload; one hit sticks
   until the *next* reload.

## Suggested assertions (net-new, all in `config::resolve` / callers)

- `Always` — "config naming a tile preset resolves with a template present":
  evaluated at the end of `resolve()`; if the raw config had `tile`/`tile_file`
  then the resolved `Config.format` must be `Some`. Catches scenario 1
  (template silently stripped). Always fits: the check runs on every resolve
  and must hold every time.
- `Always` — "configured exec survives config resolution": if raw had `exec`,
  resolved `Config.exec` equals it. Catches scenario 2 (wholesale collapse to
  defaults). Distinct message from the first — different degradation arm.
- `Reachable` — "preset merge applied to module config": the successful
  merge path (config.rs:132-136) executed, so Antithesis knows the interesting
  branch is being explored at all.

At f87ec19 there is no retry and no error surface at any of these arms, so any
injected fault at resolve time is expected to fail the Always assertions —
that is the finding, not noise.

## Key observations

- The three arms produce three *different* wrong tiles (raw-payload-as-text,
  demo tile, raw-JSON-object tile), all of which look plausible and none of
  which say "config error" on the tile. Severity S2 (silent wrong state) plus
  S5 (heat) simultaneously.
- Resolve-once means the degradation has infinite duration but a tiny trigger
  window — exactly the shape Antithesis's fault-at-startup exploration is good
  at and wall-clock testing never hits.
- The bundled-preset path (`tile: "claude"`) cannot fs-fail (compiled in via
  `crate::tiles::get`, config.rs:104-106); only `tile_file` deployments have
  the fs-fault leg. The deserialize-collapse leg applies to both.
- **Live deployment shape (resolved 2026-07-22):** the user's real config
  (`~/.config/waybar/config.jsonc`) uses bundled `tile: "claude"` in all 20
  `cffi/pwetty#N` blocks; no `tile_file` anywhere. So scenario 1's fs-fault
  leg is *not* reachable in a production-shaped harness — only the
  deserialize-collapse leg is. The workload should still include a
  `tile_file` variant to keep the fs-fault arm covered (it remains supported
  surface per README).

## Open questions

- Does waybar itself abort/refuse the whole bar on a malformed top-level
  config JSON before the plugin ever sees it? Why it matters: bounds which
  perturbations reach `resolve()` (only *valid JSON, wrong types* get through
  waybar's parse) — the workload should perturb value types, not JSON syntax.
- Should scenario 1's assertion instead demand that resolve be retried or the
  error be rendered on the tile? `(needs human input)` — design call on the
  intended degradation contract; at f87ec19 no such mechanism exists, so the
  property can only assert non-degradation.

### Investigation Log

#### Does the live deployment use bundled `tile: "claude"` or an external `tile_file`?

2026-07-22.

- Examined: `/home/chussenot/.config/waybar/config.jsonc` (read-only), all
  three bars and all 20 `cffi/pwetty#N` module blocks.
- Found: every pwetty module declares `"tile": "claude"` (the compiled-in
  preset); the only other pwetty keys are `module_path`, `corner_radius`,
  `stream`, `width`, `exec`, `interval`, `on-click`.
- Not found: any `tile_file` key in the live config.
- Conclusion: resolved — production is bundled-preset-only; fs-fault leg
  unreachable in production shape, workload adds a `tile_file` variant to
  keep the arm covered (noted in Key observations).
