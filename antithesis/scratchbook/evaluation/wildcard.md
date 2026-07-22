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

# Wildcard Evaluation — waybar-pwetty-box property catalog

Role: fourth lens, no fixed rubric. The other three lenses accept the catalog
and (transitively) the SUT analysis; this pass questioned both, read the
backend repo the fixtures depend on, and looked for perspectives none of the
fixed lenses constructs. Everything below is grounded in code read at
f87ec19 (plugin) and the current ~/agentic-db tree (backend).

---

## W1 — The session-fixture mechanism is falsified by the backend's liveness GC and window join (catalog-wide, harness-breaking)

The deployment topology's entire session-fixture strategy is: "the workload
drives the production path from its true ingress: writing session rows/state
to the claude-status SQLite DB" (`deployment-topology.md`, Backend data
plumbing). Two backend mechanisms, neither mentioned anywhere in the
scratchbook, break this as designed:

1. **The daemon GC reaps DB-injected sessions within seconds.**
   `~/agentic-db/internal/daemon/gc.go` `deadPredicate`: a session is dead if
   its `window_id` is **absent from the live niri model** (immediate, no
   debounce), OR its `terminal_pid` has no `/proc/<pid>`, OR first-party
   absence persists 3 ticks. GC tick is 1s. A fixture row with a fabricated
   `window_id` or `terminal_pid` is reaped in ~1–2s.

2. **Sessions without a live window never render at all.**
   `~/agentic-db/internal/tile/tile.go` `BuildAll`:
   `if !s.WindowID.Valid { continue }` and the window id must resolve in
   `winByID` (the live niri window set) to join a workspace. So the GC's
   escape hatch (NULL window_id = "non-local", never reaped) produces a
   session that is *invisible to every tile*.

Net: **a DB write alone can never place a `prompt` on a tile.** Every
property whose workload precondition is "while a fixture session holds
prompt" rests on this lever — at minimum `cache-error-demotes-live-tile`
(P0), `prompt-pulse-visibly-advances` (P0), `daemon-restart-no-placeholder-clobber`,
`prompt-priority-survives-session-cap`, `cold-start-stream-tile-converges`,
`output-readd-tile-recovers`, `unknown-session-state-renders-blank` (Go leg),
`tile-watch-output-schema-valid`. Lens 3 evaluates implementability against
the topology *as documented*; the topology's claim is wrong, so all three
fixed lenses inherit the error.

**Viable fixture shape** (needs to be written into the topology): one real
window per target niri workspace (any trivial Wayland client), session row
carrying that real `window_id`, `terminal_pid` NULL (skips the /proc check),
and the first-party sessions dir absent or `-sessions-dir ""` (fpAvailable
false ⇒ no first-party reaping). Corollaries:

- niri's **dynamic workspaces** mean the number of desktops (and hence which
  `winit:N` cache keys exist) is itself a function of window placement —
  the harness must place windows to make desktops 1–3 exist at all, or
  tile-watch 3 polls a missing key forever (indistinguishable from the
  stranded-watcher failure it's supposed to detect).
- The session-churn driver closing windows **renumbers workspace indexes**
  (already known from the rename investigation) — fixture keys shift under
  the churn driver's own activity.
- Better still: use the real ingress. `claude-status hook` (see W2) creates
  rows exactly the way production does, resolving a real niri window — one
  binary invocation per state flip, and it sidesteps SQLite locking-fidelity
  questions (the hook opens the DB with WAL + busy_timeout(2000),
  `internal/db/db.go:341`; a raw `sqlite3` CLI writer would not).

## W2 — The historically buggiest layer is outside both the catalog and the harness, and it didn't have to be (catalog-wide, framing)

SUT analysis §5 names the backend hotspot: six fixes in session-state
derivation ("reaper liveness, idle-nudge strands '?', waiting→'?' false
positives, nondeterministic window pick, stale-busy inversion, cwd
resolution"). §12's ranked attack surfaces then silently drop it — the
ranking stops at "contract conformance" of payloads. The catalog inherits
the blind spot: **0 of 41 properties assert on state derivation, session→
workspace association, the reaper, or the first-party overlay.** Lens 2
audits the portfolio against §12's risk areas, so it cannot flag a risk area
§12 itself dropped.

This exclusion is usually justified as "no real Claude Code in the harness,"
but the boundary is drawn too far downstream. Three ingress mechanisms are
exercisable with zero Claude Code:

- **The hook hot path**: `claude-status hook` reads a JSON event from stdin
  and upserts a row (`internal/hook/hook.go`). It is documented as "must
  NEVER block or fail Claude. Run swallows every error… and always returns
  nil." That is a *by-design silent-failure ingress*: a hook that fails to
  record a `prompt` transition is the purest form of the F9 alert-loss the
  product exists to prevent, with zero trace anywhere downstream. Feeding it
  synthetic/recorded hook JSON is a trivial workload driver.
- **The first-party overlay**: `overlayFirstParty` (daemon.go:306,
  reconcile.go:66-78) refines session state every tick from
  `~/.claude/sessions/<pid>.json` — an **undocumented, version-drifting
  format** (`internal/clauded/clauded.go` says so explicitly, including the
  overloaded `waiting` semantics that already caused a bug). The daemon takes
  `-sessions-dir`; the workload can write partial/garbage/format-drifted
  files there. This is ideal Antithesis filesystem-mischief territory and it
  is structurally disabled in the harness as designed (no dir ⇒ overlay code
  never runs).
- **The reaper itself**: "a live prompt session is never reaped" is a
  missing property with a *proven* bug class behind it ("reaper liveness" is
  a fixed bug). A GC false-positive deletes the alert — silent, S2, and the
  mechanism (window-model staleness racing niri events, first-party
  miss-counter races around the 3-tick threshold) is exactly
  fault-timing-sensitive.

Suggested action: either extend the catalog with an ingress/association
property cluster (hook-path row fidelity; prompt-never-reaped-while-window-
live; session renders on exactly the desktop its window occupies; overlay
tolerance), or record explicitly in the SUT analysis that the tested
boundary starts at the DB and why — right now the exclusion is an accident
of §12's ranking, not a decision.

## W3 — The in-container supervisor erases the most probable production S2 scenario (catalog-wide)

Production facts, all confirmed in the scratchbook itself: the daemon exits
when the niri event stream closes (daemon.go:217-221); **no restart unit
exists**; the cache has no TTL; tile-watch trusts it unconditionally. So the
single most probable production catastrophe for the product's purpose is:
*daemon dies once, nothing restarts it, every tile shows plausible frozen
state forever.*

The harness supervisor auto-restarts the daemon, so this scenario **cannot
occur in any run**: all daemon-death properties test restart *recovery*
instead. The topology frames the supervisor as a substitute for
node-termination faults, but it is also a SUT modification that deletes a
failure mode. Meanwhile the driver list has no **kill-niri** driver, despite
niri-death being the documented trigger of daemon exit (and, in the nested
topology, cheap to inject).

Suggested action: add a kill-niri driver; add a withheld-restart variant
(supervisor delays daemon restart by a long random window) with a property
on what the user sees during it (today: nothing — which is the finding); at
minimum record in the topology that supervised-restart is harness fiction
relative to production.

## W4 — No pointer events: the newest code in the repo is dead code in the harness, and one property is only accidentally true (catalog-wide + `static-idle-redraw-budget`)

The last four commits before f87ec19 are hover/cosmetic machinery: fading
cooldowns (170335f), focus-bubble corner_radius (e9405f4), tile typography
(c33331b), hover ring fade (49d377b). The catalog leans on churn-history to
prioritize ("the gating subsystem is the proven highest-churn regression
cluster") but its churn snapshot predates this — **recency-weighted churn is
now in the pointer-interaction path, which no property touches and the
workload cannot reach** (no input injection anywhere in the topology).

Concrete interaction found in code (src/lib.rs:97-155): each enter/leave
flip spawns a `timeout_add_local(33ms)` loop calling `queue_draw` until the
160ms fade completes (plus one extra tick). Two consequences:

1. `static-idle-redraw-budget` asserts `Always("static tile content queues
   no frame-clock redraws between content changes")`. A legitimate hover on
   a static tile queues redraws. The property holds in the harness **only
   because pointer events don't exist there** — if anyone later adds pointer
   injection (sway can synthesize it), the P1 property false-positives on
   correct behavior. The assertion needs a "no pointer interaction in
   flight" scope, or must count only content-driven redraw sources.
2. Rapid enter/leave storms stack concurrent 33ms timers (each self-
   terminates one tick after its fade window — bounded, but N-fold transient
   redraw amplification, and each holds a strong widget ref across teardown
   — the same pattern as the leaking 150ms poll). Nobody modeled a hover
   storm racing SIGUSR2 teardown.

Suggested action: scope the redraw-budget assertion now (cheap); note
pointer injection as an untapped fault dimension; decide explicitly whether
hover paths are in or out of scope for this campaign.

## W5 — Reader-thread panic ⇒ permanent, silent, trace-free tile freeze; no property names it (catalog-wide gap with a cheap fix)

`Cargo.toml` sets no `panic = "abort"` — panics unwind. Template rendering
(minijinja), markup composition, and JSON parsing all run **on the detached
reader thread**, *outside* the content lock: `publish.set(build.content(
&parse_data(&line)))` (content.rs:275) evaluates `build.content(...)` before
`set` is entered. So a panic anywhere in the render path:

- kills only the reader thread (no host abort — the S1 framing in SUT §7
  covers main-thread panics only);
- does **not** poison the mutex (panic is outside the lock), so
  `contentstore-mutex-never-poisoned` — the only adjacent property — can
  never observe the common case; poisoning requires the panic inside `set()`;
- drops the `BufReader` (unwinding closes the pipe read end) ⇒ producer
  SIGPIPEs on next write and dies ⇒ **no respawn** (the respawn loop lived
  in the dead thread) ⇒ tile frozen at last content forever, zero log lines.

This is the worst S2 shape (permanent + silent + no observable) reachable by
one panic in dependency code under hostile input, and the catalog's only
coverage is accidental: `reload-conserves-producer-chains` asserts chain
count "equals" module count, and `eventually_converged.sh` checks counts
"proportional" — a dead chain is caught **only if these are enforced
two-sided (== not ≤)**, which no property text makes explicit (they're all
framed as leak detectors, upper-bound in spirit).

Suggested action: make the count equalities explicitly two-sided in the
property texts; add a trivial reader heartbeat (`Sometimes("stream reader
iterated")` per module, or a workload `Always("every stream module has a
live reader thread")` at checkpoints). One line of instrumentation converts
an invisible failure class into a detectable one.

## W6 — Wall-clock-bounded `Always` liveness assertions are falsified by the fault injector itself (cross-cut)

Several workload `Always` assertions carry inline wall-clock bounds:
reconverge within 5s (`producer-kill-tile-reconverges`), valid line within
10s (`stream-recovery-after-framing-violation`), pulsing within 3s
(`prompt-priority-survives-session-cap`), reconverge within 500ms
(`animating-gate-matches-stored-content`), two poll periods
(`publish-visible-within-poll-bound`). Antithesis's whole method is to
stretch schedules; CPU-throttle and thread-pause faults legitimately blow
every one of these bounds on *correct* code.

The sharpest case is `animating-gate-matches-stored-content`: its own
Antithesis Angle prescribes pausing the reader **mid-`set()`**, and its
Property text sanctions divergence "only while a set() is in flight" — but
under a pause, a `set()` *is* in flight for the entire pause, so the 500ms
`Always` fires with no bug present. The only code shape that satisfies the
bound under arbitrary pauses is an atomic single-lock `set()` — i.e., the
property quietly demands a specific fix while claiming to tolerate the
current design. It cannot distinguish sanctioned in-flight divergence from
the defect. Reformulation: assert convergence at fault-quiet checkpoints
(`ANTITHESIS_STOP_FAULTS`, already in the topology), or assert on `set()`
duration/ordering directly (dirty-before-unlock), not on wall-clock
observation windows.

`prompt-priority` self-flags its 3s bound as an open question; the pattern
deserves one catalog-wide rule rather than per-property rediscovery: **every
wall-clock `Always` must either carry a fault-quiet precondition or be
recast as quiesce-then-check.** Lens 3 will likely check each bound's
observability; the systemic false-positive mechanism is the part that needs
naming once.

## W7 — Exploration economics of a polling-and-rendering SUT were never assessed (catalog-wide)

Nothing in the scratchbook estimates whether this SUT is *explorable* at
useful depth. The stack polls at every layer — 13ms DB poll + debounce, 75ms
× N tile-watch, 150ms dirty poll × M instances, 30fps animated tiles under
llvmpipe software rendering, 33ms hover timers — and Antithesis pays for
every branch of that busy machinery in every explored timeline. The
interesting state space (discrete lifecycle events: kills, reloads, torn
writes) is tiny relative to the render/poll noise, and llvmpipe makes each
frame genuinely expensive. Risk: runs spend their budget re-exploring poll
iterations and shader rasterization instead of interleavings.

Mitigations that don't change the SUT: run-1 with 2 tiles (the topology
already asks); quiet payloads outside driver activity; consider a harness
config with lower target fps for non-animation-property runs. Suggested
action: make "branches/sec and unique-behavior discovery rate" an explicit
probe-run acceptance metric before committing long runs.

## W8 — The direct-tiles.json lever races the live daemon (property-specific: `static-idle-redraw-budget`, `idle-level-gate-clamp-divergence`, `unknown-session-state-renders-blank`, `tile-watch-output-schema-valid`)

The daemon's dedupe compares against `d.lastTiles` — *its own last marshal*,
not file content (daemon.go:151-153, 414-418). A workload-injected
tiles.json payload is therefore invisible to the daemon and silently
clobbered by its next differing write: ~250ms-throttled under churn,
potentially minutes on a quiet desktop. Precondition duration for every
"inject via direct cache writes" property is thus nondeterministic and
coupled to unrelated driver activity. No document states the interaction
rule. Suggested action: pause/stop the daemon during lever-2 injections, or
point the variant tile's tile-watch at a workload-owned cache path — one
sentence in the topology, but it must be written down before workload
implementation bakes in flaky preconditions.

Related, same file: `writeTiles` failure (e.g. ENOSPC) is log-and-return
(daemon.go:414-417) — retried next tick, but persistent disk pressure is a
silent-staleness leg. Note the cross-property coupling: the shader-churn
driver's per-frame stderr spam (a property's own trigger) is a disk-filling
mechanism that can *induce* daemon write failures in the same container —
a fault combination no lens constructs because it crosses cluster
boundaries (Cluster A trigger → Cluster C effect via shared disk).

## W9 — `so-replacement-reload-race`: the run can't change the decision (property-specific)

Cross-lens tension worth naming: the property is production-real (elevated
by investigation) and implementable (file mutator), but its possible
findings indict **glibc's dlopen and waybar's loader path**, not this repo —
no code change in pwetty-box alters whether a torn .so maps and faults. The
already-designed mitigation (pinned copy + `pwetty-promote`, currently
fictional) is the correct fix *regardless of what the run finds*, and
building it costs less than building the three-mode file mutator. The
property's real value is deployment-practice validation, which needs no
multiverse. Suggested action: build the promote script first; keep only the
atomic-rename mode as a cheap regression sanity in the harness; downgrade
the unlink/truncate modes to "if idle capacity".

## W10 — Two-compositor validity caveat (property-specific: `output-readd-tile-recovers`, hotplug leg of `module-teardown-never-aborts-host`)

In production, waybar and the daemon observe the *same* compositor (niri).
In the sway-outer harness, waybar's outputs are sway's; the daemon's
desktops come from nested niri's single hardcoded output. Output-identity
findings from the harness therefore validate waybar-on-sway behavior, not
the production identity flow — the dimension (F10) that motivated the
properties is exactly the one the topology bifurcates. The topology
acknowledges the rename-stranding trigger is impossible; it does not state
the broader transfer caveat. One paragraph in each property's evidence file
would prevent over-claiming from a green run.

## W11 — Oddities (small, enumerated)

1. **Stale cross-reference**: topology says the supervisor "satisfies every
   property marked ⚠️node-term in the catalog"; the catalog's legend says no
   property carries that marker (it was evidently removed in a revision).
   Cosmetic, but it will confuse the workload implementer.
2. **existing-assertions.md never scanned agentic-db**, yet the catalog
   places five properties' Go assertions there. The file flags this itself;
   it's a one-grep close-out that should happen before workload work.
3. **Circular oracle**: `eventually_converged.sh` compares rendered content
   against `pwetty render` of the same payload — the CLI links the same
   rlib (Cargo.toml `crate-type = ["cdylib", "rlib"]`), so template/markup/
   render bugs cancel out. Valid as a *plumbing* divergence check; should be
   labeled as such so nobody reads it as rendering correctness.
4. **Poll-mode hung exec**: confirmed `run_command` has no timeout
   (content.rs:288-296). The catalog's own open question ("coverage hole…
   never-exiting-producer-under-poll") is real: memory is covered
   (`stream-ingest-memory-bounded`), the *staleness* of a wedged poll thread
   is owned by no property. Small, poll mode isn't production wiring — but
   it's one `Sometimes` away from closed.
5. **llvmpipe worker threads**: Mesa spawns per-context rasterizer pools;
   "waybar thread count stays flat across reloads"
   (`reload-conserves-producer-chains`) will be noisy unless the harness
   sets `LP_NUM_THREADS=0` or the assertion counts only plugin-named
   threads. Worth one line in the topology env.
6. **Wall-clock discontinuity unmodeled**: idle decay and "ago" labels
   derive from wall time in the backend; suspend/resume and NTP steps are
   daily-driver events on this hardware and the natural use of Antithesis
   clock faults — yet clock faults are scoped exclusively to the f32
   monotonic leg. Probably S5 (absurd "ago", idle-level jumps), but nobody
   *decided* that; it's unexamined.
7. **`prompt-pulse-visibly-advances` f32 leg is a parameter sweep**: with
   the env-var start-offset seam, the 36h/12d/97d quantization regimes are
   deterministic single-parameter tests — cargo-test territory, not
   multiverse territory. Keep the six-link integration property for
   Antithesis; move the quantization regimes to a seeded unit test and the
   P0 loses its only ⚠️clock dependency.
8. **"No durable state in the plugin" framing**: ICON_CACHE negative
   entries, INK_CACHE growth, and leaked reader generations are
   process-lifetime state in a process that runs for weeks — behaviorally
   durable across the horizon that matters. The framing licenses "crash
   loses only animation phase," which understates why waybar restart is the
   only healer for several failure classes the catalog itself documents.

---

## Passes (checked, look right)

- Catalog internal consistency: 41 properties claimed, 41 evidence files
  present, slugs match; severity/priority tags coherent with sut-analysis §9.
- The p9c masking/sequencing logic (run teardown workloads against the fix
  branch; Cluster A dominance) is sound and consistently applied across
  catalog, relationships, and topology phasing.
- The `<glow>`/detector, SIGPIPE-141, dash-no-exec, reload-unconditional,
  and Pango-C0 investigation results quoted in the catalog match what the
  code and the evidence files support; I found no overclaimed "confirmed".
- `watcher-key-survives-output-rename`'s re-scope to the misconfiguration
  variant is honest and correctly propagated into the topology's settled
  questions.
- Shared-instrumentation dedupe (generation counter, out-of-range idle
  Sometimes, validating tee) is real and correctly cross-referenced in
  property-relationships.md.
- The relationships file's dominance claims spot-checked against code paths
  (line-cap ⇒ RSS stream leg; gate-vs-tick-source complementarity) — hold.
- Two-sided risk of `duplicate-line-rerender-idempotent` (crash-looping
  producer ⇒ 1Hz redraw) is correctly surfaced as an open question rather
  than asserted.

## Uncertainties

- **GC reap timing vs fixture windows**: window-absence reaping is
  immediate-per-tick in code, but I did not measure the end-to-end window
  (DB write → daemon snapshot → GC tick) — the fixture-viability claim (W1)
  is mechanism-certain, magnitude-estimated (~1–2s).
- **Whether the fixed lenses' briefs cover W6's systemic form**: Lens 3
  plausibly flags individual bounds; I could not determine from the briefs
  whether any lens owns catalog-wide assertion-design rules. Flagged anyway
  per wildcard mandate.
- **Hover-vs-teardown race severity** (W4.2): strong-ref pattern confirmed
  in code; whether a stale hover timer firing across module dispose can do
  more than waste a tick (GTK ref-counting should make it benign) was not
  probed.
- **llvmpipe thread-pool shape** (W11.5): per-context pool behavior asserted
  from Mesa knowledge, not measured in this stack; the mitigation
  (LP_NUM_THREADS / named-thread counting) is cheap either way.
- **Whether `claude-status hook` can fully replace DB-write fixtures** (W1/
  W2): the hook resolves a niri window itself (SessionStart path walks
  /proc ancestry to find the terminal); driving it from a workload shell may
  need a real terminal-like process tree. Mechanism exists; ergonomics
  unverified.
