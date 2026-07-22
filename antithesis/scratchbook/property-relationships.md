---
sut_path: /home/chussenot/Documents/waybar-pwetty-box
commit: f87ec19c3e40a62425b2145891c2b45d62a36363
updated: 2026-07-22
external_references:
  - path: /home/chussenot/agentic-db
    why: claude-status backend producing the tile-watch stream the claude tile consumes; several properties assert in this repo (Go SDK)
  - path: https://github.com/Alexays/Waybar
    why: host process that dlopens this cdylib via its CFFI module ABI; defines module lifecycle and FFI contract
---

# Property Relationships — waybar-pwetty-box

Clusters of properties sharing evidence, code paths, or failure mechanisms,
with suspected dominance (one property's violation implying another's).
Lightweight — connections noticed during synthesis, not deep analysis. All
slugs refer to `property-catalog.md`.

## Cluster A — Teardown / GL lifecycle (the confirmed-crash family)

`module-teardown-never-aborts-host`, `engine-init-failure-contained`,
`cairo-text-survives-gl-failure`, `shader-recompile-gl-object-leak`,
`shader-recompile-only-on-mtime-change`

- Shared mechanism: one thread-current EGL context per instance; GL calls
  outside a current context abort (epoxy); GL resources "die with the
  context" by policy.
- Dominance: a `module-teardown-never-aborts-host` violation terminates any
  timeline before other properties can signal — at f87ec19 it fires first and
  masks everything downstream of a teardown. Run teardown-heavy workloads
  against the fix branch.
- `shader-recompile-only-on-mtime-change` violation (per-frame retry) is the
  rate *multiplier* for `shader-recompile-gl-object-leak`; leak exhaustion
  escalates into the draw-path `.unwrap()` panics — an S1 endpoint shared
  with the teardown family.

## Cluster B — Reload / process accumulation

`reload-conserves-producer-chains`, `orphaned-tile-watch-bounded`,
`respawn-backoff-floor-holds`, `so-replacement-reload-race`

- Shared mechanism: detached reader threads + "process-lifetime" false
  premise + no teardown hooks; children respawned forever.
- Dominance: `reload-conserves-producer-chains` violation multiplies the
  baseline fork rate that `respawn-backoff-floor-holds`'s aggregate form
  measures ((N+1)/s after N reloads). The two orphan properties are in-process
  (reload) vs OS-level (waybar death) variants of the same conservation law.
- Sequencing: all masked at f87ec19 by the Cluster A crash on reload; these
  become the active reload bugs once the p9c fix merges.

## Cluster C — Silent staleness of the data chain (S2 / F9)

`cache-error-demotes-live-tile`, `daemon-restart-no-placeholder-clobber`,
`watcher-key-survives-output-rename`, `output-readd-tile-recovers`,
`producer-kill-tile-reconverges`, `stream-recovery-after-framing-violation`,
`cold-start-stream-tile-converges`, `poll-mode-cold-start-converges`,
`tile-watch-output-schema-valid`

- Shared failure shape: every link failure renders as plausible normality;
  `prompt` (byte-stable, no time-varying field) is maximally maskable.
- Dominance/adjacency: `tile-cache-never-torn` violations (Cluster E) feed
  `cache-error-demotes-live-tile` (torn read → read error → placeholder).
  `daemon-restart-no-placeholder-clobber` is the writer-side sibling of
  reader-side `cache-error-demotes-live-tile` (missing key after successful
  read vs failed read). `output-readd-tile-recovers` embeds
  `watcher-key-survives-output-rename`'s Always as its stranding detector —
  the latter is the mechanism, the former the end-to-end journey.
  `poll-mode-cold-start-converges` is the degenerate (never-converges)
  variant of `cold-start-stream-tile-converges`.

## Cluster D — Attention pipeline (S3)

`prompt-priority-survives-session-cap`, `prompt-pulse-visibly-advances`,
`animating-markup-has-tick-source`, `animating-gate-matches-stored-content`,
`publish-visible-within-poll-bound`, `static-idle-redraw-budget`,
`idle-level-gate-clamp-divergence`

- Shared chain: producer prompt-cap → template `<pulse>` → `content_animates`
  → tick gate/throttle → f32 clock → frame clock → pixels.
- `prompt-pulse-visibly-advances` is the terminal observable; every other
  property in the cluster is a mechanism-level approximation of it. A
  violation of any upstream property (cap drops prompt; no tick source; gate
  stuck; publish invisible) should manifest as a pulse-advance failure —
  upstream properties exist for triage precision, not extra coverage.
- Complementary pair (explicitly non-overlapping): `animating-gate-matches-stored-content`
  covers transient flag/markup *disagreement*; `animating-markup-has-tick-source`
  covers the permanent failure where they *agree* but no tick source exists
  (correction recorded during synthesis — the former cannot catch the latter).
- Heat direction: `idle-level-gate-clamp-divergence` is the mechanism-level
  cause whose violation implies `static-idle-redraw-budget`'s runaway leg for
  out-of-range idle levels; the shared `Sometimes("out-of-range idle level
  reached the renderer")` should be implemented once.

## Cluster E — Ingestion integrity and bounds

`tile-cache-never-torn`, `torn-ndjson-frame-rendered`,
`stream-line-length-bounded`, `stream-ingest-memory-bounded`,
`duplicate-line-rerender-idempotent`, `content-snapshot-torn-read`,
`contentstore-mutex-never-poisoned`

