# `claude` tile

One niri desktop, rendered from its data. **The tile is data-driven** — waybar
always references `claude`; the template decides the layout from what you send,
so you never have to tell waybar what a desktop holds.

Send the desktop's windows as `items[]` **rows** (max 2). A row is either a
**session row** (`state` — a Claude session) or an **app row** (`app` +
`app_icon` — an ordinary window); mix them freely.

- **1 row** → the rich single layout: big shortcut, the row's indicator (status
  mascot, or the app's inline icon), its label, `↑unpushed`, and a wrapped title.
- **2 rows** → the stacked dual layout: the shared shortcut in a big left gutter,
  each row a block (indicator + label + `↑N`, then its title, tickered).
- **any mix** → one session next to one plain app renders in that same dual
  geometry. This is what `items[]` buys over the old shapes.

```
single                          dual (two rows: sessions, apps, or a mix)
┌────────────────────────┐     ┌────────────────────────────┐
│ 5  ⬛ pwetty-box   ↑3   │     │      ⬛ api            ↑3   │
│ refactor the inline…   │     │ 5    refactor the flow…    │
│                        │     │      ⬛ VS Code             │
└────────────────────────┘     │      lib.rs — pwetty-box…  │
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

**Bar height.** A two-row desktop is taller than a one-row one, and a
waybar bar is a single shared height — so size the bar for the dual case
(**~96px** at the default font; the preset's own default). One-row tiles simply
use the extra room. Set `"height"` once on the bar.

## The data contract

`exec` stdout (or static `text`) is a JSON object matching
[`schema.json`](./schema.json).

| field       | type        | source | notes |
|-------------|-------------|--------|-------|
| `shortcut`  | int/string  | MOCK   | desktop number, shown first (gutter when two rows) |
| `active`    | boolean     | niri   | focused desktop → accent card |
| `items`     | array (1–2) | —      | the desktop's rows; **this is the shape to send** |
| `is_claude` | boolean     | —      | legacy, **accepted and ignored** — rows are self-describing now |
| `sessions`  | array (1–2) | —      | legacy: read as `items` session rows |
| `app`       | string      | window | legacy flat window (no `items`/`sessions`): app/window label |
| `app_icon`  | string      | window | legacy flat window: bundled icon name or absolute `.svg` path |
| `title`     | string      | window | legacy flat window: window title |

Each `items[]` row:

| field        | type        | source       | notes |
|--------------|-------------|--------------|-------|
| `kind`       | enum        | derive       | `session` \| `app` — optional; inferred from `state` vs `app` |
| `state`      | enum        | REAL         | session rows, required: `working` \| `prompt` \| `idle` \| `shell` — drives the indicator |
| `folder`     | string      | REAL         | session rows: basename of the session `cwd` |
| `unpushed`   | integer     | MOCK         | session rows: `↑N` after the folder; hidden when 0 or idle |
| `idle_level` | integer 0–6 | REAL-derived | session rows, when `state=idle`: bright (0) → dim (6) |
| `idle_ago`   | string      | REAL-derived | session rows, when `state=idle`: e.g. `12m`, beside the bar |
| `app`        | string      | window       | app rows, required: the app/window label |
| `app_icon`   | string      | window       | app rows, required: bundled icon name (e.g. `code`) or absolute `.svg` path |
| `title`      | string      | MOCK         | either kind: window title (wrapped with one row, tickered with two) |

Indicators: `working`→deep-orange Claude mascot, `shell`→electric-cyan mascot,
`idle`→fade bar + `idle_ago`, `prompt`→blinking `?`, an app row→its own icon
inline. If **any** session row is `prompt`, the **whole tile pulses** (one
attention signal per desktop).

### Background agents: the bead strip

A row can stand for a set of **background agents** instead of one conversation,
by carrying a `fleet` object. Two desktops get one: the one running the
`claude agents` view (`scope: "whole"` — the view is machine-wide, so it shows
every agent), and one whose session spawned agents (`scope: "owned"`).

```
single (a console desktop)              dual (a conversation + its agents)
┌──────────────────────────────┐       ┌────────────────────────────┐
│ 5  ?  agents ⚠3 10/13        │       │      ▤▤ project-manager 4h │
│ ▮▮▮ ▮▮▮▮ ▮ ▮▮ ▮▮▮            │       │ 4    adopt-worktrees…      │
│ project-manager-integrator:  │       │      project-man 6/6       │
│ answer: Chamber 7 handed of… │       │      ▮▮▮▮ ▮▮               │
└──────────────────────────────┘       └────────────────────────────┘
```

One bead per agent, in `demands` order (already worst-first), grouped where the
demand changes:

| demand | bead | means |
|---|---|---|
| `awaits-human` | yellow | waiting on YOU — also makes the row `prompt`, so the tile pulses |
| `working` | peach | making progress |
| `blocked` | sapphire | stopped, but on another agent — not on you |
| `idle` | grey | alive, at rest |
| `done` | dark grey | finished |

Past 13 beads it draws 13 and a `+N`, rather than shrinking them below reading
size.

**The row is still a session row.** `state` is required and still drives the
indicator and the tile-wide pulse, so a template that knows nothing about
`fleet` renders it as an ordinary session — which is the whole reason the
producer can ship the field before a renderer supports it. There is deliberately
no `kind: "fleet"`: `kind` is a closed enum and rows are discriminated by
`state`.

**Two things the template has to defend against**, both from real data:

- `needs` arrives **unclipped** — one live value was ~600 characters, an entire
  AskUserQuestion with its options. The producer sends full fidelity on purpose
  and leaves clipping to the consumer, so the template clips; a wrapped 600-char
  ask would grow past the tile's fixed height and get cut mid-word.
- `neediest` is not always a short handle. An agent spawned without `--name` is
  titled from its intent — a real one reads `Review baroque database replica
  configuration` — so the dual layout clips the label too, and clips the FOLDER
  rather than the counts, because a cut-off `6/` is worse than a shortened name.

> **Migration note.** The contract moved from flat per-session fields to a
> `sessions` array. A single session is now `"sessions": [ { … } ]` (was the
> fields at top level). Window payloads are unchanged.
>
> Then `sessions[]` and the flat window fields were unified into one `items[]`
> array of rows, so a desktop holding two apps — or a session *and* an app —
> is finally expressible; it was not before, because `is_claude` forced the
> payload to be all-session or all-window. Both older shapes still validate and
> render: `sessions: [ … ]` is read as session rows, and the flat
> `app`/`app_icon`/`title` trio as a single app row. `is_claude` survives only as
> an ignored field — it no longer picks a branch. New feeds should send `items[]`.

## Inspecting / previewing

```bash
pwetty schema claude
pwetty check claude
pwetty render claude --all-states -o /tmp/claude        # PNGs of every sample
echo '{"shortcut":5,"items":[{"state":"working","folder":"api"}]}' \
  | pwetty render claude --data - -o /tmp/claude
```

Samples in [`samples/`](./samples/): `working`, `prompt`, `idle`, `shell` (single
sessions), `duo` (two sessions), `window` (a plain window), `duo-window` (two app
rows — one bundled icon name, one absolute `.svg` path), `mixed` (a session row
plus an app row).
