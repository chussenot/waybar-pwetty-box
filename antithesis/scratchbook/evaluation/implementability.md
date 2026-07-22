---
sut_path: /home/chussenot/Documents/waybar-pwetty-box
commit: f87ec19c3e40a62425b2145891c2b45d62a36363
updated: 2026-07-22
external_references:
  - path: /home/chussenot/agentic-db
    why: claude-status backend; several catalog properties assert in this repo
  - path: https://github.com/Alexays/Waybar
    why: host process CFFI contract the catalog's lifecycle properties target
---

# Evaluation — Implementability Lens

Method: every SUT-side assertion placement claim was checked against the actual
code (src/content.rs, src/lib.rs, src/markup.rs, src/text.rs in this repo;
internal/tile/tile.go, internal/daemon/daemon.go, internal/daemon/gc.go,
internal/db/db.go in ~/agentic-db at HEAD e0fe9a2). Workload observability and
precondition-constructability were checked against the deployment-topology
design (single container, sway-outer stack, in-container supervisor, DB/
tiles.json write levers).

## Catalog-wide findings

### CW-1. "Published/rendered markup" is unobservable from the workload — no export seam exists or is planned

Multiple **workload** assertions are specified against state that lives only
inside the waybar process (`ContentStore`):

- `producer-kill-tile-reconverges`: workload `Always` — "published markup
  matches markup rendered from the current cache payload within 5s". The
  evidence file (properties/producer-kill-tile-reconverges.md, assertion 2)
  gives no mechanism for the workload to read published markup.
- `prompt-priority-survives-session-cap`: workload `Always("prompt session
  renders as pulsing tile within 3s")` and the cross-repo guard "rendered
  markup contains `<pulse`".
- `output-readd-tile-recovers`: `Sometimes("tile ... rendered live session
  content ...")`.
- `stream-recovery-after-framing-violation`: workload `Always("stream applies
  a valid line within 10s")` — "applies" is an in-process event.
- The topology's `eventually_converged.sh`: "every tile's rendered content
  matches `pwetty render` of the current tiles.json payload".

The only external observables today are screenshots. Pixel-comparing a live
tile against a `pwetty render` reference PNG **cannot work for animated
content**: a prompt tile's whole-frame alpha oscillates per frame
(`cr.paint_with_alpha(osc(time, PULSE_PERIOD, 0.40))`, src/lib.rs:582-586), so
a static reference never matches an arbitrary-phase frame. It works only for
static payloads (idle level 6, app tiles), which excludes exactly the
prompt-centric checks.

The clean fix is a small SUT seam: env-gated debug export of the
last-published markup per tile (e.g. `ContentStore::set` appends/writes
`/tmp/pwetty-debug/tile-N.markup` when `PWETTY_DEBUG_DUMP` is set). That is a
code change **nowhere accounted for** in the catalog, topology, or SDK plan.
Alternatives: restate these oracles SUT-side (plugin-side assertions comparing
against tiles.json couple the plugin to the backend — worse), or accept
screenshot-only checking scoped to static payloads (loses the prompt legs).

**Suggested action**: add the markup-export seam to the topology's SDK/seam
inventory and re-anchor the five assertions above to it; keep screenshots for
the static-content subset only.

### CW-2. Session and title preconditions require real niri windows — the DB-write lever is necessary but not sufficient, and the topology overstates it

Verified in agentic-db:

- `BuildAll` drops sessions with NULL `window_id` (tile.go:404) and requires
  the window to resolve in the daemon's live niri model (tile.go:407) — a
  session row alone never reaches any tile payload.
- The reaper deletes any session whose `window_id` doesn't match a live niri
  window (gc.go:58-62), and any with a `terminal_pid` that isn't running
  (gc.go:63-65), on the daemon's 1s tick. A naively injected DB row **vanishes
  within ~1 second**.
- `Title` is **not a DB column** (schema, db.go:30-41); it comes from the niri
  window (tile.go:152-154, 177). deployment-topology.md's `helper_session.sh`
  claims to "set folder/title fields" via DB writes — folder yes (`cwd` →
  `filepath.Base`), **title no**.

Consequences for precondition constructability:

