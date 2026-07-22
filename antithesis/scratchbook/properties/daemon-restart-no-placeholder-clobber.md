# daemon-restart-no-placeholder-clobber

Focus: failure recovery — daemon death/restart; recovery procedure that assumes clean (populated) state.

All suggested assertions are **net-new**; the codebase has no Antithesis instrumentation (see `existing-assertions.md`).

## Code paths (all in /home/chussenot/agentic-db)

- `internal/daemon/daemon.go:180-248` — actor loop. `pollDB` **primes a DB snapshot immediately** (`daemon.go:283`), which marks dirty and arms the **13ms debounce**; `reconcile` (`daemon.go:501-511`) then calls `maybeWriteTiles`.
- `internal/daemon/daemon.go:382-389` — `maybeWriteTiles`: `lastTileBuild` is the zero time on a fresh daemon, so the very first reconcile **always** proceeds to `writeTiles`.
- `internal/daemon/daemon.go:397-419` — `writeTiles` has **no guard on model population**: `BuildAll` over an empty `model.Workspaces()` returns `{}`; `lastTiles` starts nil ≠ `"{}"`, so the dedupe passes and **`{}` is written over the previous good cache** (`tile.WriteCacheBytes`).
- `internal/niri/eventstream.go:148-178` — `StreamEvents` spawns `niri msg -j event-stream` as a child and feeds events **asynchronously**; the initial WorkspacesChanged/WindowsChanged snapshot arrives only after child spawn + niri IPC (multi-ms, unbounded under scheduling faults). Nothing synchronously primes the model before the first reconcile.
- `internal/tile/tile.go:511-531` — `tile-watch` polls the cache at 75ms; a **missing key → `emptyPayload`** (`tile.go:516-521`), which is a single **idle session at max decay level** (`tile.go:93-99`) — visually "long idle".
- `daemon.go:217-221` — the daemon **exits** when the niri event stream closes; only recap timers ship as systemd units (`share/systemd/claude-daily-recap.*` — no daemon unit, no `Restart=`); the daemon runs via niri `spawn-at-startup` (README:145-147), so **nothing restarts it** — restart is manual or workload-driven.

## Failure scenario (the race)

Daemon killed and restarted while a session is in `prompt` state:

1. t=0: restart. `pollDB` primes a snapshot within ~1ms (in-process SQLite read).
2. t≈13ms: debounce fires → `reconcile` → `writeTiles`. If the niri event-stream child has not yet delivered its initial snapshot (child spawn + IPC, easily >13ms under Antithesis scheduling), the model is empty → **`tiles.json` is clobbered with `{}`**.
3. Within 75ms: every `tile-watch` reads `{}`, misses its key, and **pushes `emptyPayload` (idle level-6) over the live `prompt` tile** — the one state the product exists to surface, silently replaced by plausible normality.
4. Once the initial niri events land, the next reconcile rewrites the correct cache and tiles reconverge — unless the daemon dies again inside the window, in which case the clobbered `{}` cache **persists indefinitely** (tile-watch keeps serving placeholders; no restart unit exists).

This is the SUT analysis §7 claim "transient cache-miss during daemon restart actively pushes placeholder over good state", now grounded to the exact ordering: immediate DB prime + 13ms debounce vs asynchronous event-stream child startup, with no model-population guard in `writeTiles`.

## Suggested assertions (net-new, Go SUT side + workload)

- SUT `Always` in `writeTiles`: message **"daemon never publishes an empty tile cache over a populated one"** — condition: `len(tiles) > 0 || previous cache file was empty/absent || the model legitimately has zero workspaces (post-adoption, niri reported an empty WorkspacesChanged)`. The last disjunct is REQUIRED: niri can legitimately report zero workspaces (headless start, or all outputs disconnected and the retained workspaces culled as windows close — see Investigation Log), so a bare `len(tiles) > 0` false-positives. The discriminating guard is `d.adopted` (daemon.go:254-260): before the first `WorkspacesChanged` the empty model means "not ready", after it an empty model means niri really said so.
- Workload `Always`: with a fixture session held in `prompt`, across repeated daemon SIGKILL/restart cycles, the prompt desktop's key in `tiles.json` **never maps to an idle payload**: message **"prompt desktop key never regresses to idle placeholder across daemon restart"**.
- Workload `Sometimes` (coverage guard): message **"daemon restarted while a prompt session was live"** — without this the Always checks can pass vacuously.

