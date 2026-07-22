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

# Property Catalog — waybar-pwetty-box

51 properties: 41 from an 11-focus discovery ensemble (deduplicated, merged,
refined by a 6-agent open-questions investigation pass) plus 10 post-evaluation
gap-fill properties (Category 7). Of the original 41, one is retired to a
shared oracle and one moved to a deterministic pre-flight (see Post-Evaluation
Amendments) — 49 remain active Antithesis properties. Each property has an
evidence file at `properties/{slug}.md`. Every suggested assertion is
**net-new** — neither repo has any Antithesis instrumentation
(`existing-assertions.md`, both repos scanned).

Severity references (S1 host crash, S2 silent staleness, S3 attention failure,
S4 visible error, S5 cosmetic/heat) and attack-surface numbers refer to
`sut-analysis.md`. Priorities: P0 (run first) → P3.

**Fault requirements legend** — ⚠️custom-fault: needs a custom fault script
(SIGUSR2 injection, file mutation, second daemon); ⚠️clock: needs
clock/virtual-time manipulation (or the SUT-side clock seam). Node-termination
faults are NOT required by any property: the topology's in-container
supervisor makes all kills ordinary process faults.

Key investigation results baked in below: waybar 0.15.0 SIGUSR2 reload
destroys/re-creates every CFFI module **unconditionally** (no config diffing;
destroy-all-then-construct-all; fresh dlopen per module); output re-add
auto-recreates bars; niri renumbers workspace indexes on output removal; the
Go SIGPIPE death chain is live-repro-confirmed (exit 141); `sh -c` does NOT
exec-replace under dash (2 processes per producer chain — count constants,
not invariants); `<glow>` is empirically time-varying and ungated by both
animation detectors; Pango accepts C0-carrying fallbacks (no host-abort leg);
the live config's module_path points at raw target/release with a
nonexistent "pinned copy" mitigation.

## Post-Evaluation Amendments (2026-07-22 — override entry text where they conflict)

A 4-lens evaluation (`evaluation/*.md`) produced these cross-cutting rules
and per-property amendments, applied catalog-wide:

**Rule 1 — No raw wall-clock bounds in fault-exposed `Always` assertions.**
CPU modulation, thread pausing, and node hang legitimately stretch every
schedule; a wall-clock-bounded Always blames the fault injector. Every such
bound (`producer-kill` 5s, `stream-recovery` 10s, `prompt-priority` 3s,
`animating-gate` 500ms, `neighbor-modules` 250ms) is re-expressed as either
(a) event-counted units that pause with the observer (poll iterations,
draws, daemon ticks — `publish-visible-within-poll-bound` is the model), or
(b) quiesce-then-check inside `ANTITHESIS_STOP_FAULTS` windows /
`eventually_converged.sh`. `respawn-backoff-floor-holds` is exempt (a lower
bound that pausing only widens). The sharpest instance: `animating-gate`'s
own prescribed fault (pause inside `set()`) violates its 500ms bound with
zero defect — its oracle moves to quiesce-then-check plus a direct
dirty-before-unlock ordering assertion.

**Rule 2 — Trigger-class tags and a second priority axis.** Each property's
trigger is one of: `interleaving` (only scheduler/fault composition reaches
it), `fault-composition` (workload action × fault timing),
`deterministic-input` (fixed input reaches it first try), or
`environment-variant`. Severity-priority (P0-P3) is unchanged, but workload
*search budget* follows the trigger class: exploration-dependent properties
(`torn-ndjson-frame-rendered`, `so-replacement-reload-race`,
`orphaned-tile-watch-bounded`, `content-snapshot-torn-read`,
`publish-visible-within-poll-bound`, `cache-error-demotes-live-tile`,
`daemon-restart-no-placeholder-clobber`) get the run-2 drivers;
deterministic-input properties (~16, e.g. the injection/config/idle-level
set) become ride-along tripwires plus a **deterministic pre-flight** (cargo
test / integration script) that verifies them outside Antithesis. The
pure-function subset (`embed-placeholder-parity`,
`no-control-chars-in-pango-markup`, `duplicate-line-rerender-idempotent`,
`cffi-v1-config-transport-retype`, the idle-level table) moves to that
pre-flight entirely.

**Rule 3 — Reachability audit / variant gating.** Every
`Sometimes`/`Reachable` names the driver or environment variant that makes
it satisfiable and is env-gated to it; unsatisfiable-today anchors (the
make_current-failure Reachable, the icon-recovery Sometimes) are demoted to
log lines until their seams/fixes exist. The
`Unreachable("engine absent while content markup is available")` in
`cairo-text-survives-gl-failure` is gated OFF in the GL-degraded environment
variant (it contradicts `engine-init-failure-contained`'s deliberate state
by design).

**Rule 4 — No nonexistent fault types in Antithesis Angles.** Antithesis has
no fs/EIO, fs-latency, memory-pressure, or fd-exhaustion fault injection.
Angles citing them are re-grounded on: workload file mutation (swap, unlink,
FIFO, permission flip), environment variants (no Mesa surfaceless ICD), and
the SUT-side **GL failure seam** (topology). Two-sided count equalities
(== not ≤) everywhere — a dead reader chain must violate, not satisfy, the
conservation properties.

**Per-property amendments:**
- `watcher-key-survives-output-rename` — **retired as a standalone
  exploration property** (post-re-scope it is a self-inflicted
  misconfiguration with no SUT recovery to test; deterministic
  demonstration). Its Always survives as the shared stranding oracle inside
  `eventually_converged.sh`; the no-recovery behavior goes to the owner as
  an accept-or-fix artifact.
- `duplicate-line-rerender-idempotent` — **moved to the deterministic
  pre-flight** (purity is a render-twice-compare unit test; the SUT-side
  memcmp machinery outweighed the invariant). Its respawn-duplicates
  `Sometimes` transfers to `producer-kill-tile-reconverges`. Uniform
  comparison must sort by name (HashMap iteration order).
- `neighbor-modules-stay-live` — the canary (event-counted, quiescent-window)
  is the property; the 250ms stall-budget Always demotes to a fault-gated
  diagnostic (llvmpipe + CPU faults blow it on correct code, and it is
  vacuous for the permanent wedge it most wants).
- `prompt-pulse-visibly-advances` — primary oracle becomes the windowed form
  in the dirty-poll callback: `Always("pulse phase advanced within K polls
  while prompt markup is displayed")` (the once-per-run Sometimes cannot
  detect freeze-after-first-second — the exact historical failure shape).
  The f32-quantization regimes move to a seeded unit test via the clock seam
  (which must be an **additive offset in the time computation**, not an
  Instant start-offset — underflow panic); the property keeps the six-link
  integration under faults and loses its ⚠️clock dependency.
- `stream-line-length-bounded` — the asserted LINE_CAP does not exist in
  code; the property is re-scoped as **assert-after-fix** (the capped-read
  change is the property's price of admission; until then the RSS ceiling in
  `stream-ingest-memory-bounded` carries the risk).
- `cffi-v1-config-transport-retype` — the SUT-side Always lacks ground truth
  (retyping is invisible to the receiver); needs a harness expected-types
  manifest the assertion compares against. Pre-flight territory (Rule 2).
- `static-idle-redraw-budget` — the fps-cap leg is near-tautological (sits
  beside the throttle that enforces it and passes during the target bug);
  the static-leg oracle must share one clamp helper with
  `idle-level-gate-clamp-divergence` rather than re-implement the buggy
  logic. Scope: "no pointer interaction in flight" (hover legitimately
  queues redraws on a static tile — the harness has no pointer today, but
  the assertion must not false-positive if input injection is ever added).
- `shader-recompile-gl-object-leak` — the RSS proxy is not viable (llvmpipe
  slowdown collapses the leak rate; objects are small heap allocs): the SUT
  counter is mandatory, and the SIGABRT exhaustion endpoint is not
  reachable in-run (documentation, not expectation).
- `cache-error-demotes-live-tile` — the "demote branch" is a fall-through,
  not a branch, in `emit()`; instrumentation requires a small restructure
  (extract the error path) — noted so `antithesis-workload` budgets it.
  Positive: the assertion fires at the demotion instant, so hour-scale
  suppression windows need no hour-scale runs.
- `producer-kill-tile-reconverges` + `prompt-priority-survives-session-cap`
  + `output-readd-tile-recovers` + `stream-recovery-after-framing-violation`
  — "rendered/published markup" oracles re-anchor on the **markup export
  seam** (topology); pixels cannot express them for animated tiles.

---

## Category 1 — Host survival (S1)

### [module-teardown-never-aborts-host] — Module teardown never aborts the waybar host

