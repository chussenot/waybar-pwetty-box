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

# Evaluation — Antithesis Fit

Lens: does each property require exploring a state space a deterministic test
cannot reach? Inputs: `property-catalog.md` (41 properties),
`sut-analysis.md` (+Errata), `deployment-topology.md`,
`existing-assertions.md`, property evidence files, and the Antithesis fault
documentation re-verified today (2026-07-22) at
https://antithesis.com/docs/concepts/fault_injection/fault_types.md and
https://antithesis.com/docs/reference/instrumentation/coverage_instrumentation.md.

## Fault-reality baseline (verified against Antithesis docs)

The documented fault set is: network faults (useless here — single
container, correctly noted), node throttling / node hang / node termination
(whole-container granularity), **thread pausing (requires coverage
instrumentation; Rust is a supported instrumentation target)**, clock jitter
(opt-in, affects all nodes, worked example ±30s, POSIX clock unspecified),
CPU modulation, and custom faults (user-written scripts).

**There is no filesystem fault injection** (no transient EIO, no fs latency,
no disk faults) **and no memory-pressure / fd-exhaustion fault** anywhere in
the documented set. The only fs-related lever is the workload/custom-fault
scripts mutating files — which is state mutation, not syscall failure
injection. This baseline drives Finding CW-1.

Given the single-container topology, the *effective* arsenal for this SUT is:
thread pausing (conditional on instrumenting the cdylib), CPU
modulation/throttling, opt-in clock jitter, and the timing/ordering
randomization of the workload's own actions (kills, reloads, writes). Every
"Antithesis Angle" in the catalog should be implementable with exactly that
list; several are not (below).

---

## Findings (catalog-wide first)

### CW-1 — Five properties assume fault types that do not exist

- **Properties:** `neighbor-modules-stay-live`, `icon-negative-cache-pins-missing`,
  `config-resolve-preserves-tile-identity`, `shader-recompile-only-on-mtime-change`,
  `engine-init-failure-contained`, `cairo-text-survives-gl-failure`
  (and, mildly, `icon-src-read-bounded-nonblocking`).
- **Concern:** Their Antithesis Angles cite "fs latency faults on the
  per-draw retry paths", "transient EIO on `fs::read`", "transient fs fault
  on a `tile_file` path during init/reload", "fs faults reach the retry
  state without file changes", and "resource-pressure faults (ENOMEM, fd
  exhaustion) during init". None of these is in the documented fault set,
  and the deployment topology's own fault table lists only
  workload-script file *mutation*. As written, these angles are
  unimplementable; the affected legs will silently never be exercised and
  their companion `Reachable`/`Sometimes` items will sit red forever
  (see CW-5).
- **Scope:** The specific trigger legs, not the invariants — most of these
  properties have a second, implementable trigger (FIFO paths, file
  delete/restore, cache-dir wipe, environment variant without the Mesa
  surfaceless ICD).
- **Evidence:** fault_types.md enumerates the complete set (fetched today);
  `deployment-topology.md` "Fault Availability Requirements" table;
  `properties/cairo-text-survives-gl-failure.md` already asks "which EGL
  failure modes are actually injectable … if neither works, the property
  needs a fault seam" — the docs answer: neither works.
- **Suggested action:** Rewrite the affected Antithesis Angles to
  workload-implementable triggers only (file swap/unlink/FIFO/permission
  flip; env-variant images). For the GL-failure pair, commit to the
  SUT-side failure seam (env var forcing `OffscreenGl::new` /
  `make_current` to fail) — without it the per-frame `make_current` leg of
  `cairo-text-survives-gl-failure` has **no trigger at all** under
  llvmpipe. Alternatively an LD_PRELOAD/FUSE custom fault could fake EIO,
  but nothing in the topology plans that build; don't leave it implied.

### CW-2 — A third of the catalog is deterministic-trigger work wearing an Antithesis costume

