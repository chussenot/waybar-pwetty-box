# tile-watch-output-schema-valid

Focus: protocol contracts — the producer's documented guarantee that its output
"ALWAYS prints valid JSON" (tile.go), extended to the real cross-repo contract:
every emitted line should validate against `tiles/claude/schema.json`. No
runtime or CI schema validation exists anywhere today (sut-analysis §6: the
README's "validate" claim is overstated; `pwetty check` compares property NAMES
only) — this property IS the missing enforcement.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

Producer (`/home/chussenot/agentic-db/internal/tile/tile.go`):

- `tile.go:17` — "Like the hook, the CLI ALWAYS prints valid JSON and returns
  nil." Repeated at `tile.go:455` (Run) and `tile.go:498` (RunWatch: "on any
  read error it emits the empty placeholder").
- `tile.go:515-531` — `RunWatch.emit()`: cache read error → `emptyPayload`
  (`tile.go:93-99`: shortcut + one `idle` session at level 6) → `json.Marshal`
  → single `os.Stdout.Write` of the whole line. Marshal of `Payload` cannot
  fail (plain types), so the *syntactic* half of the claim looks robust — good;
  the property verifies it stays that way under faults.
- **Semantic hole #1 (found during this analysis):** `tile.go:57` — `App string
  \`json:"app,omitempty"\``. For a window with an empty niri `app_id`,
  `cleanAppLabel("")` returns `""` (`tile.go:376-388`) so `omitempty` drops the
  key, while `resolveAppIcon("")` returns `"app"` (`tile.go:245-249`) so
  `app_icon` is present. The schema (`tiles/claude/schema.json:9-24`) requires
  BOTH `app` and `app_icon` when `is_claude` is false → a schema-invalid payload
  from the untouched production code path. The template renders `{{ app }}`
  against an undefined variable as an **empty string** (minijinja 2.21.0 default
  `UndefinedBehavior::Lenient`; probed empirically at f87ec19 — see Investigation
  Log). `'/' in app_icon` on an undefined `app_icon` does not error either: it
  evaluates falsy, so the `name='{{ app_icon | default('app') }}'` branch fires
  and supplies the generic `app` icon. Net user-visible severity of hole #1:
  a normal-looking tile with a blank app label and the generic icon — silent
  degradation, never an error card.
- **Semantic hole #2:** `tile.go:120-121` — `SessionTile{State: s.State}` copies
  the DB string verbatim; `db.go:429-444` (`LoadLive`) performs no validation
  despite `state.go:29-31`'s "validated on read" comment. Any non-enum state in
  the DB flows into the payload → violates the closed `state` enum
  (`schema.json:54-56`). Full consequences in
  `unknown-session-state-renders-blank.md`.
- **Fault amplifier:** `tile.go:431-437` — `WriteCacheBytes` uses a FIXED temp
  name (`path + ".tmp"`). The daemon has no single-instance lock (checked
  `daemon.go` — the "no locks" at daemon.go:21 refers to the actor model). Two
  daemons can interleave `os.WriteFile` on the same tmp path and rename a torn
  file into place → `ReadCache` unmarshal error → `emptyPayload` substitution
  on every tile simultaneously. **Two-daemon realism settled 2026-07-22** (see
  Investigation Log): there is NO systemd unit for the daemon at all
  (`share/systemd/` holds only unrelated recap oneshot timers), no
  flock/pidfile/O_EXCL anywhere in the repo; the daemon is started by niri
  `spawn-at-startup` (README:147) and by `mise run install`'s "flip", whose
  old-daemon wait is a bounded busy-count (5000 `kill -0` iterations —
  microseconds, not a real timeout) killing only the FIRST matching pid.
  Nothing prevents a manual `claude-status daemon` from coexisting with the
  niri-spawned one — the scenario is legitimate, not contrived.

Consumer:

- `src/content.rs:126-130` — `parse_data` accepts anything (string fallback), so
  the plugin never *checks* the contract; violations surface only as downstream
  rendering artifacts. The contract is enforced by nothing on either side.

## Failure scenario

Under Antithesis faults (kill daemon mid-write, delete/corrupt `tiles.json`,
run a second daemon, fill the disk so `os.WriteFile` half-succeeds):

1. Syntactic violation (line is not one complete JSON doc) — would mean the
   single-Write/Marshal reasoning above has a hole (e.g. a partial write on a
   full pipe, an unexpected Marshal error path). Catching one is a real finding.
2. Semantic violation (line is JSON but fails schema validation) — reachable
   today via hole #1 (empty app_id) and hole #2 (non-enum state); each renders
   as silent degradation, never as a visible error.
3. Masking (line is schema-VALID but fabricated): the `emptyPayload`
   substitution replaces whatever was true — including a live `prompt` — with a
   plausible "long idle". Schema validity is necessary, not sufficient; the
   substitution event itself must be observable or S2 staleness hides inside a
   "passing" contract.

## Suggested assertions (net-new)

- Workload `Always`, wrapping the tile-watch pipe with a validating tee: message
  **"tile-watch emitted one complete JSON document per line"** (syntactic).
- Workload `Always`, same tee, using a compiled `tiles/claude/schema.json`
  validator: message **"tile-watch line validates against the claude tile
  schema"** (semantic). Expected to fail once Antithesis finds an empty-app_id
  window or a dirty DB state — both first-run findings.
- SUT-side Go `Sometimes` in `RunWatch.emit()` on the `ReadCache` error branch
  (`tile.go:517-521`): message **"tile-watch substituted the empty placeholder
  for an unreadable cache"** — makes masking events visible in timelines and
  correlatable with prompt-loss findings (cross-ref: the staleness-focused
  properties from other ensemble members).

