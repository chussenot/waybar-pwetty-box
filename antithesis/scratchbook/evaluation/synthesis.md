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

# Evaluation Synthesis — waybar-pwetty-box property catalog

Four lenses ran against the 41-property catalog: `antithesis-fit.md`,
`coverage-balance.md`, `implementability.md`, `wildcard.md` (this directory).
Findings below are categorized as **Gap** (expand the catalog), **Bias**
(human judgment required), or **Refinement** (fix applied directly), with the
action taken. Where multiple lenses converged independently on the same
finding, that's noted — it was the strongest signal in the evaluation.

## Gaps (filled via targeted discovery — 10 new properties, Category 7)

| # | Finding (lens) | Action taken |
|---|---|---|
| G1 | Backend ingress & state derivation — the historically buggiest layer (six fixes) had **zero properties**; the exclusion was an accident of the attack-surface ranking, not a decision (wildcard W2 + coverage F1, convergent) | 4 new properties: `hook-prompt-never-silently-dropped`, `live-prompt-session-never-reaped`, `session-desktop-attribution-tracks-window`, `first-party-overlay-garbage-tolerant`. Historical mechanisms (133ae8d, 7d42f65, 1c26d14, e60a874) validated from fix commits per validating-claims. |
| G2 | Reader-thread panic ⇒ permanent, silent, trace-free tile freeze; no property named it; existing count equalities only catch it if two-sided (wildcard W5) | New `stream-reader-thread-liveness` (heartbeat + two-sided named-thread count + panic drop-guard); two-sided `==` made explicit in the conservation properties and `eventually_converged.sh`. |
| G3 | Attack surface #2 (draw-path panic aborts) had no dedicated property (coverage F3) | New `draw-path-gl-panic-tripwires` (3 Unreachables + armed-witness Sometimes; injectability limits recorded honestly). |
| G4 | Poll-mode hung-exec **staleness** owned by no property (coverage F6, wildcard W11.4) | New `poll-refresh-survives-hung-exec` (P3, gap demonstrator). |
| G5 | Claim L5 (idle decay reaches static) untested; testable with backdated timestamps, no clock fault (coverage F8) | New `idle-decay-reaches-static` — which **found a real HEAD divergence**: NULL-last_talk idle renders bright+animating forever via sessionTile vs dimmest via aggregate. |
| G6 | S8/bpe inter-renderer GL state had no regression guard and no recorded exclusion (coverage F4) | New `shader-pass-blend-state-neutralized` (P3 two-line tripwire) **with the alternative exclusion rationale** in its evidence file — owner picks (see Biases). |
| G7 | Claim L3 (active accent) untested; the needed workspace-churn driver also unblocks the injection properties' niri-title vectors (coverage F5) | New `active-accent-follows-focus` + `parallel_driver_focus_churn.sh` in the topology. |

## Biases (human judgment required — presented, not resolved)

1. **Testing boundary for the campaign** (wildcard W2): the gap-fill now
   covers backend ingress/derivation, but whether that layer is *in scope*
   for the Antithesis campaign (vs. Go unit-test territory) is a scope call.
   Evidence: six historical fixes there; the properties are drivable in the
   harness. Default taken: included at P1/P2.
2. **Supervisor-as-harness-fiction** (wildcard W3): production has no daemon
   restart mechanism; the harness supervisor erases the most probable
   production failure (daemon dies once → tiles frozen forever). Actions
   taken: kill-niri driver + withheld-restart variant added to the topology;
   the *design* question (should production ship a restart unit / staleness
   indicator?) is the owner's. Evidence: daemon.go exit-on-stream-close, no
   unit ships, no cache TTL.
3. **Priority semantics** (fit CW-4): P0-P3 encoded bug severity; Antithesis
   search budget should follow exploration value. Action taken: trigger-class
   second axis added (Amendments Rule 2) with deterministic-input properties
   demoted to pre-flight + ride-along. The owner may want to re-weight run-2
   drivers differently.
4. **`so-replacement-reload-race` economics** (wildcard W9): the finding
   indicts glibc/waybar loader behavior, not this repo; the already-designed
   (but fictional) pinned-copy + `pwetty-promote` mitigation is the correct
   fix regardless of run outcome and costs less than the three-mode mutator.
   Recommendation recorded: build the promote script first; keep only
   atomic-rename mode as cheap regression sanity; unlink/truncate modes
   if idle capacity.
