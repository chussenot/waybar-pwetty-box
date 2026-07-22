# reload-conserves-producer-chains — evidence

No Antithesis instrumentation exists anywhere in this codebase (see
`existing-assertions.md`); every assertion suggested here is net-new.

## Claim

Across N SIGUSR2 reloads of waybar, the plugin's live resources stay
proportional to the current module count: exactly one reader thread, one
`sh`/`tile-watch` producer chain, and one 150ms dirty-poll glib timer per
stream-mode module instance. At f87ec19 this is **known violated by code**
(the "reload resource leak" in sut-analysis §5, confirmed-by-code, unfiled):
the property exists to (a) demonstrate the leak under Antithesis and (b) pin
the invariant once fixed.

## Code paths (all verified at f87ec19)

- `src/content.rs:207-215` (poll) and `src/content.rs:259-284` (stream):
  `thread::spawn` with comment "Detached: lives for the process (waybar
  modules are process-lifetime)" — a false premise under reload. The stream
  thread loops forever: read lines → `child.wait()` → sleep 1s → respawn.
  Nothing ever signals it to stop; no handle is retained.
- `src/lib.rs:366-374`: `gtk::glib::timeout_add_local(150ms, ...)` closure
  captures a strong `area.clone()` (GTK widget ref) and the `ContentStore`
  clone, and always returns `ControlFlow::Continue`. The timer is never
  removed; the strong ref keeps the widget object alive after waybar destroys
  its module tree.
- `src/lib.rs:1665`: `waybar_module!(PwettyBox)` — grep confirms no `deinit`
  implementation anywhere in `src/lib.rs`; the crate default deinit only drops
  the module box. Nothing tears down threads, children, or timers.
- `src/lib.rs:351-361`: the frame-clock tick callback is widget-bound
  (`add_tick_callback`), so it likely stops firing once the widget is
  unrealized — the immortal leak is the 150ms timeout, not the tick.
- Waybar 0.15.0 reload orchestration (verified from tag source, 2026-07-22 —
  see Investigation Log): SIGUSR2 → RELOAD (the default) → `gtk_app->quit()`
  → `bars.clear()` (waybar src/client.cpp:314) → `Client::main` re-runs and
  rebuilds every bar and module (main.cpp:173-176). No config diffing
  anywhere: all M CFFI instances are destroyed and re-created on **every**
  reload, so the leak formula is exactly (N+1)×M with a bare
  `kill -USR2` — no config mutation needed. Waybar's `CFFI::~CFFI` calls
  only `hooks_.deinit` (src/modules/cffi.cpp:97-101); it closes no fds and
  joins no threads, so the plugin's leaked reader thread keeps the old pipe
  read end open and old producer chains never receive EPIPE.

## Failure scenario

Live deployment is 10 stream-mode module instances (sut-analysis §3). Each
SIGUSR2 reload re-runs `wbcffi_init` for all 10 → 10 new threads + 10 new
`sh -c "claude-status tile-watch <idx>"` chains + 10 new timers, while the 10
old ones keep running (old threads stay blocked reading the old children's
stdout; old children keep polling tiles.json every 75ms and respawn if they
die). After N reloads: `(N+1)×10` producer chains all reading tiles.json,
`(N+1)×10` threads in the waybar process, `(N+1)×10` timers each doing a
150ms atomic-swap + potential `queue_draw` on a zombie widget. Unbounded
process/thread/CPU growth with a trivially reachable trigger (config touch,
`killall -USR2 waybar`).

## Suggested assertions (net-new)

Workload-observable (no SUT change needed):

- `Always`: "tile-watch producer chain count equals live stream-module count
  after reload settle" — after each reload + settle window, count processes
  matching `claude-status tile-watch` (and their `sh` parents) descended from
  the waybar PID; must equal configured stream-module count.
- `Always`: "waybar thread count stays flat across reloads" — sample
  `/proc/<waybar_pid>/status` `Threads:` after each reload; must stay ≤
  (baseline measured after first init) + slack, independent of reload count.

