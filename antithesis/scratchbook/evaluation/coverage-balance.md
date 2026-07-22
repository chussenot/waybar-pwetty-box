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

# Coverage-Balance Evaluation — property-catalog.md (41 properties)

Lens: is this the right *set*, judged as a portfolio against `sut-analysis.md`
(esp. §12 ranked attack surfaces, §9 severity model, §4 claims, §5 bug
history) and `deployment-topology.md`. Property-level quality is out of scope
for this evaluation; other agents cover it.

Method: mapped every catalog property to (a) the §12 attack surface it serves,
(b) the §4 claim it tests, (c) the component it asserts in, (d) its assertion
types; then walked the SUT analysis section by section looking for risk with
no property, and the topology looking for properties whose levers don't exist.
Spot-verified load-bearing claims in both codebases (tiles/claude/tile.json
template; agentic-db `internal/{hook,clauded,daemon,tile}`).

## 1. Coverage matrix vs §12 ranked attack surfaces

| # | Attack surface (rank) | Properties | Verdict |
|---|---|---|---|
| 1 | Teardown / GL context lifecycle | module-teardown-never-aborts-host (P0), engine-init-failure-contained, output-readd-tile-recovers, cairo-text-survives-gl-failure | **Dense, well-sequenced** (fix-branch phasing handled) |
| 2 | Host abort via draw-path panics | *(none dedicated)* — indirect only: shader-recompile-gl-object-leak's escalation endpoint; workload `finally_no_crash` | **Under-covered relative to rank #2** — see Finding F3 |
| 3 | Reload accumulation | reload-conserves-producer-chains (P0), orphaned-tile-watch-bounded, so-replacement-reload-race, respawn-backoff (aggregate leg) | Dense; masking by #1 explicitly handled |
| 4 | Silent staleness of data chain | 9 properties (Cluster C) | Dense — but the *poll-mode hung-command* staleness leg from §7 is missed (Finding F6), and daemon-death-permanent-staleness question dropped (Finding F9) |
| 5 | Stream seam robustness | torn-ndjson, stream-recovery-after-framing-violation, stream-line-length-bounded, producer-kill-tile-reconverges, respawn-backoff | Complete — torn/garbage/huge/kill/backoff all present |
| 6 | Animation gating both directions | 7 properties (Cluster D) | Complete — frozen + heat directions, detector blind spot (`<glow>`) caught, complementary pair explicitly disjoint |
| 7 | Config/preset degradation | config-resolve-preserves-tile-identity, cffi-v1-config-transport-retype | Adequate (unknown-preset arm and type-collapse both inside config-resolve) |
| 8 | Resource boundedness long-run | shader leaks, fork storms, RSS ceilings, f32 clock (prompt-pulse leg) | Mostly covered; INK_CACHE growth demoted to an open question, never a property (Finding F7) |
| 9 | Contract conformance | tile-watch-output-schema-valid, unknown-session-state, idle-level-gate, producer-binary-swap | Complete at the tiles.json seam |

Attack surfaces get properties roughly proportional to rank, with one
inversion: #2 has effectively zero dedicated coverage while #5/#6 are saturated.

## 2. Coverage matrix vs §4 claimed guarantees

Claims are the SUT analysis's own "claims to test, not facts". Tested:
S1 (indirectly via config-resolve), S2, S3 (partially — C0/U+FFFC vectors only),
S4, S5, S6, S9, S11, S12, L2, L4 (loose bounds), L7, L8, L9.

**Untested, with no recorded exclusion rationale anywhere in the catalog:**

- **S8 — deterministic GL state between renderers.** This claim exists
  *specifically as the regression guard for bead bpe*, and §5 puts GL-state
  bugs in the highest-severity density cluster. The catalog covers GL
  *lifecycle* (p9c family) exhaustively but inter-renderer *state* pollution
  not at all. See Finding F4.
- **L3 — desktop switch → active-accent repaint.** No property touches the
  `active` flag / `<active/>` accent, and the topology has no workspace-churn
  driver. See Finding F5.
- **L5 — idle decays through 7 levels; tile eventually goes static.** The
  backend decay progression has no property; the catalog covers only
  out-of-range levels (injected) and the redraw budget *given* a level.
  The never-reaches-static failure is the S11 heat bug arriving via backend
  arithmetic instead of via gating. See Finding F8.
