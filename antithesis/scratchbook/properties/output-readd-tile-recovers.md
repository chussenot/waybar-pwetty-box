# output-readd-tile-recovers

Focus: lifecycle transitions — output add/remove/rename cycles: after an
output disappears and reappears, its bar and tiles must come back with live
content.

All suggested assertions are **net-new**; no Antithesis instrumentation exists
anywhere in this codebase (see `existing-assertions.md`).

## Claim

After an output remove + re-add cycle (lid close/open, dock replug), waybar
re-creates the bar for that output, `wbcffi_init` runs fresh instances, new
tile-watch chains spawn, and every tile on that output converges to live
backend content. This is the *recovery* half of the output-churn story
(`module-teardown-never-aborts-host` is the *survival* half); it composes
teardown + re-init + startup convergence + the backend's connector-name
keying into one end-to-end cycle.

## Code paths (verified)

- Re-init leg: `wbcffi_init` → `PwettyBox::init` (`src/lib.rs:162-380`) is
  stateless across instances except process-global caches (thread-local
  `INK_CACHE`, negative `ICON_CACHE`, `gl::ensure_loaded` Once) — none of
  which block a correct re-init. Each init spawns a fresh producer chain
  (`src/content.rs:256-285`).
- Convergence leg: identical mechanism to `cold-start-stream-tile-converges`
  (tile-watch initial emit + 75ms cache poll + 150ms dirty poll).
- **Rename-stranding leg** (backend): tile-watch resolves its cache key from
  a connector name — `defaultOutput = "HDMI-A-1"` hardcoded
  (`/home/chussenot/agentic-db/internal/tile/tile.go:41-44,501`), overridable
  only via `-output`; the documented wiring (`tiles/claude/README.md:25-31`,
  `"exec": "claude-status tile-watch 5"`) passes **no** `-output` flag. If
  the compositor re-adds the display under a different connector name (dock
  replug commonly does: DP-3 → DP-4), the daemon writes cache keys under the
  new name while every tile-watch keeps looking up the old key →
  `emptyPayload` forever (tile.go:516-521). All desktops on that output
  render as idle; severity S2.
- Waybar's per-output bar orchestration (verified from the 0.15.0 tag source,
  2026-07-22 — see Investigation Log): removal and re-add are both automatic
  and live for the whole run. Removal: `signal_monitor_removed` →
  `handleMonitorRemoved` defers to an idle callback →
  `handleDeferredMonitorRemoval` hides the window, removes it from the app,
  and erases the bar (waybar src/client.cpp:125-150) — module destructors
  (CFFI deinit → widget-tree dispose) run here. Re-add:
  `signal_monitor_added` → `handleMonitorAdded` → xdg_output `.done` →
  `handleOutputDone` → `bars.emplace_back(std::make_unique<Bar>(&output,
  config))` (client.cpp:84) for every config block matching the output.
  Matching caveat: a config with no `"output"` field matches any output
  (src/config.cpp:211 `return true;`); a config pinned to
  `"output": "HDMI-A-1"` matches name-or-identifier only
  (config.cpp:201-208), so a re-add under a *new* connector name creates
  **no bar at all** — a whole-bar-absent variant distinct from the silent
  tile stranding below.

## Failure scenario

1. Output removed → bar destroyed → (at f87ec19: SIGABRT, see
   `module-teardown-never-aborts-host`; this property presumes that fix).
2. Output re-added:
   - Same connector name: expect full recovery ≈ bar creation + init + initial
     emit + dirty poll. A failure here (blank bar, stale placeholder) means
     the re-init leg broke — e.g. leaked state from generation 1, or the
     accumulated-producer interference documented in
     `reload-conserves-producer-chains`.
   - Different connector name: **known-mechanism stranding** — tiles render
     the empty placeholder forever while the daemon's cache holds live state
     under the new key. Nothing crashes, nothing logs.
