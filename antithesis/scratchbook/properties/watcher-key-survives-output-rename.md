# watcher-key-survives-output-rename

Focus: distributed coordination — writer and readers coordinate through a
string key (`"<output>:<idx>"`) whose left half is resolved *once, at reader
spawn, from a compile-time constant*, while the writer re-derives it live.
Output identity change permanently partitions readers from the writer.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

- `/home/chussenot/agentic-db/internal/tile/tile.go:44` — `defaultOutput =
  "HDMI-A-1"` (compile-time constant), overridable only via `--output`.
- `/home/chussenot/agentic-db/internal/tile/tile.go:501-511` — `RunWatch`
  resolves `key := Key(output, idx)` **once at startup** and never
  re-resolves. `Key` is `output + ":" + itoa(idx)` (tile.go:82).
- The documented waybar config (waybar-pwetty-box
  `tiles/claude/README.md:29-30`) is `"exec": "claude-status tile-watch 5"` —
  **no `--output`**, so every deployed watcher is keyed to the baked-in
  constant.
- Writer side: `/home/chussenot/agentic-db/internal/tile/tile.go:411-413` —
  `BuildAll` keys the cache by `Key(ws.Output, ws.Idx)` from the daemon's
  **live** niri model; connector renames flow into new cache keys on the next
  write.
- Miss behavior: `emit()` (tile.go:517-521) — key absent → emptyPayload
  (idle level-6), indistinguishable from a genuinely empty desktop.
- No recovery path exists: the plugin's respawn (waybar-pwetty-box
  `src/content.rs:260-284`) re-execs the same literal command string → same
  stale output name. Only editing the waybar config + reload changes the key.

## Failure scenario

1. Steady state: watchers keyed `HDMI-A-1:1..N`; cache keys match; tiles live.
2. Output identity changes: dock replug enumerates the connector as
   `DP-3` (or the monitor is unplugged and niri migrates its workspaces to
   the surviving output — keys move to the other output's name *and* the
   indexes are renumbered: niri appends migrated workspaces to the primary
   output's list right before its trailing empty workspace, and the IPC
   `idx` is recomputed positionally — see Investigation Log; both halves of
   `Key(output, idx)` change).
3. Daemon publishes the next cache keyed `DP-3:*`. Every watcher's
   `HDMI-A-1:<idx>` lookup misses **forever**.
4. Every tile on the bar renders the idle placeholder. A `prompt` session is
   masked (F9/F10 in sut-analysis §11). No error anywhere: the daemon is
   healthy, the watchers are healthy, the pipeline is "working".
5. Kill/respawn of watchers, daemon restart, waybar reload — none of them
   reconverge; the partition is permanent until a human edits config.

This is the coordination gap: writer and readers share no discovery or
subscription protocol for output identity — the "membership" half of the key
is frozen into N reader processes at spawn.

## Suggested assertions (net-new)

- Workload `Always` (evaluated at quiescent checkpoints): message **"every
  live tile-watch key resolves in the tile cache when its desktop exists"** —
  condition: for each running `tile-watch <idx>` process, if `niri msg -j
  workspaces` reports a workspace with index `idx` on *any* output, then
  `Key(watcher_output, idx)` ∈ keys(`tiles.json`). Key-missing is legitimate
  only when no such workspace exists (fewer desktops than configured tiles).
  Watcher `output` is recoverable from the process cmdline (`--output` or
  known default).
- Workload `Sometimes` (coverage guard): message **"output topology changed
  while watchers were live"** — the Always is vacuous for this hazard unless
  the environment actually changes output identity during the run.

SUT-side instrumentation is deliberately not proposed: a `Sometimes(key
present)` inside `emit()` fires trivially early in any run and stays
satisfied after the strand, so it cannot detect the permanent miss; the
quiescence cross-check needs the workload's view of niri + the process table.

## Fault requirements / feasibility

Requires the harness to change output identity under a live pipeline.
**Settled 2026-07-22** (see Investigation Log): not achievable with real
niri, on any backend, by any configuration:

- winit backend: exactly one output, name hardcoded `"winit"` — so even
  *restarting* the nested niri never changes the connector name across
  generations (that fallback is dead).
- niri headless backend: test-only (zero outputs, internal-only
  `add_output`, no IPC exposure, incomplete frame timing).
- Virtual-output IPC (`create-virtual-output`) is an unmerged community
  proposal with no progress as of Feb 2026; maintainer confirmed "Not at
  the moment" for headless outputs (Oct 2024).
- Replacing the *outer* compositor (e.g. sway headless, which does hotplug —
  see `output-readd-tile-recovers`) does not help this property: the
  writer's keys come from the **daemon's niri model**, and niri's outputs
  are fixed.

The property therefore runs as the **misconfiguration variant**: spawn
watchers with a `--output` that doesn't match niri's live output name (or
flip a watcher's `--output` across a respawn while the daemon stays up).
This produces the identical silent-idle end state and validates the Always
assertion logic; only the physical rename *trigger* is out of reach. The
catalog's Antithesis Angle should be scoped to that variant. (A niri-IPC
impostor — replaying a recorded event stream with rewritten output names on
a fake `NIRI_SOCKET` — could exercise the true rename transition, at the
cost of faking the compositor for the daemon; note for workload design, not
assumed.)

