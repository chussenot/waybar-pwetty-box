# module-teardown-never-aborts-host

Focus: lifecycle transitions — module teardown must never abort the waybar host.

All suggested assertions are **net-new**; no Antithesis instrumentation exists
anywhere in this codebase (see `existing-assertions.md`).

## Claim

Destroying a pwetty module instance — output removal, SIGUSR2 config reload,
or waybar exit — never terminates the waybar process. At f87ec19 this is
**violated and confirmed** (bead p9c, SIGABRT, gdb on the actual core dump);
the fix exists only on unmerged branch `worktree-fix-gl-teardown-crash`
(commit 30100f9). This is the flagship regression property for the SUT's
hottest zone.

## Code paths (all re-verified at f87ec19)

- `src/lib.rs:41-52` — `Engine { gl: OffscreenGl, renderer: Renderer, ... }`.
  Rust drops fields in declaration order: `gl` first.
- `src/offscreen.rs:75-80` — `OffscreenGl::drop` unbinds the current context
  (`make_current(display, None, None, None)`) then destroys it. After this,
  *nothing* is current on the thread.
- femtovg's `OpenGl::drop` (external crate; grounded in the p9c gdb backtrace,
  not a femtovg source read) then issues `glDeleteVertexArrays` via
  glow/epoxy → epoxy `__assert_fail "Couldn't find current GLX or EGL
  context"` → SIGABRT of the whole waybar process.
- `src/lib.rs:255-260` — the `needs_gl` gate: for a shader-less content tile
  (the claude preset default), `make_current` is *never* called during draws,
  so at dispose time no context has ever been current on the main thread —
  the crash needs no unlucky timing at all for those tiles.
- Drop trigger: the draw closure (`src/lib.rs:231`) holds an `Rc<Shared>`;
  GTK's `g_object_run_dispose → g_signal_handlers_destroy` frees the closure
  and drops the last `Rc` → `Engine` drop. Confirmed by the core-dump
  backtrace (bead p9c). `wbcffi_deinit` itself only drops the `Box<PwettyBox>`
  (waybar-cffi 0.1.1 `src/lib.rs:119-124`), decrementing the same `Rc`.
- `grep "impl Drop" src/` → only `OffscreenGl`. `ShaderPass` has no Drop (its
  `delete_*` calls at `src/shader.rs:274-275,321-322` are in render/compile
  paths), so femtovg's canvas is the **only** GL-calling destructor — the fix
  covering it covers the whole abort class at this commit.
- The fix (30100f9): `ManuallyDrop<Renderer>` + `impl Drop for Engine` that
  makes the offscreen context current before dropping the canvas, and leaks
  the canvas if `make_current` fails. Regression test
  `engine_drop_without_current_context` reproduces the abort when the guard is
  neutered.

## Trigger inventory

1. **Output removal** (lid close, dock unplug) — CONFIRMED trigger (p9c core
   dump, 2026-07-21). GTK destroys the bar window for the removed output.
2. **SIGUSR2 config reload** (bead q9y) — mechanism **confirmed from waybar
   0.15.0 source** (2026-07-22, see Investigation Log): SIGUSR2's default
   action is RELOAD, which quits the GTK app and ends the `Client::main`
   iteration with `bars.clear()` (src/client.cpp:314) — every `Bar` (and its
   `Gtk::Window` member, hence the module widget tree) is destroyed on
   *every* reload, config changed or not. Reload therefore runs the identical
   dispose leg as output removal, with no config mutation required to arm it.
   Still unvalidated is only the *attribution* of the Jul-19 crash file to
   this trigger (evidence overwritten by the Jul-21 crash, single apport
   path) — if Antithesis crashes on SIGUSR2 with the same stack, q9y is p9c;
   if it crashes differently, q9y was a second bug.
3. **Waybar exit** — GTK tree destruction on shutdown runs the same dispose
   path. A crash-on-exit is lower severity (process was dying anyway) but
   still corrupts exit status and produces core-dump noise.