*Found by: lifecycle. Priority: **P0**. ⚠️custom-fault (SIGUSR2, output unplug).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Destroying a pwetty module instance (output removal, SIGUSR2 reload, waybar exit) never terminates the waybar process. |
| **Invariant** | Workload `Always("waybar host survives module teardown")` — waybar PID persists (or exits cleanly, no SIGABRT/core) after each teardown trigger. Vacuity guard: workload `Sometimes("a module teardown was triggered while tiles were live")`. SUT-side (lands with the fix branch's `impl Drop for Engine`): `Reachable("engine teardown entered with no GL context current")`, `Sometimes("engine teardown completed without abort")`, `Reachable("engine teardown leaked the canvas because make_current failed")`. |
| **Antithesis Angle** | Trigger diversity at arbitrary timing vs draws and stream publishes. SIGUSR2 reload is a source-confirmed trigger of the same widget-dispose leg as output removal (waybar reload is unconditional destroy-all → rebuild); output unplug is exercisable via the sway-outer-stack topology (`swaymsg output unplug`). At f87ec19 fails deterministically (gdb-confirmed p9c). Post-fix, hunts residual destructor paths and confirms/refutes q9y. |
| **Why It Matters** | The confirmed S1 crash: one lid-close kills every bar on the desktop. Fix unmerged (branch `worktree-fix-gl-teardown-crash`, 30100f9). |

**Open Questions:**
- None. (Reload semantics and harness hotplug both resolved by investigation; only the historical q9y *attribution* stays open, and this property is itself the confirm/refute vehicle.)

### [engine-init-failure-contained] — Engine init failure degrades without aborting

*Found by: lifecycle. Priority: P1.*

| | |
|---|---|
| **Type** | Safety (containment) + Reachability |
| **Property** | When offscreen GL / renderer init fails at `wbcffi_init`, the module still initializes, no panic escapes the FFI boundary, and every engine-less draw completes safely. |
| **Invariant** | SUT `AlwaysOrUnreachable("draw callback completed with engine absent")`. SUT `Reachable("module init degraded to engine-less mode: offscreen GL init failed")` and `Reachable("module init degraded to engine-less mode: renderer init failed")` (distinct arms, lib.rs:211-219). Workload `Always("waybar stays alive after engine init failure")`. |
| **Antithesis Angle** | Environment variant without Mesa surfaceless (or the SUT-side GL-failure seam) makes the degraded path deterministic; then the full workload exercises engine-less draws, hovers, reloads, teardown. |
| **Why It Matters** | Containment protects S1; the ceiling it buys (whole-bar blank including CPU-renderable text) is surfaced as `cairo-text-survives-gl-failure`. |

**Open Questions:**
- Is blank-including-CPU-text accepted-by-design or a defect? `(needs human input)`
- Can `Renderer::new` fail when `OffscreenGl::new` succeeded? If effectively unreachable, the second `Reachable` demotes to documentation.

### [neighbor-modules-stay-live] — Neighbor modules stay live under plugin torture

*Found by: wildcard. Priority: P1.*

| | |
|---|---|
| **Type** | Liveness + stall-budget Safety |
| **Property** | While the plugin's data chain and draw path are under fault, waybar's shared GTK main loop keeps servicing the rest of the bar; the plugin's draw callback never exceeds a stall budget. |
| **Invariant** | SUT `Always("the pwetty draw callback completes within its stall budget")` (~250ms, to calibrate). Workload `Sometimes("canary module content changed while plugin faults were active")` — a co-hosted 1s clock module keeps updating; the canary is the primary wedge detector (investigation confirmed GTK3 hard-no-ops draws for unmapped widgets, so the SUT-side Always is structurally vacuous in unmapped windows — keep the bar always mapped in the harness). Hint: `Sometimes("a draw took longer than 50ms")`. |
| **Antithesis Angle** | Workload file mutation on the per-draw retry paths (broken background-shader file re-read per frame; `<icon src>` swapped to a FIFO or huge file); a FIFO icon path wedges the loop permanently; 10 instances share one loop. |
| **Why It Matters** | Claim S2 ("slow command never blocks the bar") is implemented for the command path only; the draw path re-imports synchronous I/O. Umbrella over `icon-src-read-bounded-nonblocking` and `stream-line-length-bounded`. |

**Open Questions:**
- Right stall budget; scale with instance count?
- Screenshot cadence for the canary under llvmpipe `(partial: SUT-side draw counters are the primary oracle per the frame-clock investigation; screenshots are secondary)`

### [icon-src-read-bounded-nonblocking] — Icon src read is bounded and non-blocking

*Found by: security. Priority: P1.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | The data-controlled `<icon src>` filesystem read never blocks the GTK main thread on a non-regular file and never reads unbounded data. |
| **Invariant** | `AlwaysOrUnreachable("draw-path <icon src> targets a regular file within size cap")` placed **before** the `fs::read` (src/lib.rs:1248-1252). Companion `Reachable("draw-path <icon src> filesystem read executed")`. |
| **Antithesis Angle** | Fs fault injection / workload data makes the path a FIFO, device, socket, or huge file; the read is synchronous in the draw callback on the one main thread. |
| **Why It Matters** | `app_icon` is a data field piped straight into `src` (tiles/claude/tile.json:7); zero validation exists. One bad path freezes the whole bar (S1). |

**Open Questions:**
- Bounded synchronous read acceptable, or move off the main thread entirely? `(needs human input)`
- Can a crafted niri `app_id` drive the backend's `resolveAppIcon` (unsanitized `filepath.Join`, tile.go:364) to emit a traversing path?

### [stream-line-length-bounded] — Stream line length is bounded

*Found by: security. Priority: P1.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | The stream reader never buffers an unbounded amount of stdout for a single line; a newline-less or oversized producer cannot exhaust host memory. |
| **Invariant** | `Always("stream reader line length within configured cap")` after each line (src/content.rs:270-276): `line.len() <= LINE_CAP` (64 KiB — investigation measured realistic max ~550B and ~6KB for pathological titles, so 64KiB has ~120× headroom). |
| **Antithesis Angle** | Producer emits a giant line or withholds `\n`; `BufReader::lines()` grows one String to EOF uncapped. |
| **Why It Matters** | In-process: unbounded line growth is unbounded growth of the whole bar. Mechanism-level counterpart of `stream-ingest-memory-bounded`. |

**Open Questions:**
- Can a block-buffered producer stall a partial line under the cap without completing? (Bounded-but-stalled vs bounded-but-delayed.)

### [stream-ingest-memory-bounded] — Producer output ingestion is memory-bounded

*Found by: resource-boundaries. Priority: P1.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | No producer behavior (newline-less output, never exiting, hostile volume) grows waybar's memory without bound through the plugin's ingestion paths. |
| **Invariant** | Workload `Always("waybar RSS stays below ceiling while a newline-less stream producer runs")` and `Always("waybar RSS stays below ceiling with a non-terminating poll-mode exec")` (distinct paths: `lines()` String vs `Command::output()` Vec). Companion `Sometimes("stream reader consumed a line larger than PIPE_BUF")`. |
| **Antithesis Angle** | Workload swaps the tile exec for adversarial producers; the poll-mode variant is reachable by a one-line config mistake (`stream: false` is the default). |
| **Why It Matters** | Host OOM kills every bar (S1 via exhaustion). |

**Open Questions:**
- Fair RSS ceiling for the harness topology — needs a calibration run.
- Is slow-burn INK_CACHE/ICON_CACHE growth observable within a compressed run, or documentation-only?

### [so-replacement-reload-race] — Host survives plugin .so replacement racing reload

*Found by: version-compat. Priority: P2 → **elevated interest**: investigation confirmed the hazard is production-real (all 20 live modules point module_path at raw target/release; the config comment's "pinned copy + pwetty-promote" mitigation does not exist anywhere). ⚠️custom-fault (file mutator + SIGUSR2).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Replacing `libpwetty_box.so` on disk concurrent with SIGUSR2 reloads never kills waybar; a later reload recovers the module. |
| **Invariant** | Workload `Always("waybar survives plugin shared-object replacement racing reload")`; `Sometimes("reload dlopened the plugin mid-replacement and skipped the module")`; `Sometimes("plugin reloaded successfully from the replaced shared object")` (build-stamped .so; should distinguish fully- vs partially-populated new bars — modules dlopen sequentially, so a mid-pass replacement yields **mixed versions within one generation**). |
| **Antithesis Angle** | File mutator in three modes — atomic rename, unlink+slow-rewrite (cargo- and cross-fs-`go install`-shaped; old inode never truncated), in-place truncate (bare-`cp`-shaped, risks delayed SIGBUS of the mapped inode). Reload is destroy-all-then-construct-all (source-confirmed), so generations never coexist — the race is per-module within one construction pass. |
| **Why It Matters** | Every rebuild races the mapped .so in the real deployment. Never-dlclose also means each upgrade cycle adds a ~7.7MB resident plugin copy. |

**Open Questions:**
- Does dlopen of a partially-written .so always fail cleanly, or can a tear after valid ELF headers map and fault later?
- Are cold plugin pages faulted late (workload should fault cold paths post-replacement)?

---

## Category 2 — Truthful display and staleness (S2)

### [cache-error-demotes-live-tile] — A cache read error never replaces live state with the idle placeholder

*Found by: data-integrity + coordination (merged). Priority: **P0**.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | tile-watch never publishes the empty placeholder as a *replacement* for previously-published live state merely because a cache read failed (startup placeholder before any successful read is legitimate). |
| **Invariant** | **Committed (post-review): Design A** — Go `Unreachable("tile-watch: cache read error demoted a live published payload to the empty placeholder")` on the condition `rerr != nil && last is non-placeholder` (tile.go:515-531; the error case is a fall-through today — extract it into a branch first, see Amendments R14). Chosen over the conditioned-`Always` alternative (kept in the evidence file) because `Unreachable` survives both candidate fix shapes (keep-last and daemon self-repair). Companion `Sometimes("tile-watch: cache recovered after a failed read")`. **Expected to fail at current code.** |
| **Antithesis Angle** | Kill/restart the daemon around its 250ms-throttled writes while tile-watch polls at 75ms; delete tiles.json while a session sits in `prompt`. Repair-suppression windows (investigated): ≤~60s while any idle session is under an hour old; up to ~1h past an hour; **unbounded** for `prompt` (no time-varying field) and for idle-with-NULL-last_talk_ts (reachable via Notification-created rows). |
| **Why It Matters** | The product's sole alert silently replaced by plausible idleness (F9). The one-shot sibling `tile-data` falls back to a live query; the streaming path got the strictly worse fallback. |

**Open Questions:**
- Is degrade-to-placeholder on read error intended beyond the daemon-never-ran case? `(needs human input)`

### [daemon-restart-no-placeholder-clobber] — Restarted daemon never clobbers a populated cache with placeholders

*Found by: failure-recovery. Priority: P1.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | A restarting daemon never overwrites a populated tile cache with an empty map while outputs exist; a desktop with a live `prompt` session never regresses to the idle placeholder across a daemon restart. |
| **Invariant** | SUT Go `Always("daemon never publishes an empty tile cache over a populated one while outputs exist")` in `writeTiles` — **with a no-outputs escape hatch** (investigation confirmed niri legitimately reports zero workspaces on headless start / all-outputs-disconnected; condition on the adopted model / last WorkspacesChanged payload, and tolerate `output: null`). Workload `Always("prompt desktop key never regresses to idle placeholder across daemon restart")` while a fixture session holds `prompt`. Coverage guard `Sometimes("daemon restarted while a prompt session was live")`. |
| **Antithesis Angle** | Race: DB prime (immediate) → first `reconcile` → `writeTiles` (first-write throttle bypass is structural) vs the async niri event-stream child delivering the initial model. Scheduling faults invert the ordering; SIGKILL inside the window makes the clobber permanent (no restart unit ships — confirmed, none exists at all). |
| **Why It Matters** | Converts a restart into an active false-state push; textbook recovery-assumes-clean-state. |

**Open Questions:**
- Race-window width on real hardware `(partial: ordering confirmed from code; absolute timings unmeasured)`

### [producer-kill-tile-reconverges] — Tile reconverges to cache truth after arbitrary-timing producer kill

*Found by: failure-recovery. Priority: P1.*

| | |
|---|---|
| **Type** | Liveness |
| **Property** | After a `tile-watch` producer is killed at any point, the rendered content reconverges to current `tiles.json` truth within bounded time. |
| **Invariant** | Workload `Always`: after killing a tile's producer, published markup matches markup rendered from the current cache payload (quiesce-then-check per Rule 1; mechanism bound ~1.5s). SUT `Sometimes("tile content reconverged to cache truth after producer kill")` as recovery replay anchor. Transferred from the pre-flighted `duplicate-line-rerender-idempotent`: `Sometimes("duplicate payload line was re-delivered by a respawned producer")` (replay-coverage anchor). |
| **Antithesis Angle** | Kill timing diversity: clean EOF, mid-write (torn line → empirically confirmed red template-error card, minijinja Lenient default — semver-stable), kills inside the backoff window. |
| **Why It Matters** | Reconvergence hangs entirely on tile-watch's fresh-`last` initial emit; a regression silently degrades to "recover on next payload change". Tests claims L2/S12. |

**Open Questions:**
- None. (Line-size and minijinja questions resolved by investigation.)

### [stream-recovery-after-framing-violation] — Stream reader recovers from a framing-violating line from a live producer

*Found by: protocol-contracts + concurrency + failure-recovery (3-way merge). Priority: P1.*

| | |
|---|---|
| **Type** | Liveness |
| **Property** | After a line violating the NDJSON framing contract (invalid UTF-8, garbage, oversized) from a producer that keeps running, a subsequent valid line is rendered within bounded time. |
| **Invariant** | Workload `Always("stream applies a valid line within 10s of a framing-violation line")`. SUT `Reachable("stream reader entered its line-error abort path")` at the `else break` arm (content.rs:271); `Sometimes("stream reader respawned producer after line decode error")` after `child.wait()` returns. Assertion 1 firing without assertion 2 = the hang. |
| **Antithesis Angle** | Byte corruption via a wrapper producer; timing between the garbage line and the next data change. **Known-failing at f87ec19**: reader breaks → blocks in `child.wait()` until the child's next write. The SIGPIPE death chain is live-repro-confirmed (exit 141, 26ms after a forced cache change) — so the stall is bounded by the next payload change and unbounded only on a never-changing payload (quiet desktop). |
| **Why It Matters** | First-run bug-finder; falsifies S12/L2 for the decode-error path. One-line fix candidates exist; assertions written to survive the fix. |

**Open Questions:**
- Can invalid UTF-8 reach the pipe from the real producer? `(partial: Go json.Marshal replaces invalid UTF-8 with U+FFFD — trigger needs a wrapper producer; the plugin's contract is with any exec producer, so the property stands)`
- Should the never-changing-payload wedge be filed as a bug rather than only a property? `(needs human input)`

### [watcher-key-survives-output-rename] — Watcher keys track output identity

*Found by: coordination. Priority: P1 → re-scoped by investigation.*
**STATUS: RETIRED as a standalone property (post-evaluation, R7) — its Always survives as the shared stranding oracle in `eventually_converged.sh`; the no-recovery behavior is an accept-or-fix artifact for the owner.**

| | |
|---|---|
| **Type** | Liveness |
| **Property** | Every live `tile-watch` reconverges with the daemon's cache keying after an output-identity change — no reader is permanently partitioned from the writer by a stale connector name. |
| **Invariant** | Workload `Always("every live tile-watch key resolves in the tile cache when its desktop exists")` at quiescent checkpoints (track desktops by stable workspace `id`, not positional `idx` — niri renumbers indexes on output removal, confirmed from source). Coverage `Sometimes("output topology changed while watchers were live")`. |
| **Antithesis Angle** | **Re-scoped**: output *rename* is impossible against real niri in any harness stack (winit output name is hardcoded; sway-outer hotplug changes sway's outputs, not niri's) — the exercisable variant is the **wrong-`--output` misconfiguration** (spawn watchers with a stale/wrong connector name), which produces the identical permanent-partition end state. The live config's mixed wiring (one bar flagless — working only because the hardcoded default matches; one bar pinned) makes this misconfiguration class production-plausible. |
| **Why It Matters** | Every tile silently renders idle placeholder forever with every process healthy (F10). Doubles as harness deployment sanity check. |

**Open Questions:**
- None. (Both prior questions resolved; the rename *trigger* moved out of scope, the misconfiguration variant carries the property.)

### [output-readd-tile-recovers] — Tiles recover after an output remove/add cycle

*Found by: lifecycle. Priority: P2. ⚠️custom-fault (sway-outer hotplug); assumes p9c fix merged.*

| | |
|---|---|
| **Type** | Liveness |
| **Property** | After an output remove + re-add cycle, waybar re-creates the bar (auto-recreation confirmed from waybar source), fresh modules spawn new producer chains, and every tile on that output converges to live backend content. |
| **Invariant** | Workload `Sometimes("tile on a re-added output rendered live session content after an output remove/add cycle")`; coverage `Sometimes("an output was removed and re-added while sessions were live")`; workload `Always("every tile-watch key resolves to a key present in the daemon cache")` as the stranding detector. |
| **Antithesis Angle** | Exercisable via the sway-outer-stack topology (`swaymsg create_output` / `output unplug`; wlroots re-adds get new `HEADLESS-%d` names by default — rename-by-default is a feature here). Two distinct failure shapes (investigated): config pinned to a renamed connector → **no bar at all** (waybar creates bars only for config-matching outputs); flagless wiring → tiles strand on the stale key. Repeated cycles compound with the reload leak. |
| **Why It Matters** | Lid/dock cycles are the daily-driver event; today step 1 is the p9c crash, and post-fix the recovery path has never been tested. |

**Open Questions:**
- None. (Hotplug feasibility, waybar auto-recreation, and live-config wiring all resolved by investigation.)

### [cold-start-stream-tile-converges] — Stream tile converges after bar-before-daemon startup

*Found by: lifecycle. Priority: P2.*

| | |
|---|---|
| **Type** | Liveness |
| **Property** | A stream tile that starts before the daemon's first cache write shows the empty placeholder, then converges to live session content once the daemon writes. |
| **Invariant** | Workload `Sometimes("stream tile converged from placeholder to live session content after a late daemon start")` — checked as "only placeholder-shaped lines until the first populated line" (investigation: the first-ever write has the same empty-model race as restart but is consumer-invisible — `{}` yields the same placeholder, deduped; expect a single placeholder→content transition). Ordering anchor `Sometimes("bar started before the daemon's first cache write")`; SUT `Sometimes("tile content transitioned from empty to populated")` in `ContentStore::set`. |
| **Antithesis Angle** | Startup-order exploration; catches the silent-failure holes that all present as "placeholder forever". |
| **Why It Matters** | The honest form of claim L9; a `prompt` session existing before login must surface after the daemon comes up. |

**Open Questions:**
- None.

### [poll-mode-cold-start-converges] — Poll-mode tile eventually reflects post-init backend state

*Found by: lifecycle. Priority: P2. Deliberately-failing gap demonstrator.*

| | |
|---|---|
| **Type** | Liveness |
| **Property** | A poll-mode tile eventually reflects backend state written after the bar started — violated by construction under the default `interval: 0`. |
| **Invariant** | Workload `Sometimes("poll-mode tile re-published content reflecting a cache write that happened after module init")` — with `interval: 0` this can never fire (the finding); with `interval: 2` (control variant) it must fire. Ordering anchor `Sometimes("poll-mode one-shot ran before the daemon's first cache write")`. |
| **Antithesis Angle** | Startup-order exploration guarantees the one-shot-before-daemon interleaving. |
| **Why It Matters** | One missing `"stream": true` line silently lands on defaults → tile stale forever (S2). Concrete accept-or-fix artifact. |

**Open Questions:**
- Is `interval: 0` + `exec` supported for live data or out-of-contract? `(needs human input)`
- Any coverage hole between this and `stream-ingest-memory-bounded` for the never-exiting-producer-under-poll case?

### [duplicate-line-rerender-idempotent] — Identical payload line re-delivery derives identical tile content

*Found by: idempotency. Priority: P2.*
**STATUS: MOVED to the deterministic pre-flight (post-evaluation, R8) — purity is a render-twice-compare unit test (sort uniforms by name); the respawn-duplicates `Sometimes` transferred to `producer-kill-tile-reconverges`.**

| | |
|---|---|
| **Type** | Safety |
| **Property** | Re-delivering an identical stream payload line derives byte-identical markup and name-equivalent uniforms — displayed content is a pure function of (last line, config), independent of delivery history. |
| **Invariant** | SUT `Always("re-delivered identical stream line derives identical tile content")`. Companion `Sometimes("duplicate payload line was re-delivered by a respawned producer")`. |
| **Antithesis Angle** | Producer kills generate duplicates naturally; scheduling jitter varies duplicate timing vs the dirty poll and frame clock. |
| **Why It Matters** | The sender-side dedup chain is only sound if rendering is pure — verified pure at f87ec19; pins the invariant against future time/random template dependence. |

**Open Questions:**
- Should the plugin add receive-side dedupe (memcmp in `set()`)? A crash-looping producer converts guaranteed duplicates into a ~1Hz redraw stream on a static tile.

### [torn-ndjson-frame-rendered] — The plugin never renders content derived from a partial NDJSON line

*Found by: data-integrity. Priority: P2.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Every stream line the plugin renders was emitted by the producer as one complete line — a torn final line never reaches the render path as content. |
| **Invariant** | `AlwaysOrUnreachable("stream reader: NDJSON line from the structured producer parsed as a complete JSON object")` at `parse_data` (content.rs:275), gated by a harness config flag. Keep render-side (investigation: do NOT switch to a producer-side line≤4096 Always — the workload's long-title lever would fail it by design). Companion `Sometimes("stream reader: producer respawned and re-emitted after EOF")`. |
| **Antithesis Angle** | Pipe writes ≤4096 bytes are atomic; Go loops on short writes for larger lines — SIGKILL between iterations leaves a torn prefix. Measured: realistic payloads ~550B; >4096B reachable via untruncated long titles (verified whole 6099B line emitted) — workload-weighted, not production-dominant. |
| **Why It Matters** | For ~1s the tile shows a template-error card or garbage derived from bytes no producer emitted as a frame. |

**Open Questions:**
- niri/Wayland-side title length caps `(partial: tile.go confirmed untruncated; compositor-side limits unchecked)`

### [tile-watch-output-schema-valid] — Every tile-watch line is one complete, schema-valid JSON document

*Found by: protocol-contracts. Priority: P2. ⚠️custom-fault (second daemon).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Every producer line parses as exactly one complete JSON document and validates against `tiles/claude/schema.json` under all fault conditions. |
| **Invariant** | Workload `Always` via a validating tee: `"tile-watch emitted one complete JSON document per line"` and `"tile-watch line validates against the claude tile schema"`. SUT Go `Sometimes("tile-watch substituted the empty placeholder for an unreadable cache")`. |
| **Antithesis Angle** | Delete/corrupt tiles.json mid-run; kill daemon mid-write; **start a second daemon** (confirmed legitimate: no unit, no flock, no pidfile anywhere — the workload just starts one); disk-full partial writes. |
| **Why It Matters** | Two semantic holes confirmed real: empty niri `app_id` (protocol-optional `set_app_id` → JSON null → Go "" → required key omitted; renders as a normal-looking card with a blank label — silent degradation, not an error card) and non-enum state under version skew. |
| |

**Open Questions:**
- None. (Empty-app_id reachability, two-daemon realism, and the minijinja rendering all resolved by investigation.)

### [unknown-session-state-renders-blank] — Session state alphabet matches the schema enum at both ends

*Found by: protocol-contracts. Priority: P2.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Every `sessions[].state` entering the payload and every state reaching the renderer is in the known alphabet — a live session is never silently displayed with no status indicator. |
| **Invariant** | SUT Go `Always("session state entering the tile payload is a schema enum value")` in `sessionTile` (tile.go:120). SUT Rust `Always("status tag reached the renderer with a known state")` at `draw_status` entry (lib.rs:1023). Workload `Sometimes("unknown-state payload traversed the chain to the renderer")`. |
| **Antithesis Angle** | Investigation confirmed current code provably writes only `{working, prompt, idle, shell}` — this is a **version-skew/corruption guard**; the workload injects via direct DB writes (no CHECK constraint, no Valid() on read). Unknown sorts LAST under the session cap — droppable before idle. |
| **Why It Matters** | The enum is enforced at zero of four hops; backend bug history clusters in state derivation. Failure is invisible: indicator is a blank gap, no log. |

**Open Questions:**
- Should the renderer's `empty` state be added to the schema enum? `(needs human input)`
- Desired rendering for unknown state — blank, `?`, or explicit glyph? `(needs human input)`

### [embed-placeholder-parity] — Placeholder count in processed markup equals extracted embed count

*Found by: data-integrity + security (merged). Priority: P2.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | After `markup::process`, the number of U+FFFC placeholders in the processed markup equals `embeds.len()`. |
| **Invariant** | `Always("markup process: embed placeholder count in processed markup equals extracted embed count")` at the single `process()` call site. |
| **Antithesis Angle** | `escape()` passes U+FFFC through; a data string in flow text (folder = cwd basename) injects a phantom placeholder — session B's row renders session A's status mascot; the last real embed silently vanishes. Trigger reachable via `mkdir` with U+FFFC in the name. |
| **Why It Matters** | Silent wrong-attribution on screen; no crash path (all consumers use `.get()`). |

**Open Questions:**
- Can U+FFFC survive niri's title/app_id plumbing, or is the folder vector the only realistic trigger? `(partial: plugin mechanism code-certain; niri-side sanitization unchecked)`

### [no-control-chars-in-pango-markup] — No control chars reach Pango

*Found by: security. Priority: P2. Severity settled at S2 by investigation (no host-abort leg).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Markup handed to Pango never contains XML-forbidden C0 control characters — so structured content is never collapsed by control bytes in data. |
| **Invariant** | `Always("Pango markup contains no XML-forbidden C0 control character")` at the Pango boundary: no C0 other than `\t \n \r`. |
| **Antithesis Angle** | C0 bytes in title data pass both escape layers, roxmltree rejects the parse → whole-tile fallback renders as escaped tag soup. Empirically confirmed (Pango 1.57): `set_markup` *accepts* the C0-carrying fallback, including NUL — no warning, no crash, no fatal-warnings abort risk. |
| **Why It Matters** | One C0 byte discards ALL structured content — the `prompt` badge vanishes into tag soup (S2). The earlier predicted S1 escalation was falsified by probe. |

**Open Questions:**
- Can C0 bytes survive niri title plumbing? Mechanism code-certain; trigger rate unknown.

### [cairo-text-survives-gl-failure] — Cairo text survives GL unavailability

*Found by: wildcard. Priority: P1. Expected to fail at f87ec19.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | A content tile with markup renders its Pango/Cairo text layer on every draw regardless of EGL/GL health — GL failure may cost the decoration, never the data. |
| **Invariant** | `Always("a content tile with markup renders its text layer on every draw")`. `Unreachable("engine absent while content markup is available")`. `Sometimes("a draw completed with needs_gl false on a content tile")`. |
| **Antithesis Angle** | The SUT-side GL-failure seam (or a no-Mesa-surfaceless environment variant) at init blanks all 10 tiles permanently — including tiles whose `needs_gl` is false; the seam's per-frame `make_current`-failure mode blanks shader tiles per frame. |
| **Why It Matters** | The code's own comment claims Cairo-path independence from GL (lib.rs:250-254) but the architecture gates text inside the engine check — prose guarantee violated in code. Maximal S2 with a healthy data chain. |

**Open Questions:**
- Can `make_current` fail transiently on surfaceless Mesa, or only fatally?
- Is blank-on-GL-failure accepted for a personal tool? `(needs human input)`

(The injectability question was resolved by evaluation R4: the topology's GL-failure seam is the trigger.)

### [icon-negative-cache-pins-missing] — Cached icon-load failure never masks a readable icon file

*Found by: idempotency. Priority: P2. Expected to fail at f87ec19.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | A draw referencing an icon whose file is readable at draw time renders it; a previously cached load failure must not permanently pin the icon as missing. |
| **Invariant** | `AlwaysOrUnreachable("cached icon-load failure never masks a readable icon file")` in `raster_svg_cached`. Companion workload `Sometimes("icon render recovered after icons cache dir was wiped and regenerated")` — never true at f87ec19, regression guard post-fix. |
| **Antithesis Angle** | Delete/restore `~/.cache/claude-status/icons/` mid-run + daemon restarts; a permission flip on the icon path (chmod 000, then restore) reaches the pin while the file exists. Pin occurs only if the failure hits the *first* draw of a (path, px, tint) key — timing-sensitive. Scale-change key-minting mechanism confirmed (lib.rs:1246): new px keys silently heal or re-pin. |
| **Why It Matters** | Pixels depend on first-draw filesystem history; three write-once layers compose across two processes; one pin poisons all 10 tiles (thread-local cache). Only waybar restart heals. |

**Open Questions:**
- Realistic non-injected trigger rate for cache-dir loss (systemd-tmpfiles)? `(partial: scale-change healing mechanism confirmed; whether this deployment's hardware ever changes scale remains a deployment fact)`

---

## Category 3 — Attention liveness (S3)

### [prompt-pulse-visibly-advances] — Displayed prompt pulse actually advances

*Found by: wildcard. Priority: **P0**. ⚠️clock (or the SUT-side clock seam) for the f32 leg.*

| | |
|---|---|
| **Type** | Liveness |
| **Property** | While prompt-state markup is displayed, the whole-tile pulse phase advances across consecutive draws. |
| **Invariant** | `Sometimes("a displayed prompt pulse advanced phase between consecutive draws")` in the draw callback. Supporting `Always("the f32 animation clock advances between consecutive animated draws")`. Scoping witness `Sometimes("prompt-state markup was rendered at all")`. Primary oracle is SUT-side draw counters/phase assertions (investigation: GTK3's frame clock is damage-gated under nested/headless compositors; screenshots are secondary). |
| **Antithesis Angle** | Six links (producer prompt-cap → template → content_animates → tick gate/throttle → f32 time → frame clock); faults and thread pauses attack each. The f32-quantization leg needs a SUT-side seam (env-var start-offset into the Engine clock, pinning 36h/12d/97d regimes) — Antithesis's documented clock jitter is opt-in and ±30s-scale. |
| **Why It Matters** | The terminal observable every upstream property approximates; the gating subsystem is the proven highest-churn regression cluster. |

**Open Questions:**
- Does the tenant's clock-jitter fault move CLOCK_MONOTONIC, and at what magnitude? `(needs human input)` — planning default: use the SUT-side seam regardless.
- Add a cross-repo guard "payload contains any prompt ⇒ rendered markup contains `<pulse`"? (Template hardcodes the producer's 2-session cap.)

### [prompt-priority-survives-session-cap] — Producer's "a prompt is never dropped" cap guarantee holds end-to-end

*Found by: protocol-contracts. Priority: P1.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Whenever any tracked session on a desktop is in `prompt`, the emitted payload retains a prompt session within the 2-entry cap and the rendered markup contains `<pulse`. |
| **Invariant** | SUT Go `Always("session cap kept a prompt session in the emitted payload")` in `PayloadFor` after truncation (tile.go:173). Companion Go `Sometimes("session cap engaged while a prompt was present")`. Workload `Always("prompt session renders as pulsing tile within 3s")`. |
| **Antithesis Angle** | Session churn racing the daemon's 1s tick + 250ms throttle + byte-dedupe; >2 sessions per desktop; prompt flapping during cache rebuild. Investigation confirmed a dirty state string (`"prompt "`) is **doubly non-alerting**: exact `==` drops the pulse AND the substring animation gate misses it. |
| **Why It Matters** | The core alert is backed only by a sort comparator with zero tests on the >2-session path. |

**Open Questions:**
- Is >2 sessions per desktop actually reached in production? Decides Always+Sometimes vs AlwaysOrUnreachable.
- Is 3s the right end-to-end bound when Antithesis pauses the daemon?

### [animating-markup-has-tick-source] — Animating markup always has a frame-clock tick source

*Found by: wildcard. Priority: P1 → **confirmed violation class at f87ec19** (glow-only tiles).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Whenever stored content's markup animates, a frame-clock tick callback exists for that widget — the hand-maintained animation detectors never structurally disagree in the frozen direction. |
| **Invariant** | `Always("animating markup always has a frame-clock tick source")` in the 150ms dirty-poll callback — **oracle extended by investigation**: `!(content_animates(&m) || m.contains("<glow")) || tick_installed`. Using the SUT's own detector alone would inherit its blind spot: `<glow>` is empirically time-varying (GLOW_SRC modulates alpha by `sin(iTime*2.5)`, probe-verified alpha 153 peak vs 107 trough) and appears in *neither* detector. Companion `Sometimes("a content publish flipped animating false-to-true after a static period")`. |
| **Antithesis Angle** | A glow-only tile is a confirmed frozen-animation violation today. Second vector: markup passthrough via `| safe` (registered unconditionally in minijinja; end-to-end probe confirmed data-borne markup passes unescaped) with the producer emitting finished `<pulse>` markup — no tick callback ever. |
| **Why It Matters** | The permanent-frozen-attention variant the sibling race property cannot catch (flag and markup agree). |

**Open Questions:**
- Is `| safe` intended usage or out of contract? `(partial: mechanically reachable today; intent undocumented)`
- Where to source `tick_installed` (plumb into the poll closure vs expose on the store)?

### [animating-gate-matches-stored-content] — The animating gate reconverges with stored content within 500ms

*Found by: concurrency. Priority: P1.*

| | |
|---|---|
| **Type** | Safety (bounded divergence) |
| **Property** | The `animating` AtomicBool and stored markup may disagree only while a `set()` is in flight — divergence must be bounded, in both polarities (frozen pulse / static-content heat runaway). |
| **Invariant** | In the 150ms dirty-poll callback: `Always("animating flag reconverges with stored markup within 500ms")`. Companion `Sometimes("animating flag transiently disagreed with stored markup")`. |
| **Antithesis Angle** | Pause the reader thread between the animating store and content write; needs `content_animates(old) != content_animates(new)` transitions — workload forces them. |
| **Why It Matters** | S3: this exact gating subsystem regressed twice in one day. Does NOT cover the no-tick-source failure (flag and markup agree there) — owned by `animating-markup-has-tick-source`. |

**Open Questions:**
- Derive the 500ms bound from the poll-period constant instead of hardcoding?
- Transition frequency under the real backend in a short run? `(partial: transitions exist; frequency unmeasured — workload may need a synthetic toggling producer)`

### [publish-visible-within-poll-bound] — Published content is rendered or redraw-pending within two dirty-poll periods

*Found by: concurrency. Priority: P1.*

| | |
|---|---|
| **Type** | Liveness (bounded visibility) |
| **Property** | Once content is readable in the store, a redraw is pending or rendered within a bounded number of 150ms poll periods. |
| **Invariant** | In the dirty-poll callback: `Always("published content is rendered or redraw-pending within two dirty-poll periods")` (backed by the net-new `generation: u64` shared with `content-snapshot-torn-read`). Plus `Sometimes("content generation advanced ahead of the dirty flag")`. |
| **Antithesis Angle** | Thread-pause on the reader between content write and dirty store stretches a ~2-instruction window into seconds; static content has no rescue. |
| **Why It Matters** | S2 staleness manufactured purely by scheduling; trivial fix (set dirty before releasing the mutex). |

**Open Questions:**
- Is writer-pause staleness a finding the owner cares about for a single-user tool? `(needs human input)`
- Bound tuning (2 vs 3 poll periods) under a 10-instance main loop.

---

## Category 4 — Resource boundedness and heat

### [reload-conserves-producer-chains] — Reload conserves producer chains, threads, and timers

*Found by: resource-boundaries (independently confirmed by lifecycle + coordination). Priority: **P0**. ⚠️custom-fault (SIGUSR2); assumes p9c fix merged (crash currently masks the leak).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Across N SIGUSR2 reloads, live reader threads, producer chains, and 150ms dirty-poll timers stay equal to the current module count. |
| **Invariant** | Workload `Always("tile-watch producer chain count equals live stream-module count after reload settle")` (each chain = 2 processes: dash does NOT exec-replace — probe-confirmed; count both `sh` and `claude-status`) and `Always("waybar thread count stays flat across reloads")`. SUT `Always("live dirty-poll timer count equals live module-instance count")`. |
| **Antithesis Angle** | Plain `kill -USR2` suffices: waybar 0.15.0 reload destroys/re-creates all modules **unconditionally** (source-confirmed, no config diffing). Leak formula exact: **(N+1)×M chains** after N reloads of M tiles; counts never converge downward (waybar closes no fds; leaked readers hold the pipe read ends) — a short settle grace covering only new-chain spawn suffices. |
| **Why It Matters** | Unbounded growth from a routine operation. Confirmed-by-code, unfiled; becomes the primary reload bug once the crash fix merges. |

**Open Questions:**
- Does `queue_draw` on the leaked, unrealized widget do measurable work? (GTK no-ops unmapped draws — likely negligible; decides whether a CPU-time bound is worth adding.)

### [orphaned-tile-watch-bounded] — Orphaned producers do not accumulate across waybar restarts

*Found by: failure-recovery + resource-boundaries (merged). Priority: P2.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | tile-watch producers do not accumulate across waybar death/restart cycles: after waybar dies, restarts, and tile data next changes, every orphaned chain exits. |
| **Invariant** | Workload `Always("tile-watch process count equals live module count at post-restart checkpoints")` — kill waybar → restart → force a tiles.json change touching every key → settle → count system-wide (bound doubled for the 2-process chains). Companion `Sometimes("waybar death left temporarily-orphaned tile-watch producers")`. |
| **Antithesis Angle** | Crash/restart churn (the p9c crash produces it organically); orphan lifetime is data-dependent — SIGPIPE death chain live-repro-confirmed (exit 141 on next write; dash parent exits with its child), so quiet-desktop orphans die only on the next payload change. |
| **Why It Matters** | A crash loop on a quiet desktop accumulates 10 pollers (×2 processes) per iteration. |

**Open Questions:**
- Does the harness supervisor kill the process group on waybar death (would make the orphan window unreachable)?
- Post-fix acceptable bound — exactly N_tiles or N_tiles + draining generation? `(needs human input)`

### [respawn-backoff-floor-holds] — Stream respawn backoff floor holds under persistent failure

*Found by: resource-boundaries + failure-recovery (merged). Priority: P2.*

| | |
|---|---|
| **Type** | Safety + liveness companion |
| **Property** | Consecutive producer spawn attempts per stream reader are spaced ≥ RESPAWN_BACKOFF (1s) under persistent failure; once the cause clears, the next cycle publishes fresh content. |
| **Invariant** | SUT `Always("stream respawn attempts are spaced at least RESPAWN_BACKOFF apart per reader thread")`. Workload `Always("per-tile producer fork rate stays at or below one per second")` (fork counts ×2 for the sh+child pair). Companion `Sometimes("stream respawn succeeded after at least one consecutive failure")`. |
| **Antithesis Angle** | Kills, unlinking or permission-flipping the exec binary (spawn-Err branch) vs instant-exit commands (EOF branch) — two branches sharing one sleep. Aggregate rate also detects reload-leak fork multiplication. |
| **Why It Matters** | The single sleep at the loop tail is the entire storm defense; the only stream test is happy-path. Natural retry-outcome instrumentation point. |

**Open Questions:**
- Which real conditions produce spawn-Err vs silent succeed-then-127? Argues for a rate-limited counter on the instant-exit path.

### [static-idle-redraw-budget] — Static content queues zero redraws; animated content respects the fps cap

*Found by: resource-boundaries. Priority: P1. Expected to fail at f87ec19 for idle_level>6 payloads.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Redraw scheduling is bounded by content class: static content queues zero frame-clock redraws between data changes; animated content queues ≤ target fps (default 30). |
| **Invariant** | SUT `Always("static tile content queues no frame-clock redraws between content changes")` and `Always("animated tile redraw rate stays at or below the target fps")`. Companion `Sometimes("an out-of-range idle level reached the renderer")` (shared with `idle-level-gate-clamp-divergence`). |
| **Antithesis Angle** | Workload writes tiles.json directly — `idle_level: 7` reaches the known blind spot (30fps forever on clamped-static content). Backend cannot emit >6 today (DecayLevels=7) — this is a contract-robustness/skew guard, injected via direct cache writes. |
| **Why It Matters** | The runaway is a constant-CPU space heater across 10 tiles; the gating class regressed twice in one day. |

**Open Questions:**
- Assertion placement (avoid re-implementing the buggy function); should 7 clamp to static? `(needs human input on product intent)`

### [idle-level-gate-clamp-divergence] — Renderer clamp and animation gate agree on effective idle level

*Found by: protocol-contracts. Priority: P1. Expected to fail at f87ec19.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | For idle content, the animation gate's verdict equals the renderer's clamped level. |
| **Invariant** | SUT `Always("idle animation gate agrees with the renderer's clamped level")` in `ContentStore::set`. Companion `Sometimes("out-of-range idle_level reached the renderer")`. |
| **Antithesis Angle** | Inject `idle_level: 7 / 100 / -1 / 6.0` via direct cache writes. The float vector is probe-confirmed end-to-end: minijinja renders `6.0` as literal `"6.0"` → renderer `parse::<usize>` fails → level 0 → hour-old idle renders as freshly-idle **bright** — a real divergence, not a collapse into the in-range case. |
| **Why It Matters** | The mechanism-level cause of the redraw-budget violation; both legs validated at f87ec19. |

**Open Questions:**
- How does the workload observe redraw rate — needs an exported per-tile draw counter?
- Is the intended contract "clamp and stay static" or "reject out-of-range"? `(needs human input)`

### [shader-recompile-only-on-mtime-change] — Background shader compiles only when its file mtime changes

*Found by: idempotency. Priority: P2. Known-violated at f87ec19 in the failure state.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Repeat draws with an unchanged shader file are side-effect-free: no file read, no GLSL recompile, no new GL objects. |
| **Invariant** | SUT `Always("background shader is recompiled only when its file mtime changes")` in `refresh_shader`. Workload `Sometimes("background shader recovered to rendering after file was broken and restored")`. |
| **Antithesis Angle** | Workload cycles the shader file valid→broken→missing→valid; a permission flip reaches the retry state without content changes. |
| **Why It Matters** | A shader typo becomes fs read + compile + stderr line per frame at 30fps; escalates toward GL exhaustion → the draw-path `.unwrap()` panic → host abort (S1). |

**Open Questions:**
- Is per-frame retry deliberate hot-reload UX? Even so, the invariant only weakens to "one attempt per observed mtime", which the code still violates in the failure state.
- Practical GL exhaustion threshold under Mesa llvmpipe — is the panic endpoint reachable within a run?

### [shader-recompile-gl-object-leak] — Shader recompilation never grows live GL objects

*Found by: resource-boundaries. Priority: P2.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Any sequence of background-shader file edits (valid or invalid GLSL) leaves the tile's live GL object count constant. |
| **Invariant** | SUT `AlwaysOrUnreachable("live ShaderPass GL object count stays constant per tile across recompiles")`. `Unreachable("shader compile failure path leaked a live GL object")`. Companions: `Sometimes("a failing shader file was retried on a subsequent frame")`; workload `Always("waybar RSS slope stays flat while a broken shader file is configured")`. |
| **Antithesis Angle** | Three code-verified leak paths (no ShaderPass Drop; compile-error path leaks failed shader AND compiled vertex shader; link failure leaks the program) × per-frame retry ≈ 216k objects/hour. |
| **Why It Matters** | Hot-reload is advertised (L8); exhaustion escalates to the shader.rs unwraps → SIGABRT of waybar. |

**Open Questions:**
- Is the RSS proxy sensitive enough under llvmpipe, or is the SUT counter mandatory?
- Does Mesa GL name exhaustion ever fail creation (→ unwrap → abort) or grow indefinitely?

---

## Category 5 — Data-plane integrity

### [tile-cache-never-torn] — Tile cache reads always parse (single-writer assumption)

*Found by: coordination + data-integrity (merged). Priority: P2. ⚠️custom-fault (second daemon).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | A reader that successfully reads an existing tiles.json always gets bytes that parse as a valid payload map — even when the unenforced single-daemon assumption is violated. |
| **Invariant** | SUT Go `Always("tile cache reads always parse as valid JSON")` in `ReadCache` (ENOENT excluded). Companions: `Sometimes("tile cache tmp rename lost a concurrent race")`; workload `Sometimes("two daemons wrote the tile cache concurrently")`. |
| **Antithesis Angle** | Fixed tmp name + no singleton guard (confirmed: no flock/pidfile/unit anywhere): two daemons interleaving open/write/rename → the second keeps writing into the live inode after the first renames it. The workload simply starts a second daemon. |
| **Why It Matters** | Torn cache → emit error branch → fabricated idle payload masks a live prompt; also last-writer-wins flapping. The SVG writer a few functions away uses unique `os.CreateTemp` — the cache writer didn't get the same treatment. |

**Open Questions:**
- Is dual-daemon operation operator error, or should the daemon enforce a singleton (flock)? `(needs human input)`

### [content-snapshot-torn-read] — A rendered frame reads markup and uniforms from the same content generation

*Found by: data-integrity + concurrency (merged). Priority: P2. Expected to fail at f87ec19.*

| | |
|---|---|
| **Type** | Safety + Reachability |
| **Property** | Each draw composes markup and shader uniforms from the same TileContent publication. |
| **Invariant** | Backed by the net-new `generation: u64` (shared with `publish-visible-within-poll-bound`): `Always("draw snapshot: markup and uniforms read from the same TileContent generation")`; `Sometimes("draw observed uniforms and markup from different content generations")`; `Always("markup generation never lags uniforms generation within a single draw")`; `Always("rendered content generation is monotonic across draws")`. |
| **Antithesis Angle** | Pure scheduling: land a `set()` between the draw's two lock acquisitions. Also a calibration signal that scheduler exploration works in this harness. No shipped preset combines `shader_uniforms` with `stream: true` (confirmed) — the workload must add a synthetic stream+uniforms tile; rank below other concurrency properties for user-visible severity. |
| **Why It Matters** | Textbook torn read, one-line fix (single snapshot accessor). |

**Open Questions:**
- Fix (single-lock snapshot) vs instrument? A fix flips the `Sometimes` to `Unreachable` as regression guard.

### [contentstore-mutex-never-poisoned] — The ContentStore mutex is never observed poisoned

*Found by: concurrency. Priority: P2.*

| | |
|---|---|
| **Type** | Reachability (negative) |
| **Property** | The three sites that silently swallow a poisoned content mutex are never reached. |
| **Invariant** | `Unreachable("ContentStore::set dropped an update on a poisoned content mutex")`, `Unreachable("ContentStore::markup read a poisoned content mutex")`, `Unreachable("ContentStore::uniforms read a poisoned content mutex")` (content.rs:88, 100-115). |
| **Antithesis Angle** | Pure tripwire — a firing means the panic-freedom argument broke in a future refactor. |
| **Why It Matters** | If ever reached: permanently, silently blank tile — the worst S2 shape. |

**Open Questions:**
- None.

---

## Category 6 — Config and version compatibility

### [config-resolve-preserves-tile-identity] — Config resolution preserves configured tile identity

*Found by: wildcard. Priority: P1.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | A module config that names a tile source or an `exec` producer never silently resolves to a different tile. |
| **Invariant** | `Always("config naming a tile preset resolves with a template present")`; `Always("configured exec survives config resolution")`; `Reachable("preset merge applied to module config")`. |
| **Antithesis Angle** | Workload file mutation (unlink/permission-flip of a `tile_file` path) timed against init/reload; type-perturbed configs collapse the whole struct → 60fps demo tile. Live config uses bundled `tile: "claude"` everywhere (confirmed) — the tile_file leg is unreachable in production shape; the workload keeps a tile_file variant to exercise it. |
| **Why It Matters** | Three degradation arms produce three different plausible-looking wrong tiles with stderr-only diagnostics — S2 + S5 simultaneously. Attack surface #7. |

**Open Questions:**
- Does waybar reject malformed top-level config JSON before the plugin sees it? Bounds which perturbations reach `resolve()`.
- Is silent fallback the intended degradation contract, or should the error surface on the tile? `(needs human input)`

### [cffi-v1-config-transport-retype] — String configs survive the CFFI v1 transport

*Found by: version-compat. Priority: P3. Expected to fail for JSON-lookalike strings.*

| | |
|---|---|
| **Type** | Safety |
| **Property** | A correctly-authored string config value reaches the plugin as a string; the pinned wbcffi v1 transport must not silently retype it in transit. |
| **Invariant** | SUT `Always("string-typed module config values arrive as strings across the CFFI boundary")` on the raw Value before resolve. The null sub-case is hard-assertable: JsonCpp `asString()` on JSON null yields `""` at every supported version (verified 1.7.4/1.9.2/master). Workload `Sometimes("a JSON-lookalike string config value was exercised through the CFFI transport")`. |
| **Antithesis Angle** | Workload varies module configs across restarts/reloads; a v2-declaring plugin build is a one-line A/B companion. Live config contains no JSON-lookalike strings (confirmed) — the property guards future edits and the v2 migration. |
| **Why It Matters** | Under v1, `"text": "42"` crosses as bare `42` → one retyped field collapses the whole config to the 60fps demo tile from a *valid* config. |

**Open Questions:**
- None.

### [producer-binary-swap-mixed-versions] — Mixed-version claude-status fleet stays contract-valid and converges

*Found by: version-compat. Priority: P3. ⚠️custom-fault (binary symlink flip).*

| | |
|---|---|
| **Type** | Safety |
| **Property** | Upgrading the claude-status binary mid-run (daemon stays old; tile-watches respawn into the new build) never yields non-contract tile output; the fleet converges once versions re-match. |
| **Invariant** | Workload `Sometimes("mixed-version daemon and tile-watch fleet was reached")`; `Always("tile lines remain contract-valid under mixed claude-status versions")` (expected to fail across the historical e34138c shape boundary); `Sometimes("tile fleet reconverged after producer upgrade completed")`. |
| **Antithesis Angle** | Two builds behind a workload-flipped symlink; kills drive the swap through varied orders — including the reverse (new daemon, old lingering watches). `go install` semantics resolved: atomic rename same-fs, unlink+rewrite cross-fs, never in-place truncation — old running binaries are safe; inode-number equality is not evidence of in-place overwrite (ext4 reuses freed inodes). |
| **Why It Matters** | tiles.json has no version marker; shape skew becomes zero-valued fields hitting the renderer's silent fallthrough. |

**Open Questions:**
- Exact pre-e34138c payload shape (pick the older commit for maximal distance)?

---

## Category 7 — Post-evaluation gap-fill (backend ingress & derivation, liveness tripwires, claims coverage)

Ten properties added after the 4-lens evaluation identified gaps (evaluation/
synthesis.md). All follow the amendment rules above (event-counted bounds,
variant gating, real-window fixtures). Full designs in their evidence files.

### [hook-prompt-never-silently-dropped] — Prompt-transition hook events are recorded or observably rejected

*Gap G1a (ingress). Priority: P1. **Expected to fail at HEAD under disk-full** (logError is itself best-effort — zero-trace loss).*

Safety. `claude-status hook` swallows every error and always exits 0; Claude Code never retries. Workload `Always("hook prompt transition was recorded in the DB or rejected in the hook log")` checked per invocation; SUT Go `Reachable("hook logged a swallowed ingress error")`, `Unreachable("hook error log write itself failed")`, `Sometimes("hook audit event insert failed while state write succeeded")`. Levers: SQLite lock contention past the 2s busy timeout, disk pressure, malformed sibling fields (strict Unmarshal drops whole events — the "tolerant parse" comment is wrong).

**Open Questions:** Is zero-trace loss under disk-full accepted by design? `(needs human input)` — Should the strict parse be tolerant (act on session_id + event when they parse)?

### [live-prompt-session-never-reaped] — A demonstrably-live prompt session is never GC-reaped

*Gap G1b (reaper). Priority: P1. Likely first-run bug-finder: the startup mass-reap tripwire is **expected to fire at HEAD**.*

Safety. Regression guard for the validated 133ae8d heartbeat-reap bug. Workload `Always("a prompt session with live window and live first-party file survives GC")` (event-counted, ≥2 ticks). SUT Go `Unreachable("gc reaped via window-absence before the first niri window snapshot arrived")` — the window-absence arm has no debounce and no model-adoption gate, so a 1s tick beating the first WindowsChanged reaps every session; per-arm `Sometimes` attribution anchors. gc.go's "a false reap is self-healing" is **false for prompt** (a blocked Claude fires no hooks — the reap permanently deletes the alert).

**Open Questions:** Should gc gate on model adoption (mirror of writeTiles' hatch)? `(needs human input)` — Is 3 ticks the right first-party debounce? — Can niri transiently omit a live window from a snapshot?

### [session-desktop-attribution-tracks-window] — A session renders on exactly its window's desktop

*Gap G1c (attribution). Priority: P2.*

Safety. Both validated attribution fixes (7d42f65 nondeterministic pick, 1c26d14 correct-or-NULL binding) live on this chain; no property covered it. Workload `Always("each fixture session renders on exactly the desktop key holding its window at quiescence")` (quiesce-then-check); `Always("app-layout desktop content is stable across cache rebuilds with unchanged topology")`; `Always("hook-bound window_id is the window hosting the client, or NULL")` via staged-/proc fixtures. Window moves get no immediate cache write (only WorkspaceActivated does) — the 250ms throttle window is the race.

**Open Questions:** Can a WindowOpenedOrChanged be lost on a live stream, or does exit-on-close convert gaps into restarts? — Is /proc staging (fake `claude`-comm) stable in the container?

### [first-party-overlay-garbage-tolerant] — The first-party overlay tolerates arbitrary session-file garbage

*Gap G1d (overlay). Priority: P1.*

Safety. The undocumented, version-fragile format is re-read at up to ~77Hz with **no size cap**; carries the validated e60a874 stale-busy gate marked "do not simplify". Workload `Always("daemon survives adversarial first-party session files")`, `Always("daemon RSS stays below ceiling while an oversized session file exists")`, `Always("tile session states remain in the schema enum under first-party garbage")`, `Always("a stale busy file never masks a fresher hook idle")` (workload-authored timestamps — no clock fault). SUT Go `Sometimes` witnesses for unparseable-skip and stale-busy-deferral (vacuity guards).

**Open Questions:** Should readFile cap file size (the only unbounded read on the daemon's hot loop)? — Do parseable-but-drifted shapes deserve a drift canary here or a doctor-side check?

### [stream-reader-thread-liveness] — Every stream module has a live reader thread

*Gap G2. Priority: P2.*

Liveness + negative Reachability. A panic in `build.content(...)` (content.rs:275, outside the mutex) kills the reader silently: no abort, no poisoning, producer SIGPIPEs and dies, no respawn (the loop lived in the dead thread) — tile frozen forever. Workload `Always("live stream reader thread count equals stream module count at checkpoints")` — **two-sided**, via named reader threads (1-line Builder change) + /proc task comm counting. SUT `Sometimes("stream reader iterated a line")` per-module heartbeat; `Unreachable("stream reader thread died by panic")` drop-guard. Also records that the chain-count properties' `==` is load-bearing.

**Open Questions:** Thread-name counting vs per-store heartbeat timestamp — owner preference? — Fix policy: catch_unwind-and-respawn vs die-loudly? `(needs human input)`

### [draw-path-gl-panic-tripwires] — Draw-path GL object creation never fails (tripwires)

*Gap G3. Priority: P2 (ride-along; precedent: contentstore-mutex-never-poisoned).*

Negative Reachability. `Unreachable("draw path: shader target texture creation failed")` (shader.rs:277), `Unreachable("draw path: shader target framebuffer creation failed")` (shader.rs:300), `Unreachable("gl bootstrap: libepoxy failed to load")` (gl.rs:21-25); companion `Sometimes("shader target was (re)created")` proves the tripwires are armed. Instrument-first (assert, then panic identically — the SDK emits before the abort). Injectability honestly limited: value is named findings instead of raw SIGABRT cores, the shader-leak escalation landing zone, and a no-epoxy environment variant.

**Open Questions:** Does llvmpipe GL name exhaustion ever fail creation, or grow to OOM? — Should the Err arms degrade instead of panic? `(needs human input)`

### [poll-refresh-survives-hung-exec] — Poll refresh recovers when a hung exec's child dies

*Gap G4. Priority: P3 (poll mode is not production wiring; gap demonstrator).*

Liveness. `run_command` has no timeout; the refresh thread has no watchdog. SUT `Sometimes("poll refresh cycle completed")` per iteration (silence during the hang fixture = the stall fingerprint); workload `Always("poll tile publishes within 2 intervals after its hung child dies")` (event-counted, quiet window); `Sometimes("a poll exec invocation outlived 10 intervals")` staleness witness. Two hang shapes: child never exits; child exits but a forked grandchild holds stdout (output() reads to EOF).

**Open Questions:** Intended contract — add a timeout to run_command? `(needs human input; interacts with the interval:0 question)`

### [idle-decay-reaches-static] — Idle decay advances through the buckets and lands static at level 6

*Gap G5 (claim L5). Priority: P1. **Found a real HEAD divergence**: NULL-last_talk idle renders bright + animating at 30fps forever via `sessionTile` (level 0 via omitempty + template default) while `aggregate` maps the same row to dimmest — same daemon, two answers; row shape reachable via Notification-created sessions.*

Liveness + Safety. Drivable with backdated `last_talk_ts` only — no clock fault (backdating is GC-safe since 133ae8d). Workload `Always("emitted idle_level matches the decay bucket of the backdated last_talk age")`, `Always("idle_level never decreases while last_talk_ts is unchanged")`, `Sometimes("an idle tile reached level 6 and went static")` (uses the shared per-tile draw counter), `Always("an idle session with NULL last_talk is not rendered as freshly idle")` — **expected to fail at HEAD**. SUT Go `Sometimes("tile payload carried an idle session with NULL last_talk")`.

**Open Questions:** Intended rendering for NULL-last_talk idle — dimmest, level 0, or omit? The two backend paths disagree today. `(needs human input)`

### [shader-pass-blend-state-neutralized] — ShaderPass neutralizes inherited GL blend state before drawing

*Gap G6 (S8/bpe regression). Priority: P3 (two-line ride-along; the alternative recorded-exclusion text is in the evidence file — owner may prefer pinning the offscreen harness in CI instead).*

Safety + Reachability. `Sometimes("shader pass entered with blend left enabled by a prior renderer")` (samples `is_enabled(BLEND)` before the disable — proves the hazard precondition, validated from fix 75886b1) + `Always("shader pass draw begins with blending disabled")` before draw_arrays — survives the plausible refactor that deletes the "redundant-looking" disable block.

**Open Questions:** Is `is_enabled` trustworthy under llvmpipe at this call point? — Property vs exclusion (offscreen-harness CI pin)? `(needs human input)`

### [active-accent-follows-focus] — The active accent tracks the focused desktop, exactly once

*Gap G7 (claim L3). Priority: P1. Builds `parallel_driver_focus_churn.sh`, which the injection properties' niri-title vectors also need.*

Safety + Liveness. Workload `Always("at quiescence exactly one tile payload is active and it matches niri's focused workspace")` (quiesce-then-check, ≥2 ticks; shares the no-workspaces escape hatch); coverage `Sometimes("the active accent moved between desktops")`; latency only as an event-counted `Sometimes` hint (never a wall-clock Always); SUT Rust `Sometimes("a draw rendered the active accent card")` (draw_active_panel, lib.rs:1098). Angle: focus churn races the one throttle-bypassing immediate cache write, the 13ms debounce, byte-dedupe (A→B→A inside one 75ms poll), and daemon restart mid-switch.

**Open Questions:** Can single-output nested niri emit WorkspaceActivated with Focused=false (untestable branch)? — Multi-output "exactly one" semantics if the topology grows.

---

## Assumptions (catalog-wide)

- **Harness topology**: per `deployment-topology.md` — single container; **sway
  with `WLR_BACKENDS=headless` as the outer compositor** hosting waybar+plugin
  (runtime output hotplug via swaymsg), niri nested as the daemon's event
  source; llvmpipe/pixman software GL; workload can read /proc, signal
  processes, write tiles.json/DB, swap binaries. Screenshot checks secondary
  to SUT-side counters (GTK3 frame clock is damage-gated).
- **Process-count constants are host-specific**: dash `sh -c` does not
  exec-replace (2 processes/chain) on this machine; the Antithesis container's
  /bin/sh must be re-checked and constants adjusted (invariants unchanged).
- **Waybar reload semantics source-confirmed at 0.15.0**: unconditional
  destroy-all-then-construct-all, fresh dlopen per module, auto bar
  re-creation on output re-add.
- **p9c sequencing**: at f87ec19 teardown-involving runs abort on the p9c
  crash first; properties downstream of any teardown cycle assume runs
  against (or alongside) the fix branch `worktree-fix-gl-teardown-crash`.
- **Fault availability**: no property requires tenant node-termination
  (in-container supervisor covers kills); clock-jitter is opt-in and
  small-magnitude — the f32 legs use the SUT-side clock seam.
- All assertions are net-new in both repos; Rust side needs the Antithesis
  Rust SDK in Cargo.toml, Go side the Go SDK in agentic-db.

## Open Questions (catalog-wide)

- Tenant fault configuration: is clock jitter enabled, and does it move
  CLOCK_MONOTONIC? `(needs human input)` — planning default: SUT-side seam.
- The ~9 per-property `(needs human input)` design-intent questions above
  (blank-on-GL-failure, empty-cache-on-read-error intent, unknown-state
  rendering, `interval: 0` contract, single-daemon enforcement, out-of-range
  idle_level policy, icon-read threading, post-fix orphan bound, writer-pause
  staleness) — collected for the owner; none block workload implementation,
  they refine assertion strictness.
- Antithesis-container `/bin/sh` identity (busybox ash vs dash) — sets
  process-count constants at harness-build time.