- **Properties:** `embed-placeholder-parity`, `no-control-chars-in-pango-markup`,
  `unknown-session-state-renders-blank`, `idle-level-gate-clamp-divergence`,
  `static-idle-redraw-budget` (injection leg), `cffi-v1-config-transport-retype`,
  `config-resolve-preserves-tile-identity` (config-matrix leg),
  `stream-line-length-bounded`, `stream-ingest-memory-bounded` (flood legs),
  `icon-src-read-bounded-nonblocking` (FIFO leg), `duplicate-line-rerender-idempotent`,
  `poll-mode-cold-start-converges`, `watcher-key-survives-output-rename`
  (post-re-scope), `animating-markup-has-tick-source` (glow-only and
  `| safe` legs), `shader-recompile-only-on-mtime-change`,
  `shader-recompile-gl-object-leak`.
- **Concern:** For each of these, the violating state is reached by a
  *fixed input*, not an interleaving: `mkdir` with U+FFFC, a C0 byte in a
  title, `idle_level: 7` written to tiles.json, `"text": "42"` in a config,
  a producer emitting one >64KiB line, a FIFO as `app_icon`, a wrong
  `--output` flag, a glow-only tile config, a broken shader file. Every one
  fails (or passes) identically on the first attempt in a deterministic
  test; the catalog's own evidence admits this in places
  (`embed-placeholder-parity.md`: "workload-input-shaped rather than
  fault-timing-shaped"; `watcher-key-survives-output-rename.md`: the
  re-scoped trigger is a misconfiguration the workload itself creates).
  Treating them as 16 of 41 co-equal exploration targets will misallocate
  workload composition and search budget toward states that need no search.
- **Scope:** The properties' *ranking and driver budget*, not their
  assertions. The SDK assertions themselves are cheap, always-on tripwires
  that ride along with ambient churn and do catch unforeseen compositions
  (e.g., C0 injection landing during a reload) — that residual value is
  real but is ride-along value, not driver-worthy value.
- **Evidence:** Per-property "Antithesis Angle" rows in
  `property-catalog.md`; evidence-file admissions cited above; the fault
  baseline (no fault type participates in any of these triggers).
- **Suggested action:** Tag every property with a trigger class —
  `interleaving` (only a scheduler/fault composition reaches it),
  `fault-composition` (workload action × fault timing), `deterministic-input`,
  `environment-variant` — and have `antithesis-workload` weight drivers
  toward the first two. Fold the deterministic-input triggers into the
  existing `parallel_driver_session_churn.sh` hostile-string/payload
  weights (the topology sketch already does this for some) instead of
  giving them dedicated drivers. Additionally run the pure-function subset
  (`embed-placeholder-parity`, `no-control-chars-in-pango-markup`,
  `duplicate-line-rerender-idempotent`, `cffi-v1-config-transport-retype`,
  `idle-level-gate-clamp-divergence` table) as a deterministic pre-flight
  (cargo test / integration script) so their verification doesn't depend
  on Antithesis time at all.

### CW-3 — Wall-clock bounds inside Always assertions will be violated by the fault injector, not the SUT

- **Properties:** `neighbor-modules-stay-live` (~250ms stall budget),
  `producer-kill-tile-reconverges` (5s), `stream-recovery-after-framing-violation`
  (10s), `prompt-priority-survives-session-cap` (3s),
  `animating-gate-matches-stored-content` (500ms).