5. **S8 property vs exclusion** (G6): keep the two-line blend tripwire, or
   take the recorded exclusion and pin the offscreen harness in CI instead.
   Both texts prepared; owner picks.
6. **The ~14 per-property `(needs human input)` design-intent questions**
   collected in the catalog (blank-on-GL-failure, empty-cache-on-read-error
   intent, unknown-state rendering, interval:0 contract, single-daemon
   enforcement, out-of-range idle_level policy, icon-read threading,
   post-fix orphan bound, writer-pause staleness, hook zero-trace loss under
   disk-full, gc model-adoption gate, reader-death fix policy, GL-unwrap
   degrade-vs-panic, NULL-last_talk rendering). None block workload
   implementation; they refine assertion strictness.

## Refinements (applied directly)

| # | Finding (lens — convergence noted) | Where applied |
|---|---|---|
| R1 | Wall-clock bounds in fault-exposed Always assertions blame the fault injector; `animating-gate`'s own prescribed fault violates its bound with zero defect (fit CW-3 + wildcard W6 + impl CW-3 — **three-way convergence**) | Catalog Amendments Rule 1 (event-counted or quiesce-then-check; `publish-visible-within-poll-bound` as the model; backoff-floor exempt); `eventually_converged.sh` named as the quiesce evaluation point. |
| R2 | Fixture strategy falsified: GC reaps DB-injected sessions in ~1-2s; no-window sessions never render; Title is not a DB column (wildcard W1 + impl CW-2 + coverage F2 — **three-way convergence**) | Topology "Backend data plumbing" rewritten: real-window fixtures, NULL terminal_pid, hook ingress preferred, title via windows, injection-quiesce rule for direct tiles.json writes (also impl CW-4/wildcard W8). |
| R3 | Harness config as sketched ships the watcher-key bug (`tile-watch N` flagless → HDMI-A-1 vs niri's `winit` output → all tiles placeholder forever) (impl CW-6) | Topology: `--output winit` mandatory; output-unpinned bar config for the readd property. |
| R4 | Nonexistent fault types cited (fs/EIO, fs-latency, memory-pressure, fd-exhaustion) (fit CW-1 + impl F14) | Amendments Rule 4; four SUT-side test seams added to the topology (markup export, GL failure, additive clock offset, reader heartbeat). |
| R5 | Reachability audit: structurally-red Sometimes/Reachables; `cairo-text` Unreachable contradicts the GL-degraded variant by design (fit CW-5 + impl F14) | Amendments Rule 3 + topology run-variant × assertion matrix requirement; the contradicting Unreachable variant-gated. |
| R6 | Trigger-class / search-budget axis (fit CW-2 + CW-4) | Amendments Rule 2; pure-function subset moved to deterministic pre-flight; exploration-dependent set named for run-2 driver weight. |
| R7 | `watcher-key-survives-output-rename` post-re-scope is a self-inflicted misconfiguration with no SUT recovery — not an exploration property (fit P-2 + impl F7) | Retired to the shared stranding oracle in `eventually_converged.sh`; accept-or-fix artifact flagged for the owner. |
| R8 | `duplicate-line-rerender-idempotent`: machinery outweighs invariant; purity is a unit test (fit P-4 + impl F19) | Moved to pre-flight; Sometimes transferred to `producer-kill-tile-reconverges`; sort-by-name comparison noted. |
| R9 | `neighbor-modules-stay-live`: canary is the property; the 250ms Always is a false-positive generator and vacuous for the permanent wedge (fit P-3 + impl F9) | Amendment: canary primary, stall-budget demoted to fault-gated diagnostic; FIFO-wedge lever isolated to a terminal-phase variant (impl F9). |
| R10 | `prompt-pulse-visibly-advances`: once-per-run Sometimes can't detect freeze-after-start (the historical failure shape); clock seam as described panics on underflow; f32 regimes are a parameter sweep (fit P-1 + impl F11 + wildcard W11.7) | Amendment: windowed poll-counted Always primary; seam respecified additive; f32 regimes to seeded unit test — the P0 loses its ⚠️clock dependency. |
| R11 | `stream-line-length-bounded` asserts a cap that doesn't exist in code (impl F10) | Amendment: re-scoped assert-after-fix; RSS ceiling carries the risk meanwhile. |
| R12 | `static-idle-redraw-budget` fps-leg tautological; hover legitimately queues redraws on static tiles (impl F12 + wildcard W4) | Amendment: shared clamp helper with `idle-level-gate-clamp-divergence`; "no pointer interaction in flight" scope. |
| R13 | `shader-recompile-gl-object-leak` RSS proxy not viable; SIGABRT endpoint unreachable in-run (impl F15) | Amendment: SUT counter mandatory; endpoint documented not expected. |
| R14 | `cache-error-demotes-live-tile` target branch is a fall-through needing restructure (impl F18) | Amendment: restructure noted for workload budgeting; demotion-instant firing noted (no hour-scale runs needed). |
| R15 | Markup unobservable from the workload; pixel oracles can't match animated content (impl CW-1) | Topology: markup export seam; four properties' oracles re-anchored (Amendment). |
| R16 | Direct tiles.json injections clobbered by daemon dedupe within seconds (impl CW-4 + wildcard W8) | Topology: injection phases quiesce the daemon or use a workload-owned cache path. |
| R17 | Thread-pausing efficacy in a dlopen'd cdylib is the concurrency cluster's single point of failure (fit CW-6) | Topology: probe run with the `content-snapshot-torn-read` Sometimes as an explicit calibration gate. |
| R18 | Exploration economics of a polling SUT never assessed (wildcard W7) | Topology: probe-run acceptance metric (branches/sec, unique-behavior rate); quieter payloads / lower fps mitigations listed. |
| R19 | Validating tee changes process counts and SIGPIPE topology (impl F17) | Topology: tee gated to contract-focused variants with adjusted constants. |
| R20 | Stale ⚠️node-term cross-reference; supervisor must not kill the process group; llvmpipe thread pools pollute thread counts; agentic-db unpinned with drifting line refs; circular `pwetty render` oracle unlabeled; two-compositor validity caveat (wildcard W11.1/W11.5, impl CW-5, coverage F11, wildcard W10) | All fixed in the topology (supervisor caveats, LP_NUM_THREADS, pinned-commit note, plumbing-check label, validity caveat). |
| R21 | `existing-assertions.md` never scanned agentic-db (wildcard W11.2) | Scanned (zero references); file updated — all Go assertions confirmed net-new. |
| R22 | `cffi-v1-config-transport-retype` Always lacks ground truth (impl F13) | Amendment: harness expected-types manifest; pre-flight classification. |
| R23 | Wall-clock discontinuity (suspend/NTP vs backend "ago" labels) unexamined (wildcard W11.6) | Recorded as a deliberate exclusion: S5-severity cosmetic outcome, clock faults opt-in; revisit only if the tenant enables clock jitter. |
| R24 | Deliberate-exclusions rationale missing for hover/Pango-ink/marquee/F11-radius (coverage F10, wildcard W4) | This synthesis records it: excluded because they are S5-cosmetic under the severity model, need pixel oracles or pointer injection the harness lacks, and their bug history is metric-tuning, not state-machine defects. Pointer injection noted as an untapped future fault dimension. |

## Convergence notes

Three findings were reached independently by 2-3 lenses (wall-clock bounds,
broken fixture strategy, nonexistent fault types) — treated as
highest-confidence. The wildcard's W1 (fixture/GC) materially invalidated a
topology section that all other lenses had accepted as ground truth;
lens-independence did its job.

## Re-evaluation decision

Gap-fill added a new category (10 properties — substantial per the skill's
threshold). A second full evaluation pass was **not** run: the new properties
were designed after and under the amendment rules the first pass produced
(event-counted bounds, variant gating, fixture constraints, honest
injectability limits), and the two systemic risks that would warrant
re-evaluation (fixture viability, wall-clock oracles) are exactly what they
were built to respect. The fresh-context self-review (skill requirement)
serves as the independent check on the integrated catalog.