- Shared seam: cache file → pipe framing → reader thread → ContentStore →
  draw snapshot.
- `stream-line-length-bounded` (mechanism: per-line cap) is the fix-level
  property whose adoption makes the stream leg of
  `stream-ingest-memory-bounded` (outcome: RSS ceiling) hold; the RSS
  property additionally covers the poll-mode leg the line cap can't.
- `content-snapshot-torn-read` and `publish-visible-within-poll-bound`
  (Cluster D) share the net-new `TileContent.generation` counter — implement
  once, serves both.
- `duplicate-line-rerender-idempotent` underwrites the dedup chain that
  `torn-ndjson-frame-rendered` and `producer-kill-tile-reconverges` rely on
  for recovery semantics (replay-on-respawn is only safe if re-render is pure).

## Cluster F — Input robustness at the markup boundary

`embed-placeholder-parity`, `no-control-chars-in-pango-markup`,
`icon-src-read-bounded-nonblocking`, `unknown-session-state-renders-blank`

- Shared trust boundary: data fields (titles, app labels, folder names,
  app_icon paths) originating from arbitrary apps via niri.
- `icon-src-read-bounded-nonblocking` and `stream-line-length-bounded`
  (Cluster E) are both dominated at the outcome level by
  `neighbor-modules-stay-live` (Cluster G): a FIFO wedge or an OOM stall is
  what the canary/stall-budget property observes; the per-mechanism
  properties exist for precise triage.

## Cluster G — Host neutrality (umbrella)

`neighbor-modules-stay-live`, plus outcome-level relationships to
`icon-src-read-bounded-nonblocking`, `stream-line-length-bounded`,
`shader-recompile-only-on-mtime-change` (per-frame fs I/O), and every S1
property.

- The canary + stall-budget observables catch any main-loop wedge, including
  causes no mechanism property predicted. Mechanism properties give the
  "why"; this cluster gives the "whether at all".

## Cluster H — Config and version seams

`config-resolve-preserves-tile-identity`, `cffi-v1-config-transport-retype`,
`producer-binary-swap-mixed-versions`, `so-replacement-reload-race`,
`unknown-session-state-renders-blank`, `idle-level-gate-clamp-divergence`

- Shared shape: unvalidated seams between separately-versioned components
  (waybar transport → plugin config; plugin literal constants ↔ backend
  constants; daemon ↔ tile-watch binary versions).
- `cffi-v1-config-transport-retype` is one *cause* of the outcome
  `config-resolve-preserves-tile-identity` asserts against (retype → whole-
  struct collapse → wrong tile); keep both — transport-level failure needs
  the A/B v2 build to localize, resolve-level failure catches all causes.
- The cross-repo constant couplings (`level='6'` ↔ DecayLevels, state enum,
  session cap ↔ template indices) connect this cluster to D's heat/blank legs
  under version skew.

## Cluster I — Backend ingress & state derivation (post-evaluation gap-fill)

`hook-prompt-never-silently-dropped`, `live-prompt-session-never-reaped`,
`session-desktop-attribution-tracks-window`,
`first-party-overlay-garbage-tolerant`, `idle-decay-reaches-static`,
`active-accent-follows-focus`

- Shared substrate: the historically buggiest layer (six fixes) — hook
  ingress → DB → reconcile/overlay → GC → tile derivation — previously had
  zero properties.
- Cross-cluster link: `live-prompt-session-never-reaped`'s startup mass-reap
  window is the **gc-side twin** of
  `daemon-restart-no-placeholder-clobber`'s writeTiles race (Cluster C) —
  both are "acts before the niri model is adopted"; one fix pattern (gate on
  model adoption) likely resolves both.
- `idle-decay-reaches-static`'s NULL-last_talk divergence feeds Cluster D's
  heat direction (bright + 30fps forever = the dsl class via backend data).
- `active-accent-follows-focus` owns the L3 claim and supplies the
  focus-churn driver that Cluster F's niri-title injection vectors and
  `tile-cache-never-torn`'s write pressure both reuse.

## Cluster J — Silent-thread-death tripwires (post-evaluation gap-fill)

`stream-reader-thread-liveness`, `draw-path-gl-panic-tripwires`,
`poll-refresh-survives-hung-exec`, `shader-pass-blend-state-neutralized`

- All ride-along tripwires converting silent failure classes into named
  findings. `stream-reader-thread-liveness` also retroactively strengthens
  Cluster B: the chain-count equalities must be two-sided (==) or a dead
  reader chain satisfies them.
- `draw-path-gl-panic-tripwires` is the escalation landing zone of
  `shader-recompile-gl-object-leak` (Cluster A).

## Shared instrumentation (implement once, serves many)

- `TileContent.generation` counter → `content-snapshot-torn-read`,
  `publish-visible-within-poll-bound`.
- `Sometimes("out-of-range idle level reached the renderer")` →
  `static-idle-redraw-budget`, `idle-level-gate-clamp-divergence`.
- Per-tile draw/queue_draw counter → `static-idle-redraw-budget`,
  `neighbor-modules-stay-live`, redraw-rate observability generally.
- Validating tee on the tile-watch pipe → `tile-watch-output-schema-valid`,
  `torn-ndjson-frame-rendered`, `producer-binary-swap-mixed-versions`.
- In-container supervisor for waybar/daemon → all ⚠️node-term properties
  without requiring tenant-level node-termination faults.