## Failure scenario

Any of the triggers above at f87ec19 → SIGABRT → every bar on every output
dies (severity S1, the worst class in the product's own ranking). With 10
instances per bar, one lid-close kills the entire desktop's status UI.

## Suggested assertions (net-new)

Workload-side (no SUT change needed):

- `Always`: message **"waybar host survives module teardown"** — after each
  teardown trigger (output removal / SIGUSR2 / config-mutating reload), the
  waybar PID either still exists (reload, output removal) or exited without
  SIGABRT/core (exit trigger). Checkable via supervisor exit-status capture +
  absence of new core dumps.
- `Sometimes`: message **"a module teardown was triggered while tiles were
  live"** — coverage guard so the Always cannot pass vacuously on a run where
  no teardown ever happened.

SUT-side (natural placement is the fix branch's `impl Drop for Engine`;
at f87ec19 there is no Drop impl to instrument — land these with the merge):

- `Reachable`: message **"engine teardown entered with no GL context
  current"** — in `Engine::drop`, when `eglGetCurrentContext()` reports
  nothing current. This is the exact p9c precondition; hitting it proves the
  dangerous window is exercised, not just teardown in general.
- `Sometimes`: message **"engine teardown completed without abort"** — last
  line of `Engine::drop`. Doubles as a replay anchor for the teardown phase.
- `Reachable`: message **"engine teardown leaked the canvas because
  make_current failed"** — the fix's fallback arm; proves the leak-on-failure
  path is reachable rather than dead code.

## Fault / harness requirements

- **SIGUSR2 injection** into the waybar process: custom fault (workload can
  `kill -USR2` from inside the container — process-level, no node
  termination).
- **Output removal**: requires the headless compositor to simulate output
  disconnect. **Settled 2026-07-22** (see Investigation Log): not possible in
  the cage → niri (winit) topology — niri exposes exactly one fixed `winit`
  output and has no runtime output create/destroy on any backend. Possible
  by re-basing waybar onto **sway `WLR_BACKENDS=headless`** (same
  pixman/noop-seat knobs; niri stays nested as the daemon's event source):
  `swaymsg output <name> unplug` calls `wlr_output_destroy`, waybar's GDK
  monitor-removed handler destroys the bar → the exact p9c dispose leg;
  `swaymsg create_output` re-adds. Fallback trigger set (SIGUSR2 + exit)
  exercises the same dispose leg either way — and the reload leg is now
  source-confirmed identical (see SIGUSR2 log entry below).
- **Waybar exit**: SIGTERM under an in-container supervisor that captures exit
  status; process-level only.

## Key observations

- Waybar never `dlclose`s the .so (sut-analysis, validated against upstream
  cffi.cpp and the local binary's imports), so teardown risk is purely
  destructor-driven — no use-after-unload dimension.
- Teardown runs entirely on the GTK main thread inside dispose; no draw can be
  concurrently executing, so the property has no draw-vs-drop race leg.
- Post-fix residual to watch: `Engine::drop` leaves the offscreen context
  current briefly, then `OffscreenGl::drop` unbinds it — the main thread ends
  teardown with no context current, which is fine for waybar (GDK never had a
  context current on this thread) but is the seam to re-examine if a second
  GL-using CFFI module is ever co-loaded (sut-analysis F13).

## Open questions

None.

### Investigation Log

#### Can the headless harness (cage/niri winit backend) remove and re-add a virtual output at runtime?

2026-07-22.

- Examined: `test/shot.sh`, `test/niri.kdl` (local, static read); niri
  sources https://raw.githubusercontent.com/YaLTeR/niri/main/src/backend/
  (mod.rs, winit.rs, headless.rs) and main.rs; niri discussions
  https://github.com/niri-wm/niri/discussions/714 and
  https://github.com/niri-wm/niri/discussions/3101; sway sources
  https://raw.githubusercontent.com/swaywm/sway/master/sway/commands/create_output.c
  and .../sway/commands/output/unplug.c, PR
  https://github.com/swaywm/sway/pull/7192, issue
  https://github.com/swaywm/sway/issues/7374; wlroots
  https://raw.githubusercontent.com/swaywm/wlroots/master/backend/headless/output.c.
- Found: niri winit backend = one hardcoded `"winit"` output, no
  on/off/hotplug handling; niri headless backend = test-only (zero outputs,
  internal-only `add_output`, incomplete); virtual-output IPC unmerged as of
  Feb 2026, maintainer "Not at the moment" (Oct 2024). Sway on the wlroots
  headless backend supports runtime `create_output`
  (`wlr_headless_add_output`, outputs `HEADLESS-N`, 1920x1080, 60 Hz frame
  timer) and `output <name> unplug` (`wlr_output_destroy`; headless/
  wayland/x11 only, sway ≥1.8). Recreated headless outputs always get a new
  incremented name (wlroots `HEADLESS-%zd` counter never resets).
- Not found: any niri-side hotplug mechanism; whether sway `output disable/
  enable` removes/re-creates the client-visible wl_output global (relevant
  only for a same-name re-add leg; unplug/create is the verified path).
- Conclusion: resolved — no in the current stack, yes with a sway-headless
  outer compositor hosting waybar (niri remains nested for the daemon).
  With that stack this property's only *core-dump-confirmed* trigger
  (output removal) becomes injectable as an ordinary test command
  (`swaymsg output ... unplug`), and `output-readd-tile-recovers` becomes
  fully exercisable. If the topology stays cage → niri, the trigger set is
  SIGUSR2 + exit only, both now source-confirmed to run the same dispose
  leg.

#### Does SIGUSR2 reload at waybar v0.15.0 destroy/re-init CFFI modules on every reload or only when the config changed?

2026-07-22.

- Examined: waybar 0.15.0 tag sources fetched from
  https://raw.githubusercontent.com/Alexays/Waybar/0.15.0/ — `src/main.cpp`,
  `src/client.cpp`, `src/bar.cpp`, `src/modules/cffi.cpp`,
  `include/util/kill_signal.hpp`.
- Found: reload is an unconditional full teardown/re-init. Chain: signal
  thread → `handleUserSignal` → RELOAD action (default:
  `const KillSignalAction SIGNALACTION_DEFAULT_SIGUSR2 =
  KillSignalAction::RELOAD;` in kill_signal.hpp; per-bar override
  `"on-sigusr2"`, bar.cpp:296-305) → `reload = true;
  waybar::Client::inst()->reset();` (main.cpp:102-106) → `reset()` calls
  `gtk_app->quit()` (client.cpp:318-322) → `Client::main` finishes with
  `bars.clear(); return 0;` (client.cpp:313-315) → main.cpp's
  `do { reload = false; ret = client->main(argc, argv); } while (reload);`
  (main.cpp:173-176) re-runs `Client::main`, which re-reads the config from
  disk (`config.load(config_opt)`, client.cpp:286) and rebuilds every bar →
  `setupWidgets` → `getModules` → `factory.makeModule` → new `CFFI(...)` →
  fresh `wbcffi_init` (cffi.cpp:89). `bars.clear()` destroys each `Bar`
  synchronously (`waybar::Bar::~Bar() = default;`, bar.cpp:324 — members
  including the `Gtk::Window` and the `modules_all_` shared_ptrs are
  destroyed), so `CFFI::~CFFI` → `hooks_.deinit(cffi_instance_)`
  (cffi.cpp:97-101) and the GTK widget-tree dispose (the p9c Engine-drop leg)
  run on every reload.
- Not found: any config diffing, module caching, or bar reuse across reload
  anywhere on this path.
- Conclusion: resolved — every SIGUSR2 reload destroys and re-creates all
  CFFI module instances; the workload does not need to mutate the config file
  between SIGUSR2s.
