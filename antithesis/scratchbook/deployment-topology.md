---
sut_path: /home/chussenot/Documents/waybar-pwetty-box
commit: f87ec19c3e40a62425b2145891c2b45d62a36363
updated: 2026-07-22
external_references:
  - path: /home/chussenot/agentic-db
    why: claude-status backend (daemon + tile-watch) must be built into the SUT image; workload drives sessions through its SQLite DB
  - path: https://github.com/Alexays/Waybar
    why: host binary in the image; version pin (0.15.x) matters for CFFI lifecycle behavior the properties target
---

# Deployment Topology — waybar-pwetty-box on Antithesis

## Design Constraint (why this topology is one container)

Antithesis injects faults at the **container** level, and separate containers
can only interact over the network. This SUT has **no network protocols
anywhere**: `tile-watch` is spawned by the plugin *inside waybar's process*
(`sh -c` from src/content.rs), talks to it over an inherited pipe, and reads
`tiles.json` — a file shared with the daemon on the same filesystem. The
daemon subscribes to niri over a Unix socket. Splitting any of these into a
second container is impossible without changing the SUT.

Consequently the minimal useful topology is a **single SUT container** that
hosts the whole production stack, with the test template inside it (test
commands must run where the processes and files are). Network faults buy
nothing here; the useful Antithesis levers are **process kills via test
commands/custom faults, thread pausing (needs instrumentation), CPU
modulation, clock jitter, and filesystem-level mischief from the workload**.
An in-container supervisor substitutes for node-termination faults (which are
disabled by default in most tenants) by making "kill waybar / kill daemon"
ordinary process faults with supervised restart — no catalog property needs
tenant node-termination as a result. **Two evaluation caveats:** (1) the
supervisor must NOT kill/adopt the plugin's process group, or the orphan
properties' window never opens; (2) supervised restart is harness fiction
relative to production, where **no daemon restart mechanism exists** — the
most probable production failure (daemon dies once, tiles freeze forever) is
erased by the supervisor. Mitigation: the withheld-restart variant and
kill-niri driver below.

This mirrors production exactly: the real deployment is also one host, one
session, same-host IPC.

## Feasibility Proof

`test/shot.sh` already runs the full real stack headless with **no GPU
device**: cage (headless backend, pixman, `LIBSEAT_BACKEND=noop`) → niri
(winit backend, llvmpipe via `LIBGL_ALWAYS_SOFTWARE=1`) → real waybar
dlopening the real `libpwetty_box.so` → grim screenshot. The Antithesis image
is this script turned into a container entrypoint, minus its two defects
(stale hardcoded module_path from another machine; asserts nothing).

## Container: `pwetty-sut` (the only container)

| | |
|---|---|
| **Role** | SUT + client (runs the test template) |
| **Image source** | New layered Dockerfile (none exists in either repo). Build stage 1: Rust toolchain per `rust-toolchain.toml` → `cargo build --release` → `libpwetty_box.so` (+ `pwetty` CLI for render probes). Build stage 2: Go toolchain → `claude-status` from ~/agentic-db (vendored/copied into the build context or a pinned commit). Runtime stage: Debian/Ubuntu with waybar (pin 0.15.x — the version whose CFFI lifecycle the properties target), cage, niri, grim, Mesa (llvmpipe + surfaceless ICD), fontconfig + pinned fonts (JetBrainsMono Nerd Font, Terminus — ink-box constants are tuned to them; unpinned fonts shift geometry under any vision-based check), sqlite3 CLI, jq, a JSON-Schema validator CLI (for the pipe tee), and a minimal supervisor (s6-overlay or a shell supervisor loop). |
| **What it runs** | Supervisor → (1) cage headless → (2) niri (winit) → (3) waybar with the harness config → (4) `claude-status` daemon. Waybar's plugin spawns the `tile-watch` children itself, as in production. After first successful render (waybar up + tiles.json populated + one grim screenshot succeeds), the entrypoint emits `setup_complete` and sleeps. |
| **Network** | None (loopback only). No ports exposed; no inter-container links. |
| **Replicas** | 1 |