3. Repeated cycles compound with the reload leak: each cycle adds a producer
   generation (old chains never die), so recovery latency and CPU drift
   upward with cycle count.

## Suggested assertions (net-new)

Workload-side:

- `Sometimes`: message **"tile on a re-added output rendered live session
  content after an output remove/add cycle"** — the core recovery condition;
  `Sometimes` because it is a progress property gated on an environmental
  event the workload triggers.
- `Sometimes`: message **"an output was removed and re-added while sessions
  were live"** — coverage anchor for the cycle itself.
- `Always` (rename variant, backend-checkable without pixels): message
  **"every tile-watch key resolves to a key present in the daemon cache"** —
  periodically compare the keys tile-watch instances were started with
  (workload knows the exec lines) against `tiles.json`'s key set once the
  daemon is steady; a persistent miss is the stranding. This turns the
  silent S2 into a checkable condition without rendering introspection.

Backend SUT-side (candidate, in `RunWatch`):

- `Sometimes`: message **"tile-watch key missing from a non-empty cache for
  over 60 consecutive polls"** — in-process detection of sustained stranding
  (5s at 75ms polls), distinguishing "daemon not up" from "key will never
  match". Worth adding only if the rename variant is implementable in the
  harness.

## Fault / harness requirements

- **Output hotplug in the headless compositor** — the hard requirement.
  **Settled 2026-07-22** (see Investigation Log): NOT possible in the
  `test/shot.sh` topology — niri's winit backend creates exactly one fixed
  output named `winit` and niri has no runtime output create/destroy on any
  backend (its headless backend is test-only, zero outputs, no IPC exposure;
  virtual-output support is an unmerged community proposal). Cage's own
  output capabilities are moot: cage outputs don't map to niri-winit outputs
  (one nested window = one niri output).
  **Achievable replacement stack**: outer compositor = sway with
  `WLR_BACKENDS=headless` (same pixman/noop-seat knobs — sway is wlroots like
  cage); waybar runs on sway; niri (winit) stays nested inside sway purely as
  the daemon's event source. Runtime hotplug via `swaymsg create_output`
  (creates `HEADLESS-N`, 1920x1080) and `swaymsg output <name> unplug`
  (calls `wlr_output_destroy` — clients see the output vanish; restricted to
  headless/wayland/x11 backends, sway ≥1.8). Caveats: wlroots names headless
  outputs with a monotonically increasing counter, so unplug+create always
  produces a *new* name (natural renamed-re-add; the same-name leg needs
  `output <name> disable`/`enable`, whose wl_output-global behavior is
  unverified — one local check settles it). The plugin/tile pipeline is
  compositor-agnostic (tile-watch reads tiles.json), so waybar-on-sway keeps
  waybar + the plugin + the daemon all real.
- Presumes the p9c teardown fix is merged; at f87ec19 step 1 aborts waybar
  and the property cannot be evaluated past it.

## Key observations

- The rename leg needs no hotplug at all to *demonstrate* mechanically: start
  the stack with an output name ≠ the tile-watch key's name and the stranding
  is immediate. Hotplug only makes it a *transition* property rather than a
  misconfiguration property; the `Always` key-resolution assertion covers
  both shapes.
- Recovery on the same-name path shares its convergence machinery with
  `cold-start-stream-tile-converges`; if both fail together, suspect the
  convergence leg, if only this one fails, suspect waybar's bar re-creation
  or plugin re-init.
