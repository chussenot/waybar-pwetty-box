# `claude` tile

One niri desktop, rendered from its data. **The tile is data-driven** — waybar
always references `claude`; the template decides the layout from what you send,
so you never have to tell waybar what is on a desktop.

A desktop is **at most two rows**, and each row is either a **Claude session**
(status indicator + folder + `↑unpushed`) or an **ordinary window** (app icon +
name). Any combination:

- **1 row** → the rich single layout: big shortcut, then the row, then a wrapped
  title. A lone window row draws its icon as a background watermark.
- **2 rows** → a stacked dual layout: the shared shortcut in a big left gutter,
  each row a block, its title tickered.
- **one of each** → the same dual layout. A desktop holding a Claude session
  *and* a browser is an ordinary thing, and it is the shape the previous
  contract could not express at all: `is_claude` was a single per-desktop flag,
  so a desktop was all-sessions or all-window and nothing in between.

```
single                          dual
┌────────────────────────┐     ┌────────────────────────────┐
│ 5  ⬛ pwetty-box   ↑3   │     │      ⬛ api            ↑3   │
│ refactor the inline…   │     │ 5    refactor the flow…     │
│                        │     │      ?  worker              │
└────────────────────────┘     │      run: git push main?    │
                               └────────────────────────────┘

mixed (one session + one window)
┌────────────────────────────┐
│      ⬛ pwetty-box     ↑2   │
│ 4    port items[] from…    │
│      ▪ Firefox             │
│      pull request #1       │
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

**Bar height.** A two-row desktop is taller than a one-row one, and a waybar bar
is a single shared height — so size the bar for the two-row case (**~96px** at
the default font; the preset's own default). One-row tiles simply use the extra
room. Set `"height"` once on the bar.

## The data contract

`exec` stdout (or static `text`) is a JSON object matching
[`schema.json`](./schema.json).

| field      | type        | source | notes |
|------------|-------------|--------|-------|
| `shortcut` | int/string  | MOCK   | desktop number, shown first (gutter when two rows) |
| `active`   | boolean     | niri   | focused desktop → accent card |
| `items`    | array (1–2) | —      | **the rows.** Each is a session or a window; see below |

Each `items[]` entry is one of two shapes. `kind` (`"session"` / `"app"`) states
which; it may be omitted, in which case a row carrying `state` is a session and
anything else is a window.

**A session row:**

| field        | type        | source       | notes |
|--------------|-------------|--------------|-------|
| `state`      | enum        | REAL         | `working` \| `prompt` \| `idle` \| `shell` \| `empty` — drives the indicator |
| `folder`     | string      | REAL         | basename of the session `cwd` |
| `title`      | string      | MOCK         | window title (wrapped when the only row, tickered when there are two) |
| `unpushed`   | integer     | MOCK         | `↑N` after the folder; hidden when 0 or idle |
| `idle_level` | integer 0–6 | REAL-derived | when `state=idle`: bright (0) → dim (6) |
| `idle_ago`   | string      | REAL-derived | when `state=idle`: e.g. `12m`, beside the bar |

**A window row:**

| field      | type   | source | notes |
|------------|--------|--------|-------|
| `app`      | string | window | the app/window label |
| `app_icon` | string | window | bundled icon name or absolute `.svg` path; omitted → the generic `app` glyph |
| `title`    | string | window | the window title |

### Legacy shapes, still rendered

Nothing has to move. Two older forms are folded into `items[]` by the template,
and both render byte-identically to what they rendered before:

| Sending | Read as |
|---|---|
| `sessions: [...]` | `items` where every row is a session |
| flat `app` / `app_icon` / `title` | `items` with a single window row |

`is_claude` is **ignored**. The row's own shape decides how it draws, so the
flag no longer selects a layout; it is harmless to keep sending. It is also why
the mixed desktop was previously unrepresentable — one flag per desktop cannot
describe two rows that disagree.

Indicators: `working`→deep-orange Claude mascot, `shell`→electric-cyan mascot,
`idle`→fade bar + `idle_ago`, `prompt`→blinking `?`, `empty`→no indicator at all
(bare shortcut — no session and no window on the desktop). If **any** row is
`prompt`, the **whole tile pulses** (one attention signal per desktop).

> **Migration note.** `items[]` supersedes `sessions[]`, which itself superseded
> flat per-session fields. Both older shapes still render, unchanged and
> pixel-for-pixel identical — moving is worth it only when you need a window and
> a session on one desktop, which is the case neither older shape can express.

## Inspecting / previewing

```bash
pwetty schema claude
pwetty check claude
pwetty render claude --all-states -o /tmp/claude        # PNGs of every sample
echo '{"shortcut":5,"items":[{"kind":"session","state":"working","folder":"api"}]}' \
  | pwetty render claude --data - -o /tmp/claude
```

Samples in [`samples/`](./samples/): `working`, `prompt`, `idle`, `shell`,
`empty` (one session row each), `duo` (two sessions), `window` (one window, flat
legacy form), `mixed` (a session and a window), `duo-window` (two windows).