### Waybar harness config (part of the image)

- **3 × `cffi/pwetty` tile instances** (not production's 10 — enough for
  multi-instance concurrency, dual-session and cross-tile interactions, small
  enough to keep the state space tight): tiles #1 and #2 as `tile: claude`,
  `stream: true`, `exec: claude-status tile-watch N --output winit`
  (**`--output winit` is mandatory** — evaluation caught that the flagless
  shipped wiring defaults to `HDMI-A-1`, which never matches niri-winit's
  hardcoded `winit` output: every tile would show the placeholder forever and
  `setup_complete` would not catch it); tile #3 reserved for per-run variants
  (poll-mode `interval` configs; `tile_file` + `background_shader`;
  shader-uniform tile for the torn-read property).
- The waybar bar config must be **output-unpinned** (no `"output"` key) for
  the output-readd property — waybar creates bars only for config-matching
  outputs, so a pinned name on a renamed re-add yields no bar at all.
- Environment: `LP_NUM_THREADS=0` (or count only plugin-named threads) —
  llvmpipe spawns per-context rasterizer pools that would otherwise make the
  "thread count stays flat" assertions noisy.
- **1 canary module** (waybar built-in `clock` with 1s interval) — the
  liveness witness for `neighbor-modules-stay-live`.
- `module_path` points at the image's baked `.so` path — with a bind-mounted
  staging copy for `so-replacement-reload-race` (the mutator swaps the staged
  file).

### Backend data plumbing (how the workload creates sessions)

**REVISED after evaluation — DB writes alone are NOT a viable fixture.** The
daemon's GC reaps any session whose `window_id` is absent from the live niri
model (~1-2s) or whose `terminal_pid` has no `/proc` entry (gc.go
deadPredicate), and `BuildAll` drops sessions without a valid live window —
they never render at all. Also, **`title` is not a DB column** (titles come
from niri windows). The viable fixture shape:

1. **One real Wayland window per target niri workspace** (any trivial
   client, e.g. `foot`/`alacritty` or a stub wl client), placed via
   `niri msg action` — this also makes the target workspaces *exist* (niri
   workspaces are dynamic; unplaced desktops mean tile-watch polls a missing
   key forever).
2. Session rows carry that **real `window_id`**, `terminal_pid` NULL (skips
   the /proc check), and the first-party sessions dir absent — or
   `-sessions-dir` pointed at a workload-controlled dir when a run wants to
   exercise the overlay.
3. Hostile *title* strings are set via the fixture windows themselves
   (terminal title escape sequences / client set_title), not the DB; hostile
   `folder` strings via the DB `cwd` column (U+FFFC via `mkdir`).
4. Preferred higher-fidelity lever: the real ingress `claude-status hook`
   (JSON on stdin) — creates rows exactly as production does and opens the
   DB with WAL + busy_timeout, sidestepping raw-sqlite locking infidelity.
   (Its SessionStart path resolves windows by /proc ancestry — ergonomics
   from a workload shell need one probe; DB-with-real-window_id is the
   fallback.)

The daemon gets its niri event stream from the nested niri. Direct
`tiles.json` writes remain the lever for payload-shape properties
(`idle_level: 7`, unknown states, 3+ sessions) **but race the daemon's
dedupe** (it compares against its own last marshal, not file content, so an
injected payload is silently clobbered by the daemon's next differing
write): **injection phases must stop or pause the daemon first**, or point
the variant tile's tile-watch at a workload-owned cache path.

### Test template: `/opt/antithesis/test/v1/pwetty/`

Sketch (final composition belongs to `antithesis-workload`):

- `parallel_driver_session_churn.sh` — random session create/state-flip/close
  via the DB, weighted toward prompt transitions; hostile-string variants
  (U+FFFC, C0, newlines, huge titles).