1. Every property needing "a live prompt session" (cache-error-demotes-live-tile,
   daemon-restart-no-placeholder-clobber, prompt-priority-survives-session-cap,
   prompt-pulse-visibly-advances, cold-start-stream-tile-converges,
   unknown-session-state-renders-blank) needs harness machinery not yet
   specified anywhere: spawn real Wayland clients inside nested niri, discover
   their window IDs (`niri msg`), write matching `window_id` (and NULL or
   live `terminal_pid`) into DB rows. The >2-sessions cap case is eased by
   co-window sessions (N rows sharing one window_id — supported, tile.go:140-145),
   so one window per desktop suffices.
2. Title-vector properties (torn-ndjson >4096B lines via long titles,
   no-control-chars-in-pango-markup, embed-placeholder-parity title leg)
   need a **title-setting client** (OSC-2-capable terminal or a small custom
   client) — and whether C0/multi-KB titles survive Wayland/niri plumbing is
   already an open question. Fallback: direct tiles.json writes reach the
   plugin but bypass the Go path (fine for plugin-side properties, not for
   tile-watch-output-schema-valid's Go legs).
3. Desktops idx 2..N exist only while windows occupy them (niri dynamic
   workspaces) — multi-desktop convergence checks need placed windows too.

Positive: `LoadLive` has no state filter (db.go:429-444) and the schema has no
CHECK constraint — injected unknown states do reach payloads, as the
investigation claimed, **provided** the window-liveness constraint above is
satisfied and the first-party overlay is disabled/absent (it is best-effort
and leaves hook state intact when the sessions dir is empty — daemon.go:300-308).

**Suggested action**: extend the topology's "Backend data plumbing" section
with a window-fixture helper (spawn client → resolve window id → DB row), and
correct the helper_session.sh description (title is not DB-settable).

### CW-3. Hard wall-clock upper bounds in `Always` assertions conflict with the fault set the same properties request

The topology lists CPU modulation in the **default** fault set and thread
pausing as a desired lever. The following `Always` assertions have fixed
wall-clock upper bounds that a legitimate pause/throttle violates on correct
code:

- `neighbor-modules-stay-live`: draw stall ≤ ~250ms (SUT-side).
- `animating-gate-matches-stored-content`: reconverge ≤ 500ms.
- `publish-visible-within-poll-bound`: ≤ 2 poll periods.
- `producer-kill-tile-reconverges`: ≤ 5s.
- `stream-recovery-after-framing-violation`: ≤ 10s.
- `prompt-priority-survives-session-cap`: ≤ 3s (its own open question already
  asks this).

Sharpest instance: **animating-gate-matches-stored-content**. Its stated
invariant is "may disagree only while a set() is in flight"; its prescribed
fault is "pause the reader thread between the animating store and content
write" — i.e. pause **inside** set(). Any pause > 500ms then fires the Always
with zero code defect: the wall-clock oracle cannot express the stated
in-flight condition. This property false-positives by construction under its
own recommended fault.

`publish-visible-within-poll-bound` is deliberately in this shape (the catalog
treats the write→dirty window itself as the bug to surface, with a named
one-line fix) — defensible, but that stance should be explicit per property.
The others should be restated as quiet-period eventually-checks
(`ANTITHESIS_STOP_FAULTS` / `eventually_` commands, which the topology already
provides) or given fault-aware conditioning. Lower-bound timing assertions
(`respawn-backoff-floor-holds` ≥ 1s spacing) are pause-safe and fine.

**Suggested action**: per timing property, either (a) mark "violation under
scheduler fault = the finding" explicitly, or (b) move the bound to a
quiet-period check. Do not ship 250ms/500ms/3s SUT-side Always bounds against
the default fault set as-is.

### CW-4. Direct tiles.json injection races the daemon and the churn driver

The daemon's byte-dedupe is against its **own** `lastTiles`
(daemon.go:411-413), not the file — so it won't clobber an injected file until
its model changes. On a quiet harness, injections persist; but the sketched
`parallel_driver_session_churn.sh` forces model changes continuously, giving
injection-based properties (`static-idle-redraw-budget`,
`idle-level-gate-clamp-divergence`, `unknown-session-state-renders-blank`,
`icon-src-read-bounded-nonblocking` FIFO leg, `tile-watch-output-schema-valid`
corruption legs) windows of only ~seconds. Not fatal — but the test-template
composition must coordinate (quiesce churn during injection phases) and no
document says so.

### CW-5. agentic-db is unpinned; Go line references float

The catalog cites tile.go:515-531, :120, :173, :364, :44 against a repo whose
frontmatter records a path but no commit. At current HEAD e0fe9a2 the anchors
are approximately right (RunWatch emit() spans 515-531 ✓; sessionTile at 120 ✓;
cap truncation at 171-173 ✓) but :364 is `iconCandidates` — `resolveAppIcon`
is at :245 (the `filepath.Join` calls are in findAppSVG/findAppPNG, :270/:289).
Pin the commit before workload implementation; prefer function-level anchors
in assertion specs.

### CW-6. Harness config as sketched deploys the watcher-key misconfiguration

The topology's waybar harness config specifies
`exec: claude-status tile-watch N` — **flagless**. `defaultOutput` is
`"HDMI-A-1"` (tile.go:44) while nested niri's only output is the hardcoded
`winit` (settled in the topology's own investigation). Flagless watchers key on
`HDMI-A-1:N`, the daemon writes `winit:N` — every tile renders the placeholder
forever, and `setup_complete`'s "tiles.json populated + one screenshot" gate
would not catch it (the file is populated; the tiles just show idle). Harness
exec lines must be `claude-status tile-watch --output winit N`. This is the
exact failure class `watcher-key-survives-output-rename` describes, baked into
the harness sketch.

## Property-specific findings

### PS-1. watcher-key-survives-output-rename — oracle and trigger cancel out

The re-scoped trigger is a deliberate wrong-`--output` misconfiguration. The
Always ("every live tile-watch key resolves in the tile cache **when its
desktop exists**") is then either vacuous (under the wrong output name, "its
desktop" never exists in the cache, so the condition excludes the very case)
or guaranteed-failing the instant the workload injects the misconfig — with no
SUT recovery mechanism to exercise (nothing in tile.go can heal a wrong key).
Key-resolution semantics ("desktop exists" — by idx? by output+idx?) are
unspecified and decide which of the two degenerate behaviors you get. As
specced this validates harness wiring (see CW-6 — usefully!), not SUT
behavior. Suggested action: reclassify as a harness/deployment sanity
assertion, define the key predicate precisely, and drop the implication that
it tests a recovery property.

### PS-2. output-readd-tile-recovers — stranding detector is dead code under the only exercisable topology

Hotplug happens at the sway layer (where waybar's bars live); tile-watch keys
derive from niri outputs, which are fixed (`winit`). So the
`Always("every tile-watch key resolves to a key present in the daemon cache")`
can never be violated **by hotplug** in this harness — it only re-detects the
CW-6 misconfiguration. Additionally, the recovery `Sometimes` is constructible
only if the harness waybar config is output-**unpinned** (a config pinned to
`HEADLESS-1` yields no bar after re-add, per the property's own investigation)
— a harness-config requirement stated in the property but absent from the
topology's harness-config section. Suggested action: note the detector's
reduced role; add the unpinned-output requirement to the topology.

### PS-3. neighbor-modules-stay-live — the FIFO wedge lever ends the run for everything else

Verified: the `<icon src>` read is `std::fs::read` inside the
`ICON_CACHE.with(|c| c.borrow_mut().entry(..).or_insert_with(load))` closure
(src/lib.rs:1248-1252, 1300-1315). `fs::read` on a FIFO with no writer blocks
in open(2) **on the GTK main thread while holding the RefCell borrow** —
permanent, unrecoverable wedge of the entire bar. As a fault lever it proves
the property once and then invalidates every other property for the rest of
the run (no draws, no reloads, no teardown). Needs a dedicated run variant or
terminal-phase scheduling in the workload composition. The SUT stall-budget
Always additionally falls under CW-3 (CPU modulation is a default fault); the
canary `Sometimes` is correctly primary — recommend demoting the 250ms Always
to a `Sometimes(draw took longer than X)` telemetry hint (the catalog already
has one at 50ms) plus the canary.

### PS-4. stream-line-length-bounded — asserts a cap that doesn't exist, at a point that can't see the failure

Verified: no LINE_CAP anywhere in content.rs; the reader is plain
`BufReader::lines()` (src/content.rs:270). Two problems:

1. The `Always("line length within configured cap")` is placed "after each
   line" — it executes only when a line **completes**. The actual S1 scenario
   (newline-less producer growing one String toward OOM) never returns from
   `read_line`, so the assertion never observes it. Only the workload RSS
   ceiling (`stream-ingest-memory-bounded`) detects the real failure mode.
2. "Configured cap" implies a cap in code. None exists; the assertion would be
   the sole definition, firing on any completed >64KiB line — that's a
   spec-by-assertion, fine as a tripwire, but the property text reads as if it
   checks an implemented bound.

Suggested action: either accompany with the code change (capped read via
`take()`/manual `read_until` loop — then the assertion sits inside the loop
and does see partial growth), or re-scope the property to "completed-line size
tripwire" and let stream-ingest-memory-bounded own the exhaustion detection.

### PS-5. prompt-pulse-visibly-advances — the clock seam as described panics on a fresh container

`Engine.start` is an `Instant` (src/lib.rs:204); animation time is
`start.elapsed().as_secs_f32()` (src/lib.rs:244). An "env-var start-offset
into the Engine clock" implemented the obvious way
(`Instant::now() - Duration::from_secs(offset)`) **panics on underflow**:
`Instant` is CLOCK_MONOTONIC since boot, and an Antithesis container's uptime
at test start is minutes — you cannot subtract 97 days from it. The seam must
be additive in the time computation instead
(`start.elapsed().as_secs_f64() + offset` cast to f32, or offset added as f32
to reproduce the quantization at the target magnitude — adding a small elapsed
to an 8.4e6 f32 offset quantizes at ulp 1.0 exactly as a real 97-day uptime
would). Feasible, small — but the evidence should specify the additive form so
the workload author doesn't ship the panicking variant. Secondary note: under
Antithesis+llvmpipe slowdown, real frame intervals stretch, so the 36h regime
(ulp ~16ms) may stop being distinguishable from healthy behavior — the 12d/97d
regimes are the robust legs.

### PS-6. static-idle-redraw-budget — one leg tautological, the other circular

- The fps-cap `Always("animated tile redraw rate ≤ target fps")` would be
  placed beside the very throttle that enforces it (min_dt gate in the tick
  callback, src/lib.rs:346-359) — and it **passes during the actual target
  bug** (30fps forever on clamped-static `idle_level: 7` content is ≤ target).
  It catches nothing the property cares about unless it counts queue_draw from
  all sources (dirty poll, hover, GTK) — a different, harder instrumentation.
- The zero-redraws-when-static leg needs a "static" definition independent of
  the buggy `content_animates` gate — i.e. re-implementing the renderer's
  clamp (`level.parse().min(IDLE_LEVELS.len()-1)`, src/lib.rs:1048-1051) in
  the assertion. Implementable (~5 lines) but it is a second implementation of
  contested logic; anchor it to the shared constant and note the drift risk.
  The mechanism-level sibling `idle-level-gate-clamp-divergence` (gate verdict
  vs clamped level, cross-checking the two existing implementations against
  each other) is the sounder oracle and needs the same clamp helper — factor
  it once.

### PS-7. cffi-v1-config-transport-retype — SUT-side Always lacks ground truth

The plugin cannot know that a received Value was *authored* as a string —
retyping is invisible at the receiving end (that is the bug). Even the null
sub-case can't distinguish authored-`""` from retyped-null at the Rust side.
The `Always("string-typed module config values arrive as strings")` is
unimplementable as a SUT assertion without a harness convention supplying the
ground truth (e.g. a companion config key listing expected-string fields, or a
sentinel naming scheme the assertion checks). The practical oracle is
workload-side and overlaps `config-resolve-preserves-tile-identity` (tile
never collapses to the demo tile). P3 priority limits the damage, but as
written the headline assertion can't be placed.

### PS-8. cairo-text-survives-gl-failure / engine-init-failure-contained — variant-gating and missing injectors

- No EGL/`make_current` fault injector exists in the topology; the mid-run leg
  needs a SUT seam the catalog only raises as an open question. The
  deterministic lever is the environment variant (no surfaceless ICD /
  env unset), which kills GL for the **whole run** (EGL platform selection is
  process-wide) — so it must be a dedicated run, and all GL-dependent
  properties are dead in it. "Resource-pressure faults during init" is not a
  standard Antithesis fault; treat the env variant as the only reliable
  trigger.
- `Unreachable("engine absent while content markup is available")` fires
  constantly **by design** in that same variant (engine-less draws with live
  content are the variant's whole point). The two properties' assertions need
  per-variant gating (env flag compiled into the assertion condition) or the
  Unreachable is guaranteed noise. The catalog does not mention gating.

### PS-9. shader-recompile-gl-object-leak — RSS proxy is not viable; rate math doesn't survive the platform

The 216k-objects/hour figure assumes 30fps retry. Under llvmpipe software GL +
Antithesis instrumentation overhead, effective frame rates drop by an order of
magnitude or more; leaked GL objects under llvmpipe are small client-side heap
allocations. The workload `Always("RSS slope stays flat")` will be
noise-dominated at realistic run lengths — the property's own open question
("is the RSS proxy sensitive enough?") should be answered **no, SUT counter
mandatory** (wrap create/delete in counters; deletes don't exist, which is the
finding). The escalation endpoint (GL name exhaustion → unwrap → SIGABRT) is
effectively unreachable within run limits on llvmpipe (client-side names, no
hard ID ceiling) — keep the abort leg as documentation, not a run goal.

### PS-10. torn-ndjson-frame-rendered — doubly-rare precondition needs an explicitly weighted driver

The kernel-torn line needs (a) a >4096B line in flight — only via multi-KB
window titles (see CW-2 title plumbing; or crafted cache entries, which the
evidence file allows) — AND (b) SIGKILL landing inside the microsecond-scale
short-write loop window in tile-watch. The property is honest that this is
workload-weighted, but the topology's driver sketch has no correspondingly
weighted phase (continuous huge-title payload churn + high-rate producer
kills). Without it, expect zero coverage of the torn branch; the
`AlwaysOrUnreachable` form at least stays sound when unreached.

### PS-11. tile-watch-output-schema-valid — the validating tee perturbs what sibling properties count

Inserting a schema-validating tee into the producer exec makes each chain 3
processes, not 2 — breaking the count constants of
`reload-conserves-producer-chains` and `orphaned-tile-watch-bounded` — and
moves the pipe topology: tile-watch's stdout reader becomes the tee, so the
SIGPIPE death chain (which `orphaned-tile-watch-bounded` and
`producer-kill-tile-reconverges` reason from) now depends on the tee's
lifetime, not the plugin's. Either run the tee variant on the reserved tile
only, in a separate run, or adjust the counting/SIGPIPE-dependent properties'
constants. No document currently reconciles this.

### PS-12. cache-error-demotes-live-tile — the target branch must be created, not annotated

`RunWatch`'s `emit()` (tile.go:515-531) has no explicit `rerr != nil` arm —
the error case is the fall-through of `if tiles, rerr := ReadCache(path); rerr
== nil`. Placing the `Unreachable` requires restructuring emit() (add the else
arm; compare `last` against the per-watcher marshaled `emptyPayload(idx,
false)` — both in scope). Small and safe, but it is a code edit, not a drop-in
assertion; the "two designs preserved in evidence" should note it. Positive
finding: the hour-scale repair-suppression windows do **not** require
hour-scale runs — the assertion fires at the demotion instant; the windows
only describe user-visible persistence.

### PS-13. duplicate-line-rerender-idempotent — uniform comparison must canonicalize

`build_uniforms` collects from a HashMap (src/content.rs:143-151); Vec order
is arbitrary. The "name-equivalent uniforms" comparison must sort by name
before comparing or the assertion is flaky. Trivial, but must be in the
assertion spec.

## Passes (checked, correct)

- **Rust line anchors**: content.rs 270-276 (reader loop), 271 (break arm),
  88/100-115 (poison swallows), 258 (backoff); lib.rs 57-79 (refresh_shader),
  211-219 (two distinct degraded-init arms), 234-247 (two-lock draw reads —
  uniforms then markup, so "markup never lags uniforms" matches read order),
  255-260 (needs_gl; `<glow` correctly forces GL yet appears in neither
  animation detector — the extended oracle is right), 332-362 (could_anim +
  tick callback), 366-374 (dirty poll), 531 (single `markup::process` call
  site), 1023 (draw_status state attr), 1048-1051 (idle clamp), 1248-1252
  (icon fs::read inside the cache-miss closure — the pre-read
  stat assertion is placeable), 1300-1315 (negative cache; a hit-time re-stat
  oracle for the pin property is implementable). All verified at f87ec19.
- **`tick_installed` plumbing is easier than the catalog fears**: `could_anim`
  is a local in scope where the dirty-poll closure is constructed
  (lib.rs:333 vs 366) — capture it; no store changes needed.
- **Generation counter**: `Inner`/`TileContent` admit an AtomicU64 +
  generation-returning accessor variants without redesign; the draw callback
  and poll closure share `Rc`/clones, so rendered-generation plumbing is
  main-thread-local. Feasible as claimed.
- **Go placements**: `writeTiles` (daemon.go:397) has `tiles`, `lastTiles`,
  `d.model`, and `d.adopted` all in scope — the no-outputs escape hatch is
  directly implementable; `PayloadFor` post-truncation point exists
  (tile.go:171-181); `sessionTile` (tile.go:120) fine. `LoadLive` has no state
  filter and the sessions schema has no CHECK — DB state injection reaches
  payloads (subject to CW-2 window liveness).
- **Second-daemon fault is real and trivial**: no flock/pidfile anywhere;
  fixed `.tmp` name in `WriteCacheBytes` (tile.go:432); both daemons can share
  the niri socket. The workload "just starts one" — confirmed.
- **Fix branch exists**: `worktree-fix-gl-teardown-crash` at 30100f9 — run 2's
  precondition is buildable.
- **`pwetty render` exists** (src/bin/pwetty.rs) and uses the live compose
  path (`render_png`) — valid reference-render generator, with the
  animated-phase caveat in CW-1.
- **In-container observability**: process/thread counts (/proc), RSS
  (/proc/PID/status), cumulative forks (/proc/stat `processes`), watcher keys
  (/proc/PID/cmdline) — all workload-readable in the single-container design.
- **Supervisor substitution for node-termination**: sound for every kill-based
  property; `orphaned-tile-watch-bounded` correctly flags that the supervisor
  must NOT kill waybar's process group (a supervisor design requirement the
  topology should pin down explicitly).
- **No property needs network faults** — the single-container decision is
  right for this SUT; nothing in the catalog contradicts it.
- **Backoff floor is pause-safe**: a lower-bound spacing assertion only widens
  under scheduler faults — the one timing Always immune to CW-3.
- **Hour-scale/97-day concerns are properly seamed or event-anchored**: the
  f32 legs go through the (fixable, see PS-5) SUT seam; the cache-demotion
  property fires at the event, not after the staleness window; INK_CACHE
  slow-burn is already flagged as possibly documentation-only.

## Uncertainties

- **Thread-pause granularity**: whether Antithesis scheduler exploration can
  land pauses in ~2-instruction windows (publish-visible,
  content-snapshot-torn-read rely on it), and whether pausing requires SDK
  coverage instrumentation as the topology asserts — not verifiable from this
  environment; a probe run settles it. The properties remain sound if the
  windows are hit rarely (Sometimes companions just fire late).
- **Slowdown factor** of the GTK+llvmpipe stack under the Antithesis
  hypervisor — scales every fps-derived estimate (PS-9 leak rates, 30fps
  regimes, screenshot cadence). Unmeasurable outside a probe run.
- **Clock-jitter semantics** (does it move CLOCK_MONOTONIC) — already a
  catalog-wide open question; the planning default (SUT seam) is right.
- **C0 / multi-KB titles through Wayland/niri plumbing** — decides whether
  title-vector properties run end-to-end or only via direct cache injection;
  needs one local probe with a title-setting client.
- **Whether `swaymsg create_output`/`unplug` hotplug behaves identically under
  the Antithesis hypervisor** — the sway-outer design is evidence-backed on
  the host; its container behavior is a build-time verification item, not
  assessable from code.
