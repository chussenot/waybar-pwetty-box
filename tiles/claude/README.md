# `claude` tile

A wide signage tile representing one niri desktop running a Claude session:
shortcut number, an animated session-status indicator, the folder name, an
unpushed-commits badge, and a scrolling window-title marquee.

```
┌──────────────────────────────────────────┐
│ 5  ●  pwetty-box            ↑3            │   line 1: shortcut + status + folder + ↑commits
│ ⟨Harder Better Faster Stronger · refac…⟩  │   line 2: title marquee (loops)
└──────────────────────────────────────────┘
```

## Using it

The preset ships *inside* the module. A waybar module references it by name and
adds only the data source — pwetty merges the preset underneath, and the
module's own keys win:

```jsonc
"cffi/pwetty#claude5": {
  "module_path": ".../libpwetty_box.so",
  "tile": "claude",          // <- this preset (geometry, fonts, template)
  "interval": 2,
  "exec": "claude-tile-data 5"   // <- your job: emit the JSON below on stdout
}
```

Override any preset field inline (e.g. `"width": 360`). To iterate on the visual
without rebuilding, point at a file instead: `"tile_file": "/path/tile.json"`.

## The data contract

The `exec` stdout (or static `text`) must be a JSON object matching
[`schema.json`](./schema.json). Fields:

| field        | type            | source | notes |
|--------------|-----------------|--------|-------|
| `shortcut`   | integer/string  | MOCK   | desktop shortcut number, shown first |
| `state`      | enum            | REAL   | `working` \| `prompt` \| `idle` \| `shell` — drives the indicator |
| `idle_level` | integer 0–6     | REAL-derived | only used when `state=idle`; 0=just-idled (white) → 6=>60min (dim) |
| `folder`     | string          | REAL   | basename of the session `cwd` |
| `title`      | string          | MOCK   | window title; scrolls as a marquee |
| `unpushed`   | integer         | MOCK   | unpushed commit count; shown as `↑N` in the marquee, hidden when 0 |
| `idle_ago`   | string          | REAL-derived | when `state=idle`: "time since active", e.g. `12m` (shown as `12m ago`) |
| `active`     | boolean         | niri   | focused desktop → an accent "card" (fill + border) so it stands out |
| `is_claude`  | boolean         | derive | `true` → the status/folder layout; `false` → the app-icon layout (below) |
| `app`        | string          | window | `is_claude=false` only: the app/window label |
| `app_icon`   | string          | window | `is_claude=false` only: a bundled icon name or an absolute `.svg` path |

REAL = available from `~/Perso/claude-status-db` (sessions row). MOCK = not a
session field yet; synthesize it. `state` values are the daemon's own strings.

**Two layouts.** A claude desktop (`is_claude=true`, the default) shows the
status indicator + folder (+ `idle_ago` when idle). A plain desktop
(`is_claude=false`) shows the leftmost window's **app icon** + `app` label
instead — `app_icon` is a bundled icon name (e.g. `code`) or a path to any
`.svg` (e.g. a freedesktop app icon). Both layouts share the line-2 title marquee.

The indicator: `working` → blinking orange ●, `prompt` → blinking yellow `?`,
`shell` → pulsing cyan ●, `idle` → a static two-cell bar that fades white→grey
with `idle_level`.

## Inspecting / previewing

```bash
pwetty list                 # bundled tiles
pwetty schema claude        # print this tile's JSON Schema (the contract)
pwetty check claude         # validate template ↔ schema ↔ samples
pwetty render claude --all-states -o /tmp/claude   # PNGs of every bundled sample
```

Sample payloads live in [`samples/`](./samples/) — one per state, used by
`pwetty check`/`render` and the test suite.