- `parallel_driver_kill_producer.sh` — kill a random `tile-watch` (random
  signal, random timing).
- `parallel_driver_kill_daemon.sh` — kill/restart the daemon (supervisor
  restarts it); occasionally delete/corrupt `tiles.json` first.
- `parallel_driver_reload_waybar.sh` — SIGUSR2 waybar (the q9y trigger);
  counts producer chains/threads/timers afterward.
- `parallel_driver_garbage_stream.sh` — swaps tile #3's exec to a wrapper
  producer that injects framing violations (invalid UTF-8, torn lines,
  newline-less floods) then restores.
- `parallel_driver_shader_churn.sh` — cycles tile #3's shader file
  valid→broken→missing→valid. (Its per-frame stderr spam is itself a
  disk-filling mechanism that can induce daemon write failures — a real
  cross-cluster fault composition; cap the log or embrace it knowingly.)
- `parallel_driver_kill_niri.sh` — kill the nested niri (the documented
  trigger of daemon exit). Pairs with the **withheld-restart variant**: the
  supervisor delays daemon/niri restart by a long random window, exposing
  the production no-restart reality (what the user sees during it: frozen
  plausible state — the finding).
- `parallel_driver_workspace_churn.sh` — `niri msg action focus-workspace` /
  window moves via the nested niri: drives the active-accent property and
  the niri-title injection vectors.
- `eventually_converged.sh` — quiet-period check (`ANTITHESIS_STOP_FAULTS`):
  every tile's rendered content matches `pwetty render` of the current
  tiles.json payload (**a plumbing-divergence check, not rendering
  correctness — the CLI links the same rlib, so render bugs cancel out**);
  producer-chain/thread/timer counts **exactly equal** to module count
  (two-sided — a dead reader chain is a violation, not just an excess);
  every live tile-watch key resolves in the cache (the shared stranding
  oracle); canary advanced. All wall-clock-bounded convergence assertions
  from the catalog evaluate here, inside the quiet window.
- `finally_no_crash.sh` — waybar PID unchanged since setup (or cleanly
  supervised-restarted a known number of times); no core files.
- `helper_session.sh` (real-window fixtures per the revised plumbing),
  `helper_screenshot.sh`, `helper_counts.sh`, `helper_validate_line.sh`
  (the schema-validating pipe tee — note: the tee adds a process per chain
  and moves the SIGPIPE reader; enable it only in contract-focused run
  variants and adjust the count constants there).
- **FIFO-wedge variant runs terminal-phase only**: the `<icon src>` FIFO
  lever permanently wedges the main thread (by design of the finding) and
  kills every other property's signal for the rest of the run — schedule it
  as its own short variant or a `finally_`-adjacent phase.
- Long-title lever (>PIPE_BUF lines) gets first-class driver weight — the
  torn-line window (SIGKILL between short-write iterations) is one of the
  few genuinely exploration-dependent preconditions.

### SDK selection

- **Rust SDK** (`antithesis_sdk` crate) into `libpwetty_box.so` — carries all
  plugin-side assertions (Categories 1-5 SUT-side items) and enables thread
  pausing via coverage instrumentation.
- **Go SDK** into `claude-status` — carries the backend assertions
  (`daemon-restart-no-placeholder-clobber`, `cache-error-demotes-live-tile`,
  `tile-cache-never-torn`, `prompt-priority-survives-session-cap`,
  `unknown-session-state-renders-blank` producer end).
- Shell workload uses the SDK-provided `ANTITHESIS_*` conventions from test
  commands; `ANTITHESIS_STOP_FAULTS` for mid-run convergence checks
  (liveness properties need quiet periods; `eventually_` covers the terminal
  check).