- **Concern:** Thread pausing "pauses threads for small periods", node
  hang freezes the container, node throttling and CPU modulation slow
  everything, and clock jitter jumps the clock by tens of seconds — all
  by design. A wall-clock-bounded `Always` evaluated while any of these is
  active reports the fault injector as a SUT bug. The 250ms draw budget is
  the worst case: llvmpipe software-renders GL shaders on the main thread,
  so CPU modulation alone can legitimately blow it with zero SUT defect.
  The catalog notices this exactly once (prompt-priority's "Is 3s the
  right bound when Antithesis pauses the daemon?") but has no
  catalog-wide rule.
- **Scope:** Assertion expression, not property intent.
- **Evidence:** fault_types.md (thread pausing, node hang, CPU modulation,
  clock jitter semantics); contrast with
  `properties/publish-visible-within-poll-bound.md`, which counts
  *dirty-poll periods with hysteresis* instead of milliseconds — pausing
  the main thread pauses the poll counter too, so the bound is
  scheduler-safe. That property is the model.
- **Suggested action:** Re-express each bound in event-counted,
  scheduler-visible units (poll iterations, draws, producer emissions,
  daemon ticks) where a paused observer stops counting; where wall time is
  unavoidable (end-to-end reconvergence), evaluate only inside quiescent
  windows — the topology already plans `ANTITHESIS_STOP_FAULTS` and
  `eventually_converged.sh`; route these five properties through that
  machinery explicitly. Keep `respawn-backoff-floor-holds` as-is: its
  bound is a *lower* bound on spacing, which pausing can only widen.

### CW-4 — Priorities encode bug severity, not exploration value; both directions misrank

- **Properties:** P0s `reload-conserves-producer-chains`,
  `module-teardown-never-aborts-host` (at f87ec19); P2s
  `torn-ndjson-frame-rendered`, `content-snapshot-torn-read`,
  `so-replacement-reload-race`.
- **Concern:** The reload leak has an exact closed-form ((N+1)×M, "plain
  `kill -USR2` suffices", "counts never converge downward") — it fires on
  the first reload with zero exploration; a 5-line deterministic test finds
  it every time. p9c at f87ec19 likewise "fails deterministically". Ranking
  these P0 for *Antithesis* conflates "most important bug" with "most in
  need of exploration". Meanwhile the properties only Antithesis can
  realistically test sit at P2: `torn-ndjson-frame-rendered` needs SIGKILL
  *between iterations of a multi-part pipe write* (no deterministic test
  lands that without ptrace surgery), `content-snapshot-torn-read` needs a
  `set()` inside a ~10-line window between two lock acquisitions (pure
  scheduler exploration — the evidence file itself calls it the
  calibration signal), and `so-replacement-reload-race` needs
  dlopen-mid-replacement and delayed-SIGBUS interleavings.
- **Scope:** Catalog-wide priority semantics; affects run-2 workload
  weighting more than run-1 (run-1's "find p9c immediately" use of
  deterministic failures as harness validation is legitimate and cheap).
- **Evidence:** `properties/reload-conserves-producer-chains.md` (formula,
  settle-window analysis); `property-catalog.md` priority tags;
  `properties/torn-ndjson-frame-rendered` catalog row (pipe-atomicity
  mechanics); `properties/content-snapshot-torn-read.md` ("routine for
  Antithesis scheduling exploration … the one property in this set that
  tests Antithesis's scheduler exploration").
- **Suggested action:** Add a second axis to the priority tags:
  `fires-deterministically` (run-1 harness validation; near-zero ongoing
  search cost) vs `exploration-dependent` (post-fix search budget). For
  the post-fix catalog, the exploration-dependent P2s above should outrank
  deterministic P0s in workload weight — the deterministic ones keep their
  assertions but need no dedicated search.

### CW-5 — No reachability audit for Sometimes/Reachable: several are structurally red, one pair is contradictory

- **Properties:** `module-teardown-never-aborts-host`,
  `engine-init-failure-contained`, `cairo-text-survives-gl-failure`,
  `icon-negative-cache-pins-missing`, `poll-mode-cold-start-converges`,
  `tile-cache-never-torn`, `producer-binary-swap-mixed-versions`,
  `so-replacement-reload-race`, `content-snapshot-torn-read`.
- **Concern:** In Antithesis, a `Sometimes`/`Reachable` that never fires is
  a failing property in every triage report. The catalog contains several
  with no reachable trigger under the actual harness:
  - `Reachable("engine teardown leaked the canvas because make_current
    failed")` — no mechanism makes `make_current` fail under surfaceless
    llvmpipe (CW-1); permanently red unless the seam lands.
  - `Reachable("module init degraded … renderer init failed")` — the
    catalog itself suspects it's dead code ("demotes to documentation").
  - `Sometimes("icon render recovered after icons cache dir was wiped")` —
    acknowledged "never true at f87ec19"; red until a fix nobody has
    scheduled.
  - `Sometimes("poll-mode tile re-published content …")` — *designed*
    never to fire under `interval: 0`; the evidence file embraces the red
    report as "the finding". Legitimate for one demonstration run;
    as a standing catalog member it is permanent report noise gated on an
    unanswered `(needs human input)` design question.
  - Custom-fault-dependent Sometimes (`"two daemons wrote the tile cache
    concurrently"`, `"mixed-version fleet was reached"`, `".so replaced
    mid-reload"`) fire only in runs whose specific driver is enabled; if
    all assertions ship in all runs, every run without that driver is red.
  - **Contradiction:** `cairo-text-survives-gl-failure` declares
    `Unreachable("engine absent while content markup is available")` while
    `engine-init-failure-contained` deliberately *creates* exactly that
    state (env variant without Mesa; its `AlwaysOrUnreachable("draw
    completed with engine absent")` plus `Reachable(degraded to
    engine-less mode)` requires reaching it, with content flowing). In the
    degraded-mode variant the Unreachable fires on every content draw.
    These two cannot ship unconditionally in the same binary.
- **Scope:** Report hygiene and assertion-set design; the invariants are
  mostly fine.
- **Evidence:** Catalog invariant cells for each slug;
  `properties/module-teardown-never-aborts-host.md` (Reachable arms);
  `properties/poll-mode-cold-start-converges.md`;
  `properties/cairo-text-survives-gl-failure.md` vs
  `engine-init-failure-contained` catalog row. Only
  `torn-ndjson-frame-rendered` does variant gating today ("gated by a
  harness config flag").
- **Suggested action:** Build the run-variant × assertion-set matrix as a
  first-class artifact: every Sometimes/Reachable lists the driver or
  environment variant that makes it satisfiable, and is compiled/enabled
  only there (env-var gating like torn-ndjson's flag generalizes). Demote
  the two no-trigger Reachables to log lines until their seams exist.
  Resolve the cairo-text/engine-init contradiction by scoping the
  Unreachable to GL-healthy variants — or drop it; the `Always("content
  tile renders its text layer")` already carries the property.

### CW-6 — The concurrency cluster's value hangs on one unvalidated capability: thread pausing inside a dlopen'd cdylib

- **Properties:** `content-snapshot-torn-read`,
  `publish-visible-within-poll-bound`, `animating-gate-matches-stored-content`
  (and the thread-pause legs of `prompt-pulse-visibly-advances`).
- **Concern:** These are the catalog's best pure-exploration properties —
  microsecond windows (~2 instructions; between two lock acquisitions)
  that only scheduler manipulation reaches. Thread pausing requires
  coverage instrumentation, which the topology plans via the Rust SDK. But
  the instrumented artifact here is unusual: a Rust cdylib dlopen'd by an
  *uninstrumented* C++ GTK host, with the target windows on the GTK main
  thread and a detached reader thread. Whether the instrumentor's pause
  points land densely enough inside `set()`/draw-callback code — and
  whether pausing GTK's main thread interacts sanely with the frame
  clock — is unvalidated anywhere in the scratchbook. If pausing is
  ineffective in this shape, all three properties' Sometimes anchors go
  permanently red (CW-5) and their Always assertions become dead weight.
- **Scope:** Three properties plus the catalog's stated fallback for clock
  work ("thread pauses attack each link").
- **Evidence:** coverage_instrumentation.md ("For thread pausing to work,
  your system under test must be instrumented"; Rust listed as supported);
  `deployment-topology.md` fault table row; no validation plan in either.
- **Suggested action:** Make run-1 (or a probe run) explicitly test the
  capability: `content-snapshot-torn-read`'s Sometimes is *already
  designed* as the calibration signal — promote that from a remark to a
  gate: if it never fires in a dedicated churn run with instrumentation
  on, re-plan the cluster (e.g., widen windows with a SUT-side sleep seam
  under a test flag) before spending run-2 budget.

---

### Property-specific findings

### P-1 — `prompt-pulse-visibly-advances`: the P0's headline assertion cannot detect the failure it exists for

- **Concern:** `Sometimes("a displayed prompt pulse advanced phase between
  consecutive draws")` has global once-per-run semantics. A pulse that
  animates for one second at run start and then freezes for the remaining
  hours — the exact S3 failure mode, and the shape of both real dsl-cluster
  regressions — satisfies it. The supporting
  `Always("f32 clock advances between consecutive animated draws")` covers
  only the clock link (link 5); links 1-4 and 6 failing (gate stuck, tick
  callback lost, publish invisible) manifest as *no draws happening*, which
  a per-draw Always never evaluates — vacuous exactly when violated. The
  evidence file even notes this shape ("no draws at all … must be treated
  as failure signal, not vacuity") without an assertion that does so.
- **Evidence:** `properties/prompt-pulse-visibly-advances.md` (assertion
  design + frame-clock investigation); Antithesis Sometimes semantics.
- **Suggested action:** Add the windowed form as the primary oracle: in the
  150ms dirty-poll callback (which runs independently of the tick chain),
  `Always("pulse phase advanced within K polls while prompt markup is
  displayed")` — poll-counted per CW-3, catching both frozen-phase and
  frozen-draw shapes. Keep the Sometimes as coverage. Also: with the
  planned clock seam, the f32 leg becomes a parameterized deterministic
  check (pin 36h/12d/97d, draw twice) — cheap to also run outside
  Antithesis; the genuinely Antithesis-only content is links 1-4/6 under
  thread pause and CPU faults, which is precisely what the current
  assertion set doesn't fail on.

### P-2 — `watcher-key-survives-output-rename`: post-re-scope, expected-fail-by-design with no exploration content

- **Concern:** After the (correct, well-evidenced) investigation, the only
  exercisable trigger is a misconfiguration the workload inflicts on
  itself, and no recovery mechanism exists in the SUT — so the Always
  fails deterministically, by construction, the moment the misconfig
  driver runs, forever, until a fix that isn't planned. That is a
  deterministic demonstration, not a property Antithesis explores. Its
  real, durable value is the *shared oracle*: the quiescent-checkpoint
  "every live tile-watch key resolves in the cache" check, which
  `output-readd-tile-recovers` already embeds as its stranding detector
  and which doubles as harness deployment sanity.
- **Evidence:** `properties/watcher-key-survives-output-rename.md`
  (Investigation Logs settling both trigger questions; "misconfiguration
  variant" scoping).
- **Suggested action:** Retire it as a standalone exploration property;
  keep the Always as a shared convergence oracle inside
  `eventually_converged.sh` (no dedicated misconfig driver), and file the
  no-recovery behavior as the accept-or-fix design artifact it actually is.

### P-3 — `neighbor-modules-stay-live`: right umbrella, wrong sharp edge

- **Concern:** Beyond CW-1 (fs-latency angle) and CW-3 (stall budget vs CPU
  faults): the property's robust half is the canary; the SUT-side 250ms
  `Always` is the false-positive generator (llvmpipe + 10-instance draws +
  node throttling), and the evidence file concedes it's also vacuous for
  the permanent-wedge case it most wants (a wedged draw never returns to
  be measured). The value ordering should be inverted.
- **Evidence:** `properties/neighbor-modules-stay-live.md` (caveat
  paragraph; open question on budget vs llvmpipe false positives).
- **Suggested action:** Canary staleness (event-counted / quiescent-window)
  is the property; keep `Sometimes(">50ms draw")` as the exploration hint;
  demote the stall-budget Always to a generous fault-gated diagnostic or
  drop it.

### P-4 — `duplicate-line-rerender-idempotent`: assertion machinery outweighs the invariant

- **Concern:** Verified pure at f87ec19; purity of (line, config) → markup
  is a unit test (render twice, compare). The SUT-side Always requires the
  store to remember and memcmp the previous line to even *detect*
  re-delivery — instrumentation heavier than the property. The useful
  Antithesis-native residue is the Sometimes (respawn-driven duplicates),
  which is really a coverage anchor for `producer-kill-tile-reconverges`'s
  replay semantics.
- **Evidence:** Catalog row ("verified pure at f87ec19; pins the invariant
  against future time/random template dependence") — a future-regression
  guard that a fixed-clock unit test pins more reliably than runtime
  comparison.
- **Suggested action:** Move purity to a unit test; transfer the Sometimes
  to `producer-kill-tile-reconverges`; drop the standalone property.

### P-5 — Underestimated: the pipe-write and dlopen races are the catalog's purest Antithesis properties

- **Properties:** `torn-ndjson-frame-rendered`, `so-replacement-reload-race`,
  `orphaned-tile-watch-bounded`.
- **Concern (inverse direction):** All three sit at P2 while needing
  exactly the capability Antithesis uniquely has: landing SIGKILL between
  `write(2)` iterations of a >PIPE_BUF line; landing a file replacement
  inside waybar's sequential per-module dlopen pass (mixed versions within
  one generation) or after mapping (delayed SIGBUS); interleaving
  crash-loops with data-dependent orphan death (SIGPIPE-on-next-write). No
  deterministic test reliably reaches any of these windows. Post-fix-merge
  these are the properties that justify the platform.
- **Suggested action:** In the exploration-dependent axis proposed in
  CW-4, rank these at the top for run-2; ensure the workload's long-title
  lever (>4096B lines) and the three .so-mutator modes get first-class
  driver weight.

---

## Passes

- **Topology reasoning is sound.** Single container is forced by the
  no-network IPC design and verified honestly; the in-container supervisor
  substituting for node-termination faults matches the documented
  container-granularity fault model. Node hang/termination of the sole
  container even gives free organic coverage for the cold-start properties.
- **Clock-jitter skepticism is correct and doc-verified.** The catalog's
  planning default (SUT-side clock seam, treat tenant jitter as bonus)
  matches what the docs actually say (opt-in, ±30s example, POSIX clock
  unspecified). No property depends on undocumented clock behavior.
- **`publish-visible-within-poll-bound`** expresses its bound in poll
  periods with hysteresis — the scheduler-safe pattern the rest of CW-3
  should copy. Genuine thread-pause property (static-content leg has no
  rescue path); correctly Antithesis-shaped.
- **`contentstore-mutex-never-poisoned`** is correctly framed as a
  zero-cost ride-along tripwire, not an exploration target.
- **`torn-ndjson-frame-rendered`'s variant gating** ("gated by a harness
  config flag", refusal to move the assertion producer-side because the
  workload's own lever would trip it) is exactly the right discipline —
  it just needs to be generalized (CW-5).
- **Vacuity guards are systematic.** Nearly every Always ships with a
  coverage Sometimes ("teardown was triggered while tiles were live",
  "daemon restarted while a prompt session was live") — good practice,
  consistently applied.
- **The staleness/recovery core (Category 2 head) is well-fitted.**
  `cache-error-demotes-live-tile` (P0) and
  `daemon-restart-no-placeholder-clobber` (P1) target genuine
  timing-window races (throttled writes vs 75ms polls; DB-prime vs
  niri-event ordering with SIGKILL inside the window) with
  repair-suppression analysis that makes the windows explorable — this is
  the catalog at its best.
- **Screenshot-vs-counter oracle decision** (SUT-side draw counters
  primary, grim secondary) is well-investigated and right for a
  damage-gated GTK3 frame clock under a deterministic hypervisor.
- **Process-count invariants are constant-parameterized, not hardcoded**
  (dash-vs-ash `sh -c` doubling), with an explicit rebase note for the
  container's /bin/sh — avoids a classic harness false positive.

## Uncertainties

- **Thread-pausing efficacy in this artifact shape** (Rust cdylib inside an
  uninstrumented C++ host; pausing GTK's main thread vs the frame clock) —
  docs confirm the capability and Rust support but say nothing about
  granularity or mixed instrumented/uninstrumented processes. CW-6
  proposes the in-run calibration gate; only a probe run answers it.
- **Whether custom faults could emulate fs/EIO faults** (FUSE or LD_PRELOAD
  shim as a custom fault script) — technically plausible, wholly unplanned,
  and a nontrivial harness build; I evaluated CW-1 against the documented
  set plus the topology's planned scripts, not against hypothetical
  custom-fault engineering.
- **Determinism-friendliness of the cage/sway/niri/llvmpipe stack under the
  hypervisor** — shot.sh proves headless operation, not that the stack's
  timers/frame callbacks behave usefully under virtual time. The topology
  flags this assumption; nothing in my lens can resolve it from documents.
- **Sometimes-satisfaction rates for the legitimate exploration anchors**
  (e.g., how often a churn run actually lands `set()` inside a draw) —
  determines whether run-2 budget suffices; unmeasurable before a first
  run. The CW-6 calibration gate bounds the risk.
- **Whether the tenant has node faults/custom faults enabled** — the
  overview docs mark node faults, clock jitter, and custom faults as
  opt-in ("talk to your forward-deployed engineer"). The catalog's
  ⚠️custom-fault properties (7 of 41) assume custom faults are available;
  if the tenant hasn't enabled them, the workload-invoked form (test
  commands issuing kills/USR2 themselves) covers most but the
  random-interval injection quality differs. Needs tenant confirmation.