## Key observations

- The strand also fires without any hardware event if the environment simply
  never had an output named `HDMI-A-1` — misconfiguration and rename produce
  the identical silent-idle end state, so the Always assertion doubles as a
  deployment sanity check inside the harness.
- Cheap real-world fix shapes (for the eventual regression framing): watcher
  re-resolves its key when a lookup misses but the cache is non-empty
  (`if key missing and len(tiles)>0, rescan for any key ending ":<idx>"`), or
  the daemon writes output-agnostic alias keys. Either turns the permanent
  partition into a one-poll blip.
- Related but distinct: `daemon-restart-no-placeholder-clobber` (missing key
  because the *cache* is transiently wrong) and `read-error-never-invents-idle`
  (read failure); here the cache is correct and readable — the *key contract*
  itself broke.

## Open questions

None.

### Investigation Log

#### Can the Antithesis environment change niri output identity mid-run (multiple virtual outputs / hotplug on a headless backend)?

2026-07-22.

- Examined: niri sources
  https://raw.githubusercontent.com/YaLTeR/niri/main/src/backend/mod.rs
  (backend enum: Tty/Winit/Headless), .../src/backend/winit.rs,
  .../src/backend/headless.rs, .../src/main.rs (backend selection); niri
  discussions https://github.com/niri-wm/niri/discussions/714 (headless
  output request) and https://github.com/niri-wm/niri/discussions/3101
  (virtual-output API design); niri wiki Configuration:-Outputs.md; local
  `test/shot.sh` + `test/niri.kdl` (static read).
- Found: winit backend creates exactly one output, name hardcoded
  `"winit"`, no on/off or hotplug handling — the name is identical across
  restarts, so cross-generation rename via compositor restart is
  impossible too. The headless backend creates zero outputs by default;
  `add_output` (names `headless-{n}`) is internal with no CLI/IPC exposure
  found, and the backend self-describes as incomplete ("missing some
  crucial parts like dmabufs", no real frame timing). Maintainer on
  headless outputs: "Not at the moment" (YaLTeR, Oct 2024, #714). The
  `create-virtual-output` IPC proposal (#3101) is unmerged; asked for
  status, a collaborator answered "No" (Feb 2026).
- Not found: any niri mechanism — config, IPC, env var, backend — that
  creates, destroys, or renames an output at runtime; any way to make the
  winit connector name vary.
- Conclusion: resolved — **no**. The rename *trigger* is unreachable with a
  real niri in the harness; the property is scoped to the misconfiguration
  variant (wrong/flipped `--output` on watchers), which exercises the same
  strand mechanism and the same assertions. Catalog Antithesis Angle should
  say so. Optional future lever: a niri-IPC impostor replaying renamed
  events on a fake `NIRI_SOCKET`.

#### When a monitor is unplugged, does niri migrate workspaces with their indexes intact, or renumber them?

2026-07-22.

- Examined: niri source at tag v26.04 (matches the locally installed
  `niri 26.04`), fetched from
  https://raw.githubusercontent.com/YaLTeR/niri/v26.04/ —
  `src/layout/mod.rs` (`remove_output`, `add_output`, module doc comment),
  `src/layout/monitor.rs` (`append_workspaces`), `src/ipc/server.rs`
  (workspace serialization). Same code present on `main`.
- Found: indexes are **renumbered**, not preserved. `remove_output` moves
  the dead monitor's workspaces to the primary monitor via
  `primary.append_workspaces(workspaces);` (layout/mod.rs:886).
  `append_workspaces` inserts them "in the end, right before the last,
  empty, workspace": `let empty = self.workspaces.remove(
  self.workspaces.len() - 1); self.workspaces.extend(workspaces);
  self.workspaces.push(empty);` (monitor.rs:736-740). The IPC `idx` is
  purely positional over the monitor's current workspace vector:
  `idx: u8::try_from(ws_idx + 1).unwrap_or(u8::MAX)` (ipc/server.rs:668).
  So a workspace that was `DP-3:2` becomes roughly
  `<primary>:(primary_nonempty_count + 2)` — both key halves change.
  Workspace *ids* (the `id` field) are stable across migration; only
  output/idx move. On reconnect, workspaces whose
  `original_output.matches(&output)` are pulled back to the re-added
  monitor in preserved relative order (layout/mod.rs:747-763, 798), so
  original indexes are approximately restored if no workspaces were
  created/closed meanwhile (unnamed empty workspaces are culled:
  `if ws.has_windows_or_name()`, layout/mod.rs:762).
- Not found: any path preserving per-output `idx` across migration, or any
  index-based (rather than position-based) IPC reporting.
- Conclusion: resolved — post-unplug the surviving output reports workspaces
  at indexes 1..(own + migrated), so the Always condition's "a workspace
  with index idx exists on *some* output" leg stays satisfiable for the
  watcher's low indexes, and the watcher's `HDMI-A-1:<idx>` lookup misses:
  the assertion fires on the strand as designed. Workloads should treat
  workspace `id` (not `idx`) as the stable identity when tracking a desktop
  across topology changes.
