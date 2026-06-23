# `claude` tile

A tall signage tile representing one niri desktop running a Claude session:
shortcut number, an animated session-status indicator (the pixel-art Claude
mascot), the folder name, an unpushed-commits badge, and the word-wrapped
window title.

```
┌──────────────────────────────────────────┐
│ 5  ⬛  pwetty-box            ↑3           │   line 1: shortcut + status + folder + ↑commits
│ refactor the inline flow layout and        │   line 2: title, word-wrapped (static,
│ add wrapped-title support                  │            as many lines as it needs)
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
| `title`      | string          | MOCK   | window title; word-wrapped on line 2 (static) |
| `unpushed`   | integer         | MOCK   | unpushed commit count; shown as `↑N` after the folder, hidden when 0 or when idle (idle shows `idle_ago` instead) |
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
`.svg` (e.g. a freedesktop app icon). Both layouts share the line-2 word-wrapped title.

The indicator: `working` → the **Claude mascot** in deep orange, slow blink +
color-matched glow; `prompt` → a blinking yellow `?` (and the *whole tile*
pulses, to pull your eye); `shell` → the **Claude mascot** in electric cyan,
pulsing + glow; `idle` → a static two-cell bar that fades white→grey with
`idle_level`. (Idle/empty tiles are fully static — they don't repaint, keeping
the bar cool; only `working`/`prompt`/`shell` animate.)

## Inspecting / previewing

```bash
pwetty list                 # bundled tiles
pwetty schema claude        # print this tile's JSON Schema (the contract)
pwetty check claude         # validate template ↔ schema ↔ samples
pwetty render claude --all-states -o /tmp/claude   # PNGs of every bundled sample

# ↓ the loop for building the data: render YOUR OWN json and eyeball it
echo '{"shortcut":5,"state":"working","folder":"api","title":"…","unpushed":2}' \
  | pwetty render claude --data - -o /tmp/claude
pwetty render claude --data ./my-payload.json -o /tmp/claude
```

`--data` is the key tool for the data layer: emit the JSON you plan to feed via
`exec`, pipe it through `render --data`, and confirm the tile looks right before
wiring it into waybar. Required fields depend on the layout (the schema's
`if/then`): a Claude tile needs `state`; a window tile (`is_claude:false`) needs
`app_icon`+`app`; only `shortcut` is always required.

Sample payloads live in [`samples/`](./samples/) — one per state, used by
`pwetty check`/`render` and the test suite.
