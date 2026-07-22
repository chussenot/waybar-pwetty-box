# duplicate-line-rerender-idempotent

Focus: idempotency and replay — re-delivery of an identical payload line must
derive identical tile content (displayed state is a pure function of the last
line + config, not of delivery history).

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Why this property exists

Duplicate delivery is not hypothetical — it is **guaranteed by design**. Every
`tile-watch` respawn re-emits its current line unconditionally
(`/home/chussenot/agentic-db/internal/tile/tile.go:533` — `last` starts `""`,
JSON is never empty, so the initial emit always fires), and the plugin applies
**every** received line with no dedupe (`src/content.rs:275` —
`publish.set(build.content(&parse_data(&line)))`, no comparison against current
content). So any producer kill/respawn cycle where the payload didn't change
delivers the exact same line again.

Deeper: the **entire dedup chain presumes derivation is pure**. The daemon
skips the cache write when the marshaled payload is byte-identical
(`/home/chussenot/agentic-db/internal/daemon/daemon.go:411-413`), and
tile-watch skips the emit when the line string is unchanged
(`tile.go:526-530`). "No change → no re-send" is only correct if rendering a
payload is a pure function of that payload — any plugin-side time-dependence
(e.g. an `ago` computed in the template) would combine with the dedupe to
freeze a wrong value on screen. Today all time-varying text (`idle_ago`,
decay level) is computed backend-side and re-emitted on change; this property
pins that architectural invariant.

## Purity verification (what was checked at f87ec19)

- `src/markup.rs:109-117` — `render_template` builds a **bare**
  `minijinja::Environment` per call: no `now()`, no `random()`, no custom
  functions/filters (those live in minijinja-contrib, not imported). Rendering
  is deterministic in (template, data).
- `src/content.rs:128-130` — `parse_data` is deterministic (serde_json parse,
  string fallback).
- `src/content.rs:143-151` — `build_uniforms` iterates a
  `HashMap<String, String>`; the resulting `Vec<(String, f32)>` **order** is
  the map's iteration order — stable for one `ContentBuilder` instance
  (the map is built once at `from_config`, `content.rs:190`), but potentially
  different across module instances / restarts (RandomState). Semantically
  harmless today: uniforms are consumed **by name**
  (`src/shader.rs:246-250` — `get_uniform_location(name)`).
- `src/content.rs:84-92` — `ContentStore::set` recomputes `animating` from the
  markup (`content_animates`, deterministic string scan) and unconditionally
  flips `dirty`. A duplicate line therefore costs one redundant
  `queue_draw` (~1 frame per respawn) but converges to identical state.
- Draw-side caches are insert-once keyed deterministically:
  `INK_CACHE` keyed by (run text, family, size×4) (`src/lib.rs:591-610`),
  `ICON_CACHE` keyed by (source, px, tint) (`src/lib.rs:1195-1203,
  1300-1315`). Re-drawing identical content inserts **no new keys** — cache
  growth is driven by distinct content, not by duplicates.

## Failure scenario (what a violation looks like)

If anyone adds a time/random-dependent template function, an env-dependent
filter, or a plugin-side "computed at render time" field, then:

1. Duplicate deliveries (every respawn) render *different* content for the
   same payload — flicker or drift on producer churn; and worse,
2. the dedupe chain stops re-sending "unchanged" payloads whose *rendering*
   should have changed — the tile silently freezes a stale derived value,
   another instance of the silent-staleness product risk (sut-analysis §9 S2).

## Suggested assertions (net-new)

- SUT `Always` in `spawn_stream` (or `ContentStore::set`): keep the last
  `(line, derived TileContent)` pair; when the same line string arrives again,
  re-derive and assert the markup is byte-identical and the uniforms are equal
  **as a name→value map** (order-insensitive): message
  **"re-delivered identical stream line derives identical tile content"**.
- SUT `Sometimes` at the same site when a duplicate is detected: message
  **"duplicate payload line was re-delivered by a respawned producer"** —
  confirms respawn-driven duplicate delivery is actually exercised (exploration
  anchor; pairs with `producer-kill-tile-reconverges`).
- Workload check: configure two module instances with the same `exec`; after
  quiescence, both stores' markup must be byte-identical (cross-instance
  determinism; would catch iteration-order leaks into markup).

## Fault requirements

Producer kills (workload-driven `pkill tile-watch`) generate duplicates
naturally; Antithesis scheduling jitter varies duplicate timing against the
150ms dirty poll and the frame clock. No node faults required.

## Key observations

- The plugin deliberately has **no** receive-side dedupe; idempotency of
  content derivation is the only thing making at-least-once delivery safe.
- Uniform Vec order is instance-stable but not globally deterministic; any
  future order-sensitive consumer (e.g. serializing uniforms into a cache key)
  would break cross-restart reproducibility silently. The assertion comparing
  uniforms as a map (not a Vec) encodes the intended semantics.

## Open questions

- Should the plugin skip `dirty` on byte-identical content (a memcmp in
  `set()`)? Today the cost is one redundant redraw per respawn — negligible at
  1s backoff, but a crash-looping producer (respawn storm) converts duplicates
  into a ~1Hz redraw stream on an otherwise static tile. If that is deemed
  acceptable, the property stays as-is; if not, the fix adds receive-side
  dedupe and this property becomes its regression guard.