## Fault requirements

SIGKILL of the daemon process inside its container, driven by the workload — **no node-termination faults required**. Antithesis scheduling/network faults widen the 13ms-vs-child-spawn race in both directions, which is exactly what makes this property valuable.

## Key observations

- The clean-state assumption is double: `writeTiles` assumes the model is populated, and `tile-watch` assumes a missing key means "desktop empty" rather than "producer not ready". Either assumption alone would be survivable; together they convert a restart into an active false-state push.
- A one-line fix exists (skip `writeTiles` until `d.adopted` / first WorkspacesChanged), which makes this a crisp regression target after the fix.

## Open questions

- How wide is the race window on real hardware vs under Antithesis (13ms debounce vs `niri msg` child spawn)? `(partial: ordering confirmed from code — DB prime is in-process and immediate, event stream is an async child process; absolute timings unmeasured)` Why it matters: only affects how often the workload must restart the daemon to hit the window, not the property's validity.

### Investigation Log

#### Does niri's event stream ever legitimately report zero workspaces?

Investigated 2026-07-22 against niri `main` (github.com/YaLTeR/niri, fetched
2026-07-22) and docs.rs niri-ipc latest.

- Examined: niri `src/layout/mod.rs` (`Layout::with_options`,
  `remove_output`→`MonitorSet::NoOutputs`, `remove_window`'s NoOutputs branch,
  the `workspaces()` iterator), `src/ipc/server.rs` (WorkspacesChanged
  construction), docs.rs `niri_ipc::Workspace`; consumer side:
  `internal/niri/eventstream.go` (Workspace decode) and `tile.BuildAll` keying.
- Found: YES, zero workspaces is a legitimate niri state, two ways. (1) A
  compositor with no outputs and no config-declared workspaces starts at
  `monitor_set: MonitorSet::NoOutputs { workspaces: vec![] }`
  (layout/mod.rs, `with_options`). (2) When the last output disconnects, the
  workspaces ARE retained (`MonitorSet::NoOutputs { workspaces }` in
  `remove_output`, no filtering) — but in the NoOutputs state a workspace is
  culled the moment its last window closes: `// Clean up empty workspaces.
  if !ws.has_windows_or_name() { workspaces.remove(idx); }` (remove_window's
  NoOutputs arm), so the vec drains to zero. The IPC layer emits exactly
  `layout.workspaces()` — which chains the NoOutputs vec — so
  `WorkspacesChanged` carries an empty `workspaces` array in that state
  (ipc/server.rs, `need_workspaces_changed` block). Additionally, docs.rs
  confirms `Workspace.output: Option<String>` — "Can be None if no outputs are
  currently connected" — so even the RETAINED zero-output workspaces arrive
  with `output: null`, which the Go decode turns into `""`
  (eventstream.go:41-42), making every `BuildAll` key `":idx"` and every
  tile-watch key (`"HDMI-A-1:idx"`) miss regardless.
- Not found: any invariant in niri guaranteeing ≥1 workspace globally (the
  per-monitor "always one empty workspace" rule applies only to
  `MonitorSet::Normal`).
- Conclusion: RESOLVED — the SUT-side Always as originally stated
  (`len(tiles) > 0`) would false-positive on genuine zero-output states
  (laptop lid close / dock unplug is a realistic trigger). The assertion needs
  the no-outputs escape hatch; `d.adopted` + the last-received
  WorkspacesChanged payload distinguish "model not primed yet" (the defect
  window) from "niri reported zero workspaces" (legitimate). Suggested
  assertion updated accordingly. Note the escape hatch also has to tolerate
  the `output: null` form, where workspaces exist but none matches any tile's
  output key.