- **Live output binding (resolved 2026-07-22): mixed.** The live config
  (`~/.config/waybar/config.jsonc`) has two pwetty bars, each pinned to one
  output via waybar's `"output"` key: the HDMI-A-1 bar's exec lines are
  **flagless** (`claude-status tile-watch N` — works only because
  tile-watch's hardcoded default happens to *be* HDMI-A-1), the eDP-1 bar's
  pass `--output eDP-1` explicitly. A config comment (config.jsonc:334-336)
  documents the stranding in the user's own words: "Without it, tile-watch
  defaults to HDMI-A-1 and every tile reads idle" — the failure mode was
  evidently hit in practice. Consequences: (1) with both bars
  waybar-`"output"`-pinned, a renamed connector means waybar creates *no bar
  at all* for the new name (config.cpp:201-208) — on this deployment the
  rename failure presents as bar-absent (waybar-level); the stranded-tile
  shape needs a bar whose `"output"` matches while its exec's key doesn't —
  exactly the copy-paste drift the flagless HDMI bar invites. (2) The
  harness should model both wiring shapes (flagless + explicit `--output`)
  so the `Always` key-resolution assertion distinguishes them.

## Open questions

None — all three resolved 2026-07-22 (see Investigation Log); the property
still presumes the p9c teardown fix (Fault / harness requirements).

### Investigation Log

#### Can the harness compositor add/remove/rename virtual outputs at runtime?

2026-07-22.

- Examined: `test/shot.sh`, `test/niri.kdl`, `test/orchestrate.sh` (local,
  static read only); niri sources
  https://raw.githubusercontent.com/YaLTeR/niri/main/src/backend/mod.rs,
  .../src/backend/winit.rs, .../src/backend/headless.rs, .../src/main.rs;
  niri discussions https://github.com/niri-wm/niri/discussions/714 and
  https://github.com/niri-wm/niri/discussions/3101; niri wiki
  Configuration:-Outputs.md; sway sources
  https://raw.githubusercontent.com/swaywm/sway/master/sway/commands/create_output.c
  and .../sway/commands/output/unplug.c; sway PR
  https://github.com/swaywm/sway/pull/7192 and issue
  https://github.com/swaywm/sway/issues/7374; wlroots
  https://raw.githubusercontent.com/swaywm/wlroots/master/backend/headless/output.c
  and .../include/backend/headless.h; cage
  https://github.com/cage-kiosk/cage (README/wiki via search).
- Found: (a) cage supports multi-output and hotplug of *its* outputs, but it
  has no IPC to create/destroy outputs on demand, and cage-level outputs are
  irrelevant to waybar anyway — waybar's outputs are niri's, and niri's winit
  backend creates exactly one output hardcoded `"winit"`
  (`Output::new("winit".to_string(), ...)`) with no on/off or hotplug
  handling. (b) niri has a `Headless` backend variant, but it is test-only:
  creates zero outputs by default, outputs named `headless-{n}` added only
  via an internal `add_output` (no CLI/IPC exposure found in main.rs), and
  the file itself says it is "missing some crucial parts"; render marks
  presentation complete without real frame timing. Maintainer on headless
  outputs: "Not at the moment" (Oct 2024, discussion #714); a
  `create-virtual-output` IPC proposal exists but is unmerged with "No"
  progress as of Feb 2026 (discussion #3101). (c) sway on
  `WLR_BACKENDS=headless` has true runtime hotplug: `swaymsg create_output`
  → `wlr_headless_add_output(backend, 1920, 1080)`; `swaymsg output <name>
  unplug` → `wlr_output_destroy` (headless/wayland/x11 backends only; sway
  ≥1.8, PR #7192). wlroots names headless outputs `HEADLESS-%zd` from a
  counter that never resets, so recreated outputs always get a new name
  (issue #7374) — rename-on-readd is the *default* behavior. wlroots
  headless outputs emit frame events from a `wl_event_loop_add_timer` at
  `HEADLESS_DEFAULT_REFRESH` = 60 Hz.
- Not found: whether sway's `output <name> disable`/`enable` destroys and
  re-creates the client-visible wl_output global (needed for the same-name
  re-add leg; sway/config/output.c and sway/tree/output.c excerpts didn't
  show the global calls) — one local nested-sway check settles it, or fall
  back to full sway restart for the same-name leg.
- Conclusion: resolved. True output hotplug is NOT achievable in the current
  cage → niri(winit) stack and cannot be added by configuration — niri
  cannot create/destroy/rename outputs at runtime on any backend. It IS
  achievable in a container with: **sway (WLR_BACKENDS=headless, pixman,
  noop seat) hosting waybar + the plugin, with niri (winit) nested inside
  sway solely as the claude-status daemon's event source**. That stack keeps
  waybar, the plugin, tile-watch, and the daemon all real, and gives
  remove/re-add (unplug + create_output) and rename-on-readd (automatic)
  triggers. Note the backend rename-*stranding* trigger is separate and
  remains impossible (the daemon's keys come from *niri's* outputs, which
  are fixed) — see `watcher-key-survives-output-rename`.

#### Does waybar v0.15.0 re-create a bar (and re-run `wbcffi_init`) for a re-added output automatically?

2026-07-22.

- Examined: waybar 0.15.0 tag sources fetched from
  https://raw.githubusercontent.com/Alexays/Waybar/0.15.0/ —
  `src/client.cpp`, `src/config.cpp`, `src/bar.cpp`, `include/client.hpp`.
- Found: yes, automatic in both directions, keyed on GDK monitor signals
  connected once per `Client::main` iteration
  (`gdk_display->signal_monitor_added() / signal_monitor_removed()`,
  client.cpp:235-238). Removal: `handleMonitorRemoved` defers via
  `Glib::signal_idle().connect_once(...)` (client.cpp:132-134, comment:
  "Defer destruction of bars for the output to the next iteration of the
  event loop") → `handleDeferredMonitorRemoval` does `(*it)->window.hide();
  gtk_app->remove_window((*it)->window); it = bars.erase(it);`
  (client.cpp:141-143), destroying the Bar and its modules. Re-add:
  `handleMonitorAdded` (client.cpp:119-123) registers the output, and on
  xdg_output `.done`, `handleOutputDone` runs
  `client->bars.emplace_back(std::make_unique<Bar>(&output, config));`
  (client.cpp:84) for each matching config — Bar construction runs
  `setupWidgets` → `getModules` → new CFFI → fresh `wbcffi_init`. Bar
  creation is gated on config match only: `isValidOutput` returns `true`
  when no `"output"` key exists (config.cpp:211); with a string/array
  `"output"`, it must equal the output name or identifier (config.cpp:
  179-208, `!`-negation and `*`-wildcard supported).
- Not found: any startup-only restriction; any path that reuses a previous
  Bar or module instance for a re-added output.
- Conclusion: resolved — waybar re-creates the bar and re-runs `wbcffi_init`
  automatically on re-add, provided a config block matches the re-added
  output's name/identifier (or the config is unpinned). The workload still
  should verify bar re-creation independently (surface count) so a
  convergence failure is attributed to the right layer, but the upstream
  mechanism is confirmed present.

#### How does the live deployment actually bind outputs — per-output `--output` flags, or flagless?

2026-07-22.

- Examined: `/home/chussenot/.config/waybar/config.jsonc` (read-only) — all
  three bars, all 20 `cffi/pwetty#N` exec lines, the bar-level `"output"`
  keys, and the config's own comments.
- Found: mixed wiring. Bar 2 (`"output": "HDMI-A-1"`): all 10 exec lines
  flagless (`claude-status tile-watch N`) — correct only because tile-watch's
  hardcoded default output is HDMI-A-1. Bar 3 (`"output": "eDP-1"`): all 10
  pass `--output eDP-1` before the index. Comment at config.jsonc:334-336
  explicitly warns: "Without it, tile-watch defaults to HDMI-A-1 and every
  tile reads idle" — the stranding was known and presumably hit.
- Not found: any unpinned (output-key-less) pwetty bar; any third output.
- Conclusion: resolved — see the Key observations bullet. Both failure
  shapes are deployment-real: bar-absent on connector rename (both bars
  output-pinned) and tile-stranding via flagless exec drift; harness models
  both.
