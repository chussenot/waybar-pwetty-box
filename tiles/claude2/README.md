# `claude2` tile

One niri desktop running **two** Claude sessions, stacked. The shared desktop
shortcut sits in a big left gutter; each session gets a block to its right:
status indicator + folder + unpushed badge, then its window title.

```
┌────────────────────────────────┐
│      ⬛ api               ↑3    │  session A: status + folder + ↑commits
│ 5    refactor the inline flow…  │            title (tickers if long)
│      ?  worker                  │  session B
│      run: git push origin main? │
└────────────────────────────────┘
```

For a single-session desktop use the [`claude`](../claude/) tile; for two, this.

## Using it

```jsonc
"cffi/pwetty#claude5": {
  "module_path": ".../libpwetty_box.so",
  "tile": "claude2",
  "stream": true,
  "exec": "claude-status tile-watch 5"   // emit the JSON below
}
```

**Bar height.** This tile is two sessions tall — it wants a **~78px** bar (vs the
~56px a single-line bar needs). Bar height is shared, so bumping it lifts every
tile on that bar; single-session tiles just gain a little breathing room. Set it
once on the waybar bar (`"height": 78`).

## The data contract

`exec` stdout (or static `text`) is a JSON object matching
[`schema.json`](./schema.json):

| field      | type            | source | notes |
|------------|-----------------|--------|-------|
| `shortcut` | integer/string  | MOCK   | desktop number, shown big in the shared left gutter |
| `active`   | boolean         | niri   | focused desktop → accent card |
| `sessions` | array (1–2)     | —      | one block per session (below) |

Each `sessions[]` entry (mirrors the single `claude` tile, minus `shortcut`):

| field        | type        | source       | notes |
|--------------|-------------|--------------|-------|
| `state`      | enum        | REAL         | `working` \| `prompt` \| `idle` \| `shell` — drives the indicator |
| `idle_level` | integer 0–6 | REAL-derived | when `state=idle`: bright (0) → dim (6) |
| `idle_ago`   | string      | REAL-derived | when `state=idle`: e.g. `12m`, beside the bar |
| `folder`     | string      | REAL         | basename of the session `cwd` |
| `title`      | string      | MOCK         | window title; on its own line (tickers if long) |
| `unpushed`   | integer     | MOCK         | `↑N` after the folder; hidden when 0 or idle |

Indicators match the `claude` tile: `working`→orange Claude mascot, `shell`→
electric-cyan mascot, `idle`→fade bar + `idle_ago`, `prompt`→blinking `?`. If
**either** session is `prompt`, the **whole tile pulses** (one attention signal
for the desktop).

## Inspecting / previewing

```bash
pwetty schema claude2
pwetty check claude2
pwetty render claude2 --all-states -o /tmp/claude2     # PNGs of the samples
echo '{"shortcut":5,"sessions":[{"state":"working","folder":"api"},{"state":"prompt","folder":"worker"}]}' \
  | pwetty render claude2 --data - -o /tmp/claude2
```

Samples in [`samples/`](./samples/): `duo` (working + prompt, active), `mixed`
(shell + idle), `single` (one session — degrades to a single block).