- **Test seams to build alongside the SDKs** (evaluation-mandated; no
  Antithesis fault type covers these):
  - *Markup export seam*: env-gated dump of each tile's last-published
    markup (file or socket) — "rendered content matches payload" oracles are
    otherwise unobservable (pixels can't match animated content).
  - *GL failure seam*: env var forcing `OffscreenGl::new` / `make_current`
    to fail — the only trigger for the GL-degradation properties (no
    fs/EIO/memory-pressure fault injection exists in Antithesis).
  - *Clock seam*: env-var **additive** offset applied in the time
    computation (lib.rs:244) — NOT an `Instant` start-offset, which
    underflows/panics on a fresh container; pins the 36h/12d/97d f32
    regimes deterministically.
  - *Reader heartbeat*: per-module `Sometimes("stream reader iterated")` —
    converts the silent reader-thread-death class into a detectable one.

### Phasing recommendation

- **Probe run (hours, before everything)**: validates the harness itself —
  draw-counter sanity (frame clock alive), grim cost, RSS baseline, and the
  **thread-pause calibration gate**: `content-snapshot-torn-read`'s
  `Sometimes` (a set() landing between a draw's two lock acquisitions) is
  the designed calibration signal for whether coverage-instrumentation
  thread pausing works inside a dlopen'd Rust cdylib in an uninstrumented
  C++ host. If it never fires in a dedicated churn run, re-plan the
  concurrency cluster (e.g. widen windows with a test-flag sleep seam)
  before spending run-2 budget. Also record branches/sec and
  unique-behavior discovery rate — the SUT is polling-heavy
  (13ms/75ms/150ms/30fps busy machinery) and exploration economics were
  flagged as a risk; a run that spends its budget re-exploring poll
  iterations instead of interleavings needs quieter payloads or lower fps.
- **Run 1 (no SUT instrumentation, f87ec19)**: workload-only assertions —
  host survival, process/thread counts (two-sided), RSS ceilings,
  convergence checks, canary. Should find the p9c abort on the first
  reload/teardown fault. Cheap, validates the harness.
- **Run 2 (fix branch + SDKs + seams)**: merge
  `worktree-fix-gl-teardown-crash`, add Rust/Go SDK assertions per the
  catalog, enable thread pausing. Weight drivers toward the
  exploration-dependent properties (torn pipe writes, .so-replacement
  races, orphan interleavings, the concurrency cluster) — the
  deterministic-trigger properties keep their assertions as ride-along
  tripwires but need no dedicated search budget.
- **Run-variant × assertion matrix**: every `Sometimes`/`Reachable` must
  name the driver or environment variant that makes it satisfiable and be
  env-gated to it (generalizing torn-ndjson's flag) — otherwise every run
  without that driver reports structurally-red coverage assertions. The
  GL-degraded environment variant in particular must gate OFF
  `Unreachable("engine absent while content markup is available")`
  (contradicts the variant by design) and gate ON the engine-less
  reachability anchors.

## Fault Availability Requirements

| Need | How satisfied |
|---|---|
| Process kill/restart (waybar, daemon, tile-watch) | Test commands + in-container supervisor — no tenant node-termination needed |
| SIGUSR2 reload | Test command (custom fault) |
| Thread pausing | Rust SDK coverage instrumentation in the plugin |
| CPU modulation | Default fault set |
| Clock jitter / virtual time (f32-quantization legs) | ⚠️ opt-in ("talk to your forward-deployed engineer", docs); documented as forward/backward jumps that stack (worked example: 30s), with **no statement on whether CLOCK_MONOTONIC moves** and no documented hours-scale magnitude; idle fast-forward can't accumulate in this never-idle SUT. **Plan the test seam** (env-var start-offset into the Engine clock) as the default; treat tenant-confirmed monotonic jumps as a bonus. See `properties/prompt-pulse-visibly-advances.md` Investigation Log |
| Filesystem mischief (delete/corrupt cache, shader churn, .so swap) | Workload scripts |
| Network faults | Not useful (single container) — deliberately unused |

## Assumptions

- cage/niri/waybar/llvmpipe run under Antithesis's deterministic hypervisor
  the way they run under the host harness (shot.sh proves the stack headless,
  not determinism-friendliness; GTK frame-clock delivery semantics are now
  settled — see Open Questions below).
- The agentic-db repo is buildable at a **pinned commit** inside the image
  (Go modules vendored or fetched at build time). Pin it and record the SHA
  next to the image — the scratchbook's Go line references float otherwise
  (evaluation already caught one drifted line ref); prefer function anchors
  in assertion-placement notes.
- Waybar 0.15.x package (or source build) is available for the chosen base
  image.
- **Two-compositor validity caveat** (sway-outer variant): in production,
  waybar and the daemon observe the *same* compositor (niri); in the
  sway-outer harness, waybar's outputs are sway's while the daemon's
  desktops come from nested niri's single fixed output. Output-identity
  findings therefore validate waybar-on-sway behavior, not the production
  identity flow — don't over-claim from a green hotplug run.

## Open Questions

- **Settled 2026-07-22** — virtual outputs at runtime under cage/niri (winit):
  **no**, and not fixable by configuration. niri's winit backend exposes
  exactly one hardcoded `winit` output; niri has no runtime output
  create/destroy/rename on any backend (headless backend is test-only, zero
  outputs, no IPC; virtual-output IPC unmerged as of Feb 2026); cage-level
  outputs don't map to niri-winit outputs. **Achievable alternative** that
  keeps waybar + plugin + daemon real: outer compositor = **sway
  `WLR_BACKENDS=headless`** (same pixman/noop-seat knobs) hosting waybar, with
  niri (winit) nested inside sway solely as the daemon's event source; hotplug
  via `swaymsg create_output` / `swaymsg output <name> unplug`
  (`wlr_output_destroy`; sway ≥1.8). Re-added headless outputs always get a
  new name (`HEADLESS-N` counter never resets) — renamed-re-add for free;
  same-name re-add needs `output disable/enable` (wl_output-global behavior
  unverified — one local check) or a sway restart. Decision for environment
  design: adopt the sway-outer stack if `module-teardown-never-aborts-host`'s
  confirmed p9c trigger and `output-readd-tile-recovers` should run as true
  hotplug; otherwise fall back to SIGUSR2 reload (source-confirmed same
  dispose leg) + exit + wrong-`--output` misconfiguration variant. Note the
  backend rename-*stranding* trigger stays impossible either way (the daemon's
  keys come from niri's outputs, which are fixed) —
  `watcher-key-survives-output-rename` is scoped to the misconfiguration
  variant. Evidence: Investigation Logs in
  `properties/output-readd-tile-recovers.md`,
  `properties/module-teardown-never-aborts-host.md`,
  `properties/watcher-key-survives-output-rename.md`.
- **Settled 2026-07-22** — GTK frame clock under this stack: delivery is
  frame-callback driven and damage-gated, not free-running. For the mapped,
  always-visible bar the loop self-sustains while anything damages it (an
  animating tile does; the 1s clock canary guarantees ≥1Hz service
  independent of the plugin — keep it in the same bar window). Hidden/
  occluded surfaces stall the GTK3 clock entirely (known GTK3 class), but
  that shape doesn't arise here; wlroots headless outputs pace repaints from
  a 60Hz software timer, so no display is needed. Consequence: animation
  properties use SUT-side draw counters / phase assertions as the primary
  oracle; screenshot diffing is a secondary, visibility-dependent check. The
  10-second draw-counter probe remains a cheap in-run sanity check, no longer
  a design gate. Evidence: `properties/prompt-pulse-visibly-advances.md` and
  `properties/neighbor-modules-stay-live.md` Investigation Logs.
- Screenshot cadence: is per-second grim capture cheap enough under llvmpipe
  for the canary/pulse checks? `(partial: settled that SUT-side counters are
  the primary oracle for animation properties regardless — see the frame-clock
  entry above — so grim cost only sizes the secondary canary cadence;
  cost itself still unmeasured, needs a probe run)`
- Should run 1 use 2 tiles instead of 3 to shrink the state space further, or
  is the per-run-variant third tile worth it from the start?
