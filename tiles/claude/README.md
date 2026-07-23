# `claude` tile

One niri desktop, rendered from its data. **The tile is data-driven** — waybar
always references `claude`; the template decides the layout from what you send,
so you never have to tell waybar how many sessions a desktop has:

- **1 session** → the rich single layout: big shortcut, status indicator, folder,
  `↑unpushed`, and a wrapped title.
- **2 sessions** → a stacked dual layout: the shared shortcut in a big left
  gutter, each session a block (status + folder + `↑N`, then its title, tickered).
- **a plain window** (`is_claude:false`) → the app's icon + name.

```
single                          dual
┌────────────────────────┐     ┌────────────────────────────┐
│ 5  ⬛ pwetty-box   ↑3   │     │      ⬛ api            ↑3   │
│ refactor the inline…   │     │ 5    refactor the flow…     │
│                        │     │      ?  worker              │
└────────────────────────┘     │      run: git push main?    │
                               └────────────────────────────┘
```

## Using it

```jsonc
"cffi/pwetty#5": {
  "module_path": ".../libpwetty_box.so",
  "tile": "claude",
  "stream": true,
  "exec": "claude-status tile-watch 5"   // emit the JSON below
}
```

**Bar height.** A two-session desktop is taller than a one-session one, and a
waybar bar is a single shared height — so size the bar for the dual case
(**~96px** at the default font; the preset's own default). One-session and window
tiles simply use the extra room. Set `"height"` once on the bar.

## The data contract

`exec` stdout (or static `text`) is a JSON object matching
[`schema.json`](./schema.json).

| field       | type        | source | notes |
|-------------|-------------|--------|-------|
| `shortcut`  | int/string  | MOCK   | desktop number, shown first (gutter when dual) |
| `active`    | boolean     | niri   | focused desktop → accent card |
| `is_claude` | boolean     | derive | omit/`true` → a Claude desktop (`sessions`); `false` → a window (`app`/`app_icon`) |
| `sessions`  | array (1–2) | —      | the session(s); **required for a Claude desktop** |
| `app`       | string      | window | `is_claude:false` only: app/window label |
| `app_icon`  | string      | window | `is_claude:false` only: bundled icon name or absolute `.svg` path |
| `title`     | string      | window | `is_claude:false` only: window title |

Each `sessions[]` entry:

| field        | type        | source       | notes |
|--------------|-------------|--------------|-------|
| `state`      | enum        | REAL         | `working` \| `prompt` \| `idle` \| `shell` \| `empty` — drives the indicator |
| `folder`     | string      | REAL         | basename of the session `cwd` |
| `title`      | string      | MOCK         | window title (wrapped when single, tickered when dual) |
| `unpushed`   | integer     | MOCK         | `↑N` after the folder; hidden when 0 or idle |
| `idle_level` | integer 0–6 | REAL-derived | when `state=idle`: bright (0) → dim (6) |
| `idle_ago`   | string      | REAL-derived | when `state=idle`: e.g. `12m`, beside the bar |

Indicators: `working`→deep-orange Claude mascot, `shell`→electric-cyan mascot,
`idle`→fade bar + `idle_ago`, `prompt`→blinking `?`, `empty`→dim hollow ring (no
session and no window on the desktop). If **any** session is `prompt`, the
**whole tile pulses** (one attention signal per desktop).

> **Migration note.** The contract moved from flat per-session fields to a
> `sessions` array. A single session is now `"sessions": [ { … } ]` (was the
> fields at top level). Window payloads are unchanged.

## Inspecting / previewing

```bash
pwetty schema claude
pwetty check claude
pwetty render claude --all-states -o /tmp/claude        # PNGs of every sample
echo '{"shortcut":5,"sessions":[{"state":"working","folder":"api"}]}' \
  | pwetty render claude --data - -o /tmp/claude
```

Samples in [`samples/`](./samples/): `working`, `prompt`, `idle`, `shell`, `empty`
(single sessions), `duo` (two sessions), `window` (a plain window).
