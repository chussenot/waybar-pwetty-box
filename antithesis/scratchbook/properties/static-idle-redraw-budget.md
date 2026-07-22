# static-idle-redraw-budget — evidence

No Antithesis instrumentation exists anywhere in this codebase (see
`existing-assertions.md`); every assertion suggested here is net-new.

## Claim

Redraw scheduling is bounded by content class: a tile whose current content
is static (terminally-idle, empty, plain text) queues **zero** frame-clock
redraws between data changes; a tile whose content animates queues at most
`target_fps` (+1 for the immediate first tick) redraws per second. This is
the codebase's claimed guarantee S11 ("keeps the bar cool",
`src/lib.rs:325-339`) and the site of its highest-churn regression cluster
(dsl 60Hz heat bug; regressed twice within a day — 4269a03, dd610c4).

## Code paths (verified at f87ec19)

- `src/lib.rs:332-362`: tick callback registered only if `could_anim`
  (template contains `<tickerbox`/`<status`/`<pulse`/`<bg`, or `fps > 0`, or
  a background shader). Each tick: `animating = forced || store.animating()`;
  if animating and `now - last >= min_dt` → `queue_draw`. `min_dt` from
  `target_fps` (config `fps`, else `DEFAULT_ANIM_FPS = 30`, src/lib.rs:507).
- `src/content.rs:53-69` `content_animates`: decides the animating flag per
  content update. Idle content animates **unless** `level='6'` (or `"6"`)
  appears — the literal hardcodes agentic-db's `DecayLevels-1` (cross-repo
  constant coupling, sut-analysis §10).
- `tiles/claude/tile.json:7`: the template emits
  `level='{{ s.idle_level | default(0) }}'` — **no clamping**. An
  `idle_level: 7` (or any non-6 out-of-range value) in the payload renders
  as `level='7'` → `content_animates` returns true → the tile animates at
  30fps forever, on content the product defines as terminally static. The
  heat bug reintroduced via data, invisible on the bar (verified: template
  passes the value through verbatim; content_animates checks equality with
  6 only).
- Config-collapse variant (sut-analysis §7): a hard config deserialize error
  falls back to `Config::default()` → demo tile → `forced`-style continuous
  animation at 60fps from a config typo.
- The claude preset ships `fps: 0` (tiles/claude/tile.json:4), so `forced`
  is false and gating is entirely `content_animates` — the assertions below
  apply un-gated to the deployed configuration.

## Failure scenario

Both directions matter (sut-analysis §9, S3/S5):

- **Runaway** (this property): static content that keeps queueing redraws
  turns a 10-tile bar into a constant-CPU space heater; the specific class
  regressed twice already, and the `idle_level: 7` data-driven case slips
  past the current guard today.
- **Freeze** (the other direction — prompt pulse frozen) is a
  staleness/liveness concern for another focus; noted for cross-reference
  since the same `content_animates` decides both.

## Suggested assertions (net-new, SUT-side — this is not observable from
the workload except as noisy process CPU time)

- `Always`: "static tile content queues no frame-clock redraws between
  content changes" — instrument the tick callback: count `queue_draw` calls
  while `store.animating()` is false and no dirty flag was consumed since
  the last content change; assert the count stays 0. (Implementation note:
  with the current code the tick callback simply doesn't queue when
  `animating` is false — the assertion catches the class of regression where
  `content_animates` misclassifies, which is exactly what idle_level:7 does:
  the flag itself is wrong, so instrument at the classification boundary:
  assert `!content_animates(markup)` for any markup whose parsed idle level
  is ≥ 6.)
- `Always`: "animated tile redraw rate stays at or below the target fps" —
  count `queue_draw` per second per tile while animating; assert ≤
  `target_fps + 1` (the `last = 0` sentinel draws immediately once).
- `Sometimes(payload idle_level > 6 rendered)`: "an out-of-range idle level
  reached the renderer" — exploration hint; the workload can inject it by
  writing tiles.json directly (the cache is the workload-writable seam).

Workload-observable proxy (coarse): waybar cumulative CPU time
(`/proc/<pid>/stat` utime+stime) over a window where every tile is
terminally idle stays under a calibrated slope. Useful as a backstop; too
noisy to localize which tile misbehaves.

## Key observations

- The first static-class assertion **fails at f87ec19** for `idle_level: 7`
  payloads — that is the point; it demonstrates the known gating blind spot
  under Antithesis and pins the fix.
- Two independent animation detectors must agree (`could_anim` on the
  template at init vs `content_animates` per update — sut-analysis F3): a
  markup-borne animated tag not literally present in the template never gets
  a tick callback at all (freeze direction). The redraw-budget property only
  covers the runaway direction; do not widen it to cover both with one
  message.
- Escaping makes false positives unlikely: data-borne `<bg`/`<pulse` strings
  are autoescaped to `&lt;bg` before `content_animates` sees them, so title
  text cannot spuriously animate a tile.

## Open questions

- Can the real backend ever emit `idle_level > 6`, or is it reachable only
  via fault-injected/synthetic payloads? Matters for severity: a real-world
  producible trigger makes this a live product bug (fan noise on an idle
  desktop); synthetic-only makes it a contract-robustness finding. The
  backend's DecayLevels derivation caps at 6 per sut-analysis, but the
  seam (tiles.json) is unvalidated input to the plugin either way.
- Where exactly to place the classification assertion so it survives
  refactors of `content_animates` (assert on parsed level vs on the
  animating flag)? Matters: asserting on the flag re-implements the buggy
  function; asserting on parsed level duplicates template knowledge. The
  cleanest is a table-driven invariant "state=idle && level>=6 ⇒ not
  animating"; needs a decision on whether 7 should clamp to 6 (static) —
  the product answer seems clearly "static", but confirm with the owner.

### Investigation Log

#### Should idle_level 7 (out-of-range) clamp to static — product intent?

Investigated 2026-07-22.

- Examined: `content_animates` and its hardcoded `level='6'` literal
  (src/content.rs:53-69), the un-clamped template interpolation
  (tiles/claude/tile.json:7, `level='{{ s.idle_level | default(0) }}'`), the
  tick gating and S11 claim (src/lib.rs:325-362), the preset's `fps: 0`
  (tiles/claude/tile.json:4), sut-analysis §7/§9/§10 (cross-repo constant
  coupling — the 6 mirrors agentic-db's `DecayLevels-1`); README and AGENTS.md
  for any statement of intended semantics for out-of-range idle levels.
- Found: the mechanism as described in this file — the template passes
  `idle_level` through verbatim and `content_animates` checks equality with 6
  only, so a payload `idle_level: 7` animates terminally-static content at
  30fps; the backend's own derivation caps at 6 per sut-analysis, so
  out-of-range values arrive only via the unvalidated tiles.json seam.
- Not found: any statement of intended behavior for `idle_level > 6` — no
  clamp in template or plugin, no documented range constraint, no comment or
  bead. The "clearly static" reading is inference from the S11 product framing
  ("keeps the bar cool"), not a recorded decision.
- Conclusion: tagged `(needs human input)` — product-intent question for the
  owner: clamp ≥6 to static, or treat >6 as an invalid payload. The answer
  fixes the table-driven invariant's exact form.