- **L6 — marquee shows every glyph.** Zero properties in the entire
  Pango-ink/metrics area, which §5 calls "most filed bugs" (4up, e0r, 1ml
  + re-fixes). Defensible under the severity model (S5 cosmetic, vision-hard
  oracles) — but the catalog nowhere says it decided this. See Finding F10.
- **S7, S10, L1-direct** — S7 (default framebuffer never used) and S10
  (no DRM/seat) are effectively proven by the harness environment itself
  (surfaceless, no /dev/dri); acceptable silent passes. L1's tight 300ms
  bound is untested directly; the looser bounded-visibility properties are
  the right call under Antithesis pauses (deliberate loosening, worth one
  line of rationale).

## 3. Component balance

Rough attribution of the 41 properties by where their primary assertion lives:

- **Rust plugin (in waybar): ~31/41 (~76%).**
- **Go daemon: ~6** — cache-error-demotes, daemon-restart-no-placeholder-clobber,
  tile-cache-never-torn, prompt-priority (Go arm), unknown-session-state (Go arm),
  tile-watch-output-schema-valid (Go Sometimes). **Every one sits on the
  tiles.json/payload seam** (`writeTiles`, `ReadCache`, `PayloadFor`,
  `sessionTile`).
- **tile-watch subprocess: ~4** (schema-valid, watcher-key, producer-binary-swap,
  orphaned/reload counts) — again all seam-level.
- **Host (waybar) itself: asserted about from outside (survival), never probed**
  — the unimplemented ABI surface (`wbcffi_update/refresh/doaction`) that §5
  calls "suspiciously quiet" has zero properties, not even a
  `Reachable("wbcffi_update invoked")` to establish whether it's exercised.

The daemon is a full actor-model program (niri event-stream handling,
reconcile, session→desktop mapping, GC/reaper, decay computation, git
`unpushed` derivation, icon resolution) and its *internals* have zero
properties. §5 explicitly flags the backend as the bug-history hotspot
("six fixes... reaper liveness, idle-nudge strands, waiting→? false
positives, nondeterministic window pick, stale-busy inversion, cwd
resolution"). Two of those six (window pick, cwd resolution) are
session→desktop *attribution* bugs — the failure mode where a prompt pulses
on the wrong desktop's tile, which is F9-equivalent in consequence (the user
looks at the wrong workspace; the alert is functionally lost) and is
exercised by the harness (daemon runs real reconcile against nested niri).
See Finding F1.

## 4. Topology cross-check (does the catalog's coverage actually materialize?)

Two problems where coverage exists on paper but the planned harness can't or
won't deliver it, and one in the opposite direction:

**(a) The workload's DB-direct ingress bypasses the bug-densest backend code
and may get its fixtures garbage-collected.** Verified in agentic-db:
`internal/hook` is the real ingress (maps Claude hook events →
states — `hook_test.go:57` TestEventStateMapping); `internal/clauded` parses
Claude's first-party session files; and `internal/daemon/gc.go` reaps a
session when its `terminal_pid` has no `/proc/<pid>`, its niri window is
gone, or its first-party `~/.claude/sessions/<pid>.json` is absent
(gc.go:27-46, 80). The topology's `helper_session.sh` "wraps the DB writes".
Consequences:
  1. `internal/hook` + `internal/clauded` (the undocumented-format parser,
     the six-fix hotspot's substrate) are **never exercised in any run** —
     an unstated scope-out.
  2. Worse, **workload-fabricated sessions are GC bait**: without live
     backing PIDs, real niri windows, and plausible session files, the
     daemon's reaper can delete injected sessions mid-scenario. That would
     silently gut Cluster C and D properties (sessions vanish → `prompt`
     never held → vacuity guards like `Sometimes("daemon restarted while a
     prompt session was live")` never fire, and the run looks green). No
     property, assumption, or topology note addresses this interaction.
     This is the single highest-leverage fix in this evaluation because it
     protects ~16 existing properties rather than adding one.

**(b) Several property vectors have no planned lever.** The C0/U+FFFC
injection properties list niri window titles as the interesting vector, both
carry open questions "can X survive niri plumbing?", and the harness can
answer them — nested niri is real — but no driver spawns Wayland clients
with hostile titles/app_ids; hostile strings enter only via DB fields
(folder/title columns). Similarly hover: the headless stack has **no pointer
input at all**, so every hover code path (enter/leave handlers, fade
cooldown timers, hover ring — the three most recent feature commits,
f87ec19/49d377b/e9405f4) is structurally unreachable in every run. Zero
Antithesis exploration will ever touch it. Fine to exclude (S5), but it
should be a recorded exclusion, not an accident of the topology.

**(c) Driver-sketch gaps for ⚠️custom-fault properties.** The topology's test
template sketch (acknowledged non-final) has no driver for: second daemon
(tile-cache-never-torn, tile-watch-output-schema-valid), .so mutator
(so-replacement-reload-race — only a bind-mount note), binary symlink flip
(producer-binary-swap), icon-cache wipe (icon-negative-cache-pins-missing),
or output hotplug (module-teardown's unplug trigger + output-readd — both
gated on the still-open sway-outer decision). Two P2 properties and one P0
trigger currently depend on an undecided environment choice.

**(d) Oracle parity is assumed, not asserted.** `eventually_converged.sh` and
producer-kill-tile-reconverges compare rendered tile content against "markup
rendered from the current cache payload" — i.e. `pwetty render`. Nothing
establishes that the CLI render path and the in-plugin render path agree; a
divergence makes the convergence oracle wrong in both directions (false
alarms or masked staleness). One cheap harness-validation assertion closes it.

## 5. Assertion-type balance

Tallied across the catalog: Always-dominant (appropriate for a render sink),
~9 liveness properties (producer-kill, stream-recovery, watcher-key,
output-readd, both cold-starts, prompt-pulse, publish-visible, respawn
companion) — liveness is *not* missing; Sometimes used systematically as
vacuity/coverage guards (good discipline, consistently applied);
Reachable/Unreachable/AlwaysOrUnreachable all present and idiomatically
placed (engine-init arms, contentstore-mutex tripwires, icon-src gate).
Category 4 (resource/heat) has no Reachable hints but its Sometimes
companions serve the same exploration-guidance role. No structural
type imbalance found. The one gap: no Unreachable tripwires on the
draw-path unwrap sites (Finding F3) despite the identical pattern already
existing for the mutex-poisoning sites.

## 6. Over-investment check

- **ContentStore micro-timing: 4 properties** (content-snapshot-torn-read,
  publish-visible-within-poll-bound, animating-gate-matches-stored-content,
  contentstore-mutex-never-poisoned) on a two-instruction/one-frame window,
  one of which (content-snapshot) admits it needs a synthetic tile because
  no shipped preset combines uniforms+stream, and self-ranks low. Against
  zero properties on daemon reconcile internals (Finding F1), this is the
  clearest plugin-inward skew. Partially justified — scheduling is
  Antithesis's home turf and the shared `generation` counter amortizes cost —
  but the portfolio would trade content-snapshot-torn-read's rank for one
  Go-side attribution property without losing much.
- **Cluster C (9 properties)** contains acknowledged dominance overlaps
  (poll-mode-cold-start is the degenerate variant of cold-start-stream;
  watcher-key's Always is embedded in output-readd). property-relationships.md
  documents these as triage-precision layers — acceptable, not waste.
- Cluster D's 7 properties are explicitly "triage precision, not extra
  coverage" around the terminal observable — deliberate and fine.

## Findings

**F1 — Backend daemon internals have zero properties despite being the
declared bug-history hotspot.**
Property: catalog-wide (Category 2 / Cluster C composition).
Concern: all ~6 Go-side properties sit on the tiles.json seam; reconcile,
session→desktop attribution, GC/reaper, and decay computation have none.
Two of the backend's six historical fixes are attribution bugs
(nondeterministic window pick, cwd resolution), and wrong-desktop attribution
is functionally F9 (prompt lost to the user) while rendering as perfect
normality on every existing property's oracle — schema-valid, prompt-retained,
converged, all green, wrong desktop.
Evidence: sut-analysis §5 backend hotspot; tile.go:146 `PayloadFor` takes
pre-joined `sessionsOnWs` (the join happens upstream in reconcile, unasserted);
tile.go:188-190 window-pick stability comment marks the exact historical bug.
Suggested action: add 1-2 Go-side properties, e.g.
`Always("a session appears in exactly its owning desktop's payload")` at
reconcile output, and `Always("app-desktop window pick is stable across cache
rebuilds when the window set is unchanged")`; fund by demoting
content-snapshot-torn-read.

**F2 — The harness ingress bypasses `internal/hook`/`internal/clauded` and
the GC reaper can delete workload-fabricated sessions, silently gutting
Cluster C/D coverage.**
Property: catalog-wide (validity of ~16 properties' coverage), plus
deployment-topology "Backend data plumbing".
Concern: coverage that exists on paper may not materialize at run time; green
runs with never-firing Sometimes guards.
Evidence: agentic-db gc.go:27-46,80 (dead predicate: /proc/<pid> liveness OR
niri window gone OR first-party session file absent); hook_test.go:57-67
(event→state machine only reachable via the hook ingress); topology says
workload "wraps the DB writes".
Suggested action: (a) topology must specify how injected sessions survive the
reaper (live sleeper PIDs as terminal_pid, real niri windows, fixture
`~/.claude/sessions/<pid>.json` files); (b) add a harness-health
`Sometimes("an injected session survived one full GC cycle")`; (c) record
hook/clauded parsing as an explicit scope-out or add a hook-CLI ingress
variant to the session-churn driver.

**F3 — Attack surface #2 (draw-path panics) has no dedicated property.**
Property: catalog-wide gap (Category 1 composition).
Concern: the #2-ranked S1 surface is covered only as an escalation endpoint
of shader-recompile-gl-object-leak and by the generic finally_no_crash.
Evidence: sut-analysis §7 items 1-2 — `create_texture().unwrap()` /
`create_framebuffer().unwrap()` per-frame (shader.rs:277,300), epoxy
`.expect` on init AND draw path (gl.rs:22,25), no glGetError anywhere; §12
ranks this #2 of 9.
Suggested action: cheap tripwires in the contentstore-mutex style —
`Unreachable("draw-path GL object creation failed")` at the two unwrap-adjacent
sites — plus either a GL-pressure workload lever or an explicit note that
draw-path GL failure is uninjectable under llvmpipe (the catalog already asks
this in two open questions; the *property* is missing, not the question).

**F4 — S8/bpe (inter-renderer GL state determinism) has no regression guard.**
Property: catalog-wide (Category 1/4 composition).
Concern: §5's highest-severity density cluster is "GL context
lifecycle/state"; the catalog saturates lifecycle and skips state. S8 exists
in the claims table specifically as the bpe regression memorial.
Evidence: sut-analysis §4 S8 ("regression of bead bpe", shader.rs:229-233
explicit disable(BLEND)); §5 density map.
Suggested action: one SUT-side
`Always("GL blend/state neutral at femtovg↔ShaderPass handoff")` at the
existing S8 site, or a recorded exclusion ("state bugs are visually cosmetic
under llvmpipe and un-oracled without vision").

**F5 — L3 / active-accent tracking untested and un-drivable: no property, no
workspace-churn driver.**
Property: catalog-wide gap + deployment-topology test-template sketch.
Concern: the `active` flag is a truthfulness feature (wrong desktop
highlighted = wrong data, S2-class); WorkspaceActivated handling is the
daemon's hottest event path (immediate cache write bypassing the tick) and
nothing exercises it; the missing driver also strands the injection
properties' niri-side vectors (F2's C0/U+FFFC "can it survive niri plumbing?"
open questions are answerable in this harness but nothing will answer them).
Evidence: sut-analysis §4 L3; tile.json template `{% if active %}<active/>`;
topology driver list has session/kill/reload/garbage/shader drivers, nothing
niri-side.
Suggested action: add `parallel_driver_workspace_churn.sh` (niri IPC
focus-workspace + open/close windows with hostile titles) and a property
`Always("exactly the focused workspace's payload has active=true after
convergence")`; this one driver serves L3, the attribution property (F1), and
the injection open questions simultaneously.

**F6 — Poll-mode hung-command staleness leg uncovered.**
Property: poll-mode-cold-start-converges, stream-ingest-memory-bounded
(the two poll-mode properties).
Concern: §7 names three poll-mode failure shapes; the catalog covers
cold-start ordering and memory, but not "Command::output() with no timeout —
hung command freezes that tile forever, silently" — permanent S2 staleness in
the exact optional mode the lens flags as commonly missed.
Evidence: sut-analysis §7 "Poll:" paragraph.
Suggested action: extend the poll-mode tile variant with a hang lever
(`exec: sleep infinity`) and a bounded-staleness or
`AlwaysOrUnreachable("poll exec completed within timeout budget")` property —
or record "poll mode is out-of-contract for live data" once the catalog's
existing `interval: 0` needs-human-input question is answered (the same
answer settles both).

**F7 — INK_CACHE unbounded growth demoted to an open question, never a
property.**
Property: stream-ingest-memory-bounded (open question), catalog Category 4.
Concern: §2 flags INK_CACHE as one of two pathological caches (never evicted,
keyed by full run-markup text, grows with changing titles/ago labels,
survives reloads). Ago labels change every minute and the session-churn
driver churns titles, so growth is workload-organic — yet it lives only as
"is slow-burn growth observable... or documentation-only?".
Evidence: sut-analysis §2 caches; catalog stream-ingest open questions.
Suggested action: cheapest is a SUT-side cache-entry counter with
`Sometimes("INK_CACHE exceeded N entries")` as an observability hint; or
explicitly fold into the RSS-ceiling property's rationale and close the
question as documentation-only.

**F8 — L5 idle-decay progression (backend arithmetic) untested.**
Property: catalog-wide gap adjacent to static-idle-redraw-budget /
idle-level-gate-clamp-divergence.
Concern: the catalog tests out-of-range levels (injected) and budget-given-
level, but not that decay *advances* and terminates at static — the
never-goes-static failure is the S11 heat bug via backend arithmetic, and
no clock fault is needed (backdated `last_talk_ts` DB writes reach any level
instantly).
Evidence: sut-analysis §4 L5 ("entirely backend-driven"); agentic-db has
decay_timeline_test.go (unit-level only, so risk is moderate not high).
Suggested action: P3 property
`Sometimes("an idle tile reached level 6 and stopped queueing redraws")`
using backdated timestamps; cheap because static-idle-redraw-budget's draw
counters already exist.

**F9 — The daemon-death-permanent-staleness design question was dropped
between SUT analysis and catalog.**
Property: catalog-wide Open Questions section.
Concern: sut-analysis Open Questions carries "is daemon death ⇒ permanent
silent staleness accepted by design?" (needs human judgment); the catalog's
collected ~9 needs-human-input items omit it. The harness supervisor
auto-restarts the daemon, so no run will ever exhibit the production reality
(no restart unit ships — confirmed in the catalog itself), and no property
covers a staleness *indicator*. The question must survive somewhere or it
dies here.
Evidence: sut-analysis §7 backend row + Open Questions; catalog "Open
Questions (catalog-wide)" list.
Suggested action: re-add to the catalog-wide open questions; optionally note
that the supervisor deliberately masks this production behavior.

**F10 — The most-filed-bugs area (Pango ink/metrics, marquee) has zero
properties and no recorded exclusion.**
Property: catalog-wide composition.
Concern: §5 density map "most filed bugs: 4up, e0r, 1ml + re-fixes"; L6 and
F11 (focus_bubble radius — falsifies a README claim; the corner_radius
feature is one of the three most recent commits) are untested. Deprioritizing
is *correct* under the §9 severity model (S5, vision-hard oracles) — the
defect is that the catalog never says it made this call, so a reader can't
distinguish deliberate exclusion from a discovery hole.
Evidence: sut-analysis §5 density map, §4 L6, §11 F11; git log (e9405f4).
Suggested action: one "Deliberate exclusions" paragraph in the catalog
naming ink/marquee/hover/F11 with the severity-model rationale; no new
properties needed.

**F11 — Oracle parity between `pwetty render` and the plugin render path is
assumed by convergence checks, never asserted.**
Property: producer-kill-tile-reconverges, topology eventually_converged.sh.
Concern: the convergence oracle silently depends on CLI/plugin render
equivalence; drift produces false alarms or masked staleness across every
convergence-based property.
Evidence: topology test-template sketch ("every tile's rendered content
matches `pwetty render` of the current tiles.json payload").
Suggested action: one harness-validation assertion,
`Always("pwetty render output equals plugin-published markup for the same
payload")` checked at quiet checkpoints, or downgrade the oracle to
plugin-side markup only.

**F12 — Unimplemented ABI surface: no Reachable probe.**
Property: catalog-wide (Category 1/6 composition).
Concern: §5 flags update/refresh/doaction as "suspiciously quiet"; §7 item 4
notes waybar-cffi's `.expect()` abort on non-UTF-8 action names. Nothing
even establishes whether waybar 0.15.0 ever invokes these entrypoints in
this config.
Evidence: sut-analysis §1 FFI surface, §5, §7 item 4.
Suggested action: low-priority `Reachable("wbcffi_update invoked")` (and
siblings) as documentation-grade probes; likely fires never → converts the
"quiet" flag into evidence.

## Passes

- **Attack surfaces #1, #3, #5, #6, #9**: property density proportional to
  rank; trigger diversity correct (reload + unplug + exit for teardown);
  known-failing properties honestly marked; crash-masking sequencing (p9c
  first) explicitly handled by run phasing.
- **Cross-cutting compounds discovery usually misses are present**:
  injection×animation (`| safe` → tick source), caching×reload (icon negative
  cache; leaked-thread caches), build×reload (.so replacement race),
  cold-start ordering in both stream and poll variants, version skew in three
  distinct seams (CFFI transport, daemon/tile-watch binaries, cross-repo
  constants).
- **Assertion-type portfolio**: liveness well-represented (~9), Sometimes
  systematically used as vacuity guards, Reachable/Unreachable idiomatic;
  AlwaysOrUnreachable used where the trigger is workload-gated. No missing
  type class.
- **Severity model adherence**: P0s sit exactly on S1/S2/S3 (crash, silent
  staleness, attention); S4 deliberately untested (honest failure) —
  consistent with §9; heat (S5) covered where it has an escalation path to S1.
- **Fault-requirement honesty**: ⚠️ legend accurate; no property requires
  unavailable tenant faults; the clock-seam fallback for the f32 legs is the
  right planning default.
- **Topology↔catalog agreement on oracles**: SUT-side draw counters as
  primary, screenshots secondary — consistently propagated into the animation
  properties after the frame-clock investigation.
- **Component coverage of tile-watch process lifecycle** (spawn, SIGPIPE
  death, orphaning, respawn, mixed versions) is complete even though
  tile-watch internals are thin — appropriate, since it's a 75ms cat-loop.
- **Config surface**: both the transport seam (v1 retype) and the resolve
  seam covered, including the production-shape note that the tile_file leg
  needs a workload variant.

## Uncertainties

- **GC reaper grace periods**: gc.go comments imply some tolerance ("keeps
  its session file... for many minutes"); I did not trace the full dead
  predicate timing, so F2's severity ranges from "sessions die in seconds"
  to "workload has minutes of slack". A 10-minute probe run or a deeper
  agentic-db read settles it.
- **`pwetty render` pipeline identity**: I did not verify whether the CLI
  shares the exact minijinja/markup code path with the plugin (F11 assumes
  it may drift; if it's literally the same functions, the finding downgrades
  to documentation).
- **Whether waybar 0.15.0 ever calls update/refresh/doaction** for CFFI
  modules in this config — not traced in waybar source; F12's probe is how
  to find out.
- **Injectability of draw-path GL failure under llvmpipe** (F3): the catalog
  itself carries this open question; if genuinely uninjectable, F3 reduces
  to the two Unreachable tripwires plus a documented limitation.
- **S8 assertion placement cost**: I did not read the femtovg↔ShaderPass
  handoff sites closely enough to confirm a state assertion is cheap; if it
  requires GL queries per frame it may not be worth it (F4's alternate action
  — recorded exclusion — stands either way).
- **Whether the demo tile's 60fps is its configured target or a cap
  violation**: affects whether static-idle-redraw-budget's fps-cap assertion
  would false-positive on the config-collapse outcome; config-resolve owns
  that outcome regardless.
- **Multi-bar (two-output) production wiring**: harness uses one bar; the
  mixed flagless/pinned wiring risk is represented via watcher-key's
  misconfiguration variant, but true two-bar interaction (20 modules, one
  process) is unexplored — likely negligible, could not be determined from
  the documents.
