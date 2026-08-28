# `quivive` tile

The fleet-presence status bar for every repository [quivive](https://github.com/chussenot/quivive)
watches. **The tile is data-driven** — waybar always references `quivive`; the
template renders from what you send, so you never tell waybar how many repos
are in the registry:

- **header** — the overall `status` (one of quivive's five: `human-needed`,
  `active`, `drained`, `all-quiet`, `no-fleet`) drives the accent icon and
  color. `human-needed` makes the **whole tile pulse** (one attention signal,
  same `<pulse>` pattern the bundled `claude` tile uses for a blinking prompt).
- **a row per repo** — name, then agent counts in the four-state machine
  (`ACTIVE`/`IDLE`/`STALE`/`DEAD`), one letter-coded, color-coded count each —
  `2A 1I 0S 0D`.
- **an attention line per attention item** — only present when a repo carries
  one; a dead-holding-paths, needs-decision, or gate-order-violation item,
  worded from its own fields. No line at all when nothing needs a look.

```
all-quiet / active                    human-needed (pulsing)
┌──────────────────────────┐         ┌──────────────────────────┐
│ ⚙ active                 │         │ 🔔 human-needed           │
│ quivive   2A 1I 0S 0D    │         │ quivive   1A 0I 1S 1D     │
│ pact      0A 0I 0S 0D    │         │ pact      1A 0I 0S 0D     │
│ recount   0A 0I 0S 0D    │         │ ⚠ quivive: dead agent-6…  │
└──────────────────────────┘         │ ⚠ quivive: needs decis…  │
                                      │ ⚠ quivive: gate violat…  │
                                      └──────────────────────────┘
```

## Using it

```jsonc
"cffi/pwetty#quivive": {
  "module_path": ".../libpwetty_box.so",
  "tile": "quivive",
  "stream": true,
  "exec": "quivive tile --stream"    // emit the JSON below, one line per change
}
```

`quivive tile` (no `--stream`) prints the same shape once for a one-shot
module (`interval`-polled instead of pushed).

## The data contract

`exec` stdout (or static `text`) is a JSON object matching
[`schema.json`](./schema.json).

| field    | type   | source | notes |
|----------|--------|--------|-------|
| `v`      | int    | REAL   | tile contract version |
| `status` | enum   | REAL   | overall status, same five values and precedence as each repo's own `status` below, aggregated across every watched repo |
| `repos`  | array  | REAL   | one entry per repo quivive's tick read this cycle |

Each `repos[]` entry:

| field       | type   | source | notes |
|-------------|--------|--------|-------|
| `name`      | string | MOCK   | short label for the row. quivive's tick currently identifies a repo by its absolute registry path, not a short name — see [Divergence note](#divergence-note) below |
| `status`    | enum   | REAL   | `human-needed` \| `active` \| `drained` \| `all-quiet` \| `no-fleet` — this repo's own derived status |
| `agents`    | object | REAL   | `{active, idle, stale, dead}` counts, always all four keys, zeros included |
| `attention` | array  | REAL   | this repo's attention items, sorted, deterministic; empty when not `human-needed` |

Each `attention[]` item is a tagged object (`kind` selects the shape), mirroring
quivive's `AttentionItem` exactly:

| `kind`                 | fields | source | notes |
|-------------------------|--------|--------|-------|
| `dead_holding_paths`    | `agent`, `paths` (sorted), `remaining_ttl` | REAL | one item per dead-or-unknown lease holder, not one per lease; `remaining_ttl` is the minimum remaining TTL across that holder's leases, clamped at 0 |
| `needs_decision`        | `bead_id` | REAL | a bead filed as needing a decision, from the committed sidecar, verbatim |
| `gate_order_violation`  | `started_id`, `started_wave`, `open_gate_id`, `gate_wave` | REAL | only the earliest open gate blocking `started_id` is reported |

Indicators: `human-needed`→red bell (pulsing), `active`→blue gear,
`drained`→green check, `all-quiet`→dim lavender folder, `no-fleet`→very dim
app glyph. Per-row counts are always color-coded the same way regardless of
the row's own status: active green, idle blue, stale yellow, dead red — so a
`STALE`/`DEAD` count reads as a warning even in a row whose own `status` isn't
`human-needed` yet.

### Divergence note

`repos[].name` is the one field this tile invents rather than derives:
quivive's own state seam (`src/state.rs`) and `docs/tile-contract.md` (as of
this tile's authoring) key a repo by its absolute registry path, not a short
display name. Every sample here uses an invented short name (`quivive`,
`pact`, `recount`, …) purely to keep rows narrow and readable. Whichever key
quivive's real `tile` command ends up emitting — a short name, a path
basename, or the full path — the template only ever prints whatever string
arrives in `name`; nothing here hardcodes a transform. The golden tests
(`quivive-jx3`, both repos) are where quivive's real emitted bytes and this
tile's samples get reconciled.

## Inspecting / previewing

```bash
pwetty schema quivive
pwetty check quivive
pwetty render quivive --all-states -o /tmp/quivive        # PNGs of every sample
echo '{"v":1,"status":"active","repos":[{"name":"quivive","status":"active","agents":{"active":2,"idle":1,"stale":0,"dead":0},"attention":[]}]}' \
  | pwetty render quivive --data - -o /tmp/quivive
```

Samples in [`samples/`](./samples/): `all-quiet`, `active`, `human-needed`,
`drained`, `no-fleet` — exactly S13's list, and the same names both repos'
golden tests share.