SUT-side (net-new instrumentation):

- Atomic counter of live dirty-poll timers (increment at registration; a
  fixed timer would decrement on `Break`): `Always`: "live dirty-poll timer
  count equals live module-instance count".

## Key observations

- **Sequencing with the p9c teardown crash**: at f87ec19 a reload may abort
  waybar (Engine drop without current GL context) before the leak can
  accumulate — the crash currently masks the leak. This property becomes the
  *primary* reload property once the fix branch
  (`worktree-fix-gl-teardown-crash`, 30100f9) merges. In Antithesis, expect
  the crash property to fire first on this commit.
- Old leaked chains are not idle: leaked tile-watch children keep emitting on
  every tiles.json change, leaked threads keep parsing + publishing to leaked
  stores, leaked timers keep polling — the leak costs CPU and pipe traffic,
  not just memory.
- Couples with `respawn-backoff-floor-holds`: leaked threads each respawn at
  1/s on producer failure, so aggregate fork rate scales with reload count.

## Open questions

- Does `queue_draw` on the leaked (unparented, kept-alive-by-timer) widget do
  measurable work, or is it a no-op for an unrealized widget? Matters: whether
  the leak has a per-timer CPU component worth asserting on (CPU-time bound)
  or only process/thread/memory components.

### Investigation Log

#### Does waybar v0.15.0's SIGUSR2 reload destroy and re-init every CFFI module on every reload, or only when its config block changed?

2026-07-22.

- Examined: waybar 0.15.0 tag sources fetched from
  https://raw.githubusercontent.com/Alexays/Waybar/0.15.0/ — `src/main.cpp`,
  `src/client.cpp`, `src/bar.cpp`, `src/modules/cffi.cpp`,
  `include/util/kill_signal.hpp`.
- Found: every reload, unconditionally. SIGUSR2's default action is RELOAD
  (`SIGNALACTION_DEFAULT_SIGUSR2 = KillSignalAction::RELOAD`,
  kill_signal.hpp); the RELOAD arm sets `reload = true` and calls
  `Client::reset()` → `gtk_app->quit()` (main.cpp:102-106,
  client.cpp:318-322); the ending `Client::main` iteration runs
  `bars.clear();` (client.cpp:314), destroying every Bar and therefore every
  CFFI instance (`CFFI::~CFFI` → `hooks_.deinit`, cffi.cpp:97-101); the
  `do { ... } while (reload)` loop in main.cpp:173-176 then re-runs
  `Client::main`, which re-reads the config from disk (client.cpp:286) and
  reconstructs all bars/modules via `getModules` → `factory.makeModule` →
  new `CFFI` → fresh `wbcffi_init`. No config comparison exists on the path.
- Not found: any module reuse or config-diff short-circuit.
- Conclusion: resolved — expected-count formula is (N+1)×M; plain
  `kill -USR2` per reload suffices, no config-mutating reloads needed.

#### What is a fair settle window after SIGUSR2 before counting?

2026-07-22.

- Examined: waybar 0.15.0 `src/modules/cffi.cpp` (destructor), plus the
  already-verified plugin thread ownership (`src/content.rs:259-284`).
- Found: waybar's module destruction closes nothing — `CFFI::~CFFI` calls
  only `hooks_.deinit(cffi_instance_)` (cffi.cpp:97-101), and the deinit
  just drops the module box; the old pipe read ends are owned by the
  plugin's leaked detached threads, which nothing stops. Old children
  therefore never see EPIPE and old chains persist indefinitely; counts
  never converge downward on their own.
- Conclusion: resolved — the settle window only needs to cover the *new*
  generation's spawn (thread + `sh`/tile-watch exec, ~1-2s is generous);
  there is no old-generation die-off to wait out, so a short fixed grace
  period after each SIGUSR2 is safe and cannot mask the leak.