## Key observations

- This is the cheapest possible contract enforcement to add (a tee + validator
  in the workload, zero SUT changes for the Always half), and it converts three
  currently-silent degradations into distinct, attributable failures.
- The fixed `.tmp` name is a one-line fix (`os.CreateTemp`, already used
  correctly 20 lines up in `wrapPNGAsSVG`, tile.go:341) — evidence the authors
  know the pattern; the cache write predates it.

## Open questions

None remaining (see Investigation Log).

### Investigation Log

#### Is the two-daemon scenario legitimate or contrived?

2026-07-22:

- Examined: `/home/chussenot/agentic-db/share/systemd/` (all four units),
  README ("Build & install", daemon sections), `mise.toml` `[tasks.install]`,
  repo-wide grep for `flock|pidfile|LOCK_EX|O_EXCL|single-instance` across
  `internal/` and `main.go`.
- Found: the presumed daemon systemd unit **does not exist** —
  `share/systemd/` contains only `claude-daily-recap` / `claude-weekly-recap`
  oneshot timers (unrelated to the daemon). The daemon is launched two ways:
  niri `spawn-at-startup` (README:147) and `mise run install`'s flip, which
  (a) finds a running daemon by scanning `/proc/*/cmdline`, (b) kills only the
  FIRST match, and (c) "waits" via a bounded busy-count — `while kill -0 pid
  && i < 5000; do i=i+1; done` — a loop of ~5000 no-sleep iterations that
  expires in microseconds–milliseconds, then starts the new daemon regardless
  (its own comment concedes the overlap: "so they don't briefly both
  reconcile" is best-effort). No lock, pidfile, bus name, or exclusive-open
  guard anywhere in the codebase.
- Not found: any mechanism, in code or deployment, that prevents two
  concurrent daemons.
- Conclusion: resolved — legitimate. Realistic routes: a manual
  `claude-status daemon` beside the niri-spawned one; the install flip racing
  a slow-exiting old daemon; two niri sessions. The torn-cache fault amplifier
  needs no Antithesis bypass — the workload can simply start a second daemon.

#### What does minijinja render for `{{ app }}` when `app` is undefined — empty string or a template error card? (and `'/' in app_icon` on undefined `app_icon`)

2026-07-22:

- Examined: minijinja 2.21.0 vendored source (`src/utils.rs` — `UndefinedBehavior`
  enum, `#[default] Lenient`; Lenient: printing undefined → empty string);
  `src/markup.rs:109-117` (`render_template` uses `Environment::new()` and never
  calls `set_undefined_behavior`, so the Lenient default applies).
- Probed: throwaway crate pinning `minijinja = "=2.21.0"` replicating
  `render_template` exactly, run against the verbatim `tiles/claude/tile.json`
  template; plus the real compose path via
  `pwetty render claude --data -` (offscreen, PNG inspected).
  - `[{{ app }}]` with `app` undefined → `"[]"` (empty string, no error).
  - `{% if '/' in app_icon %}` with `app_icon` undefined → no error, falsy →
    else branch → `{{ app_icon | default('app') }}` → `app`.
  - Full template with `{"is_claude": false, "shortcut": "3", "title": "Some
    Window"}` (no `app`, no `app_icon`) → renders cleanly: empty app label,
    `<icon name='app'/>`. PNG shows a normal card (shortcut + title + generic
    icon), no error card.
- Not found: nothing missing — behavior fully settled empirically.
- Conclusion: resolved. Hole #1's user-visible severity is a blank app label +
  generic icon (silent degradation), NOT a red error card. Body updated; the
  property's invariant and assertion types are unchanged (the schema-validation
  `Always` is still the enforcement; this only pins the severity framing).

#### Can niri report an empty `app_id` in practice?

Investigated 2026-07-22 against niri `main` (github.com/YaLTeR/niri, fetched
2026-07-22) and docs.rs niri-ipc latest.

- Examined: docs.rs `niri_ipc::Window` field docs; niri `src/ipc/server.rs`
  `make_ipc_window` (where the IPC Window is built); consumer side
  `internal/niri/windows.go:27` (`AppID string \`json:"app_id"\``).
- Found: YES. `Window.app_id` is `Option<String>` ("Application ID, if set")
  and is populated directly from the xdg-shell toplevel role state:
  `app_id: role.app_id.clone()` in `make_ipc_window` — i.e. it is `None`
  (JSON `null`) for any toplevel whose client never called
  `xdg_toplevel.set_app_id`, which the protocol makes optional. Go's decoder
  leaves the zero value on JSON null, so `niri.Window.AppID == ""` — feeding
  `cleanAppLabel("")`/`resolveAppIcon("")` exactly as hole #1 describes. This
  is not confined to exotic clients: any minimal/misbehaving Wayland client
  (simple SDL apps, test clients) can map a toplevel without an app_id, and
  the value can also legitimately be the empty string if a client sets "".
- Not found: any niri-side defaulting or synthesis of a missing app_id (no
  fallback to WM_CLASS inside niri itself; xwayland goes through
  xwayland-satellite, which presents ordinary Wayland toplevels — same
  optionality).
- Conclusion: RESOLVED — the trigger is real, not synthesis-only. The
  workload can exercise hole #1 with a genuine client that never sets app_id
  (or by fake cache entry, still the cheaper route); the schema-validation
  `Always` is expected to fail on it at f87ec19. Property unchanged otherwise.
