---
title: Bundled tiles and the tile gate
status: active
date: 2026-08-29
---

# Bundled tiles and the tile gate

Whole tiles ship **inside the module**: geometry, fonts, colours, and the
`format` template, packaged as a named preset. A waybar module references one by
name and supplies only the data — pwetty layers the preset underneath, and the
module's own keys win:

```jsonc
"cffi/pwetty#claude5": {
  "module_path": ".../libpwetty_box.so",
  "tile": "claude",                 // bundled preset (geometry + template)
  "interval": 2,
  "exec": "claude-tile-data 5"      // your job: emit JSON matching the contract
}
```

Override any preset field inline (`"width": 360`), or point at an external file
to iterate without rebuilding (`"tile_file": "/path/tile.json"`).

## What ships

| Tile | For | Contract |
|---|---|---|
| [`claude`](../tiles/claude/README.md) | a niri desktop running Claude session(s), or holding a plain window | [`schema.json`](../tiles/claude/schema.json) |
| [`empty`](../tiles/empty/README.md) | a compact, narrow tile for a windowless desktop | [`schema.json`](../tiles/empty/schema.json) |
| [`quivive`](../tiles/quivive/README.md) | fleet presence across the repositories [quivive](https://github.com/chussenot/quivive) watches | [`schema.json`](../tiles/quivive/schema.json) |

Each tile's own README is the binding contract and is what `pwetty` prints — it
is embedded into the `.so` with `include_str!` and surfaced verbatim, which is
why those READMEs carry no front matter (see
[documentation.md](documentation.md)).

## The tile gate

This is the repository's contract system. **Every bundled tile** must:

1. **Keep the four-file layout** under `tiles/<name>/`:

   ```
   tiles/<name>/
     tile.json      the "pretty half": geometry, fonts, colours, `format`
     schema.json    draft-07 JSON Schema — the data contract
     samples/       one JSON payload per state the tile renders
     README.md      the binding contract, printed by the CLI
   ```

2. **Register a `TilePreset`** in `src/tiles/mod.rs`, `include_str!`-ing all four
   so the tile travels with the `.so`.

3. **Pass `pwetty check <tile>`** — template ↔ schema ↔ samples: the preset
   resolves to a config with a `format`, every `{{ … }}` the template binds is
   declared in the schema, every sample is valid JSON, and every sample renders
   through `render_template` + `markup::process` without falling back to escaped
   text.

`tests/tile_gate.rs` enforces 1 and 2; `mise run tiles` runs 3 across every
bundled tile, and `mise run render` renders every sample headless so a tile that
type-checks but paints nothing is caught too.

### Schemas defer to the producer

A tile's schema is **not** the source of truth for the data — the *producer's*
contract doc is (quivive's `docs/tile-contract.md` is the precedent). The schema
is this repo's reading of that contract, and it is written to survive the
producer moving first:

- `additionalProperties: true` — a producer that starts emitting a new field
  must not break a tile that does not render it yet. **Additive changes are not
  breaking.**
- `required` lists **only** what the producer *always* emits. A field that is
  conditional in the producer is optional here, whatever the samples happen to
  contain.

The question a change has to answer is therefore "is this additive or
breaking?", and the answer lives on the producer's side. `.claude/agents/tile-contract.md`
is the brief for that role.

### Samples are copies, never edits

Where the producer repo carries goldens, the tile's `samples/` are
**byte-identical copies** of them. The `quivive` tile is the worked example:
its five samples are quivive's own `tests/goldens/*.json`, down to
`serde_json::to_string_pretty`'s indentation.

`tests/tile_gate.rs` carries a sync test that compares them against a sibling
quivive checkout and **skips loudly** — printing what it did not verify — when
that checkout is absent, so an unrun check never reads as a passing one. Editing
one side by hand is the bug the rule exists to prevent; see
[the study](studies/quivive-tile-contribution.md).

## The `pwetty` CLI

The data source is **decoupled**: a tile declares the JSON it wants via a JSON
Schema, and the CLI surfaces that contract so a separate data-gathering layer
knows exactly what to emit.

```bash
pwetty list                  # bundled tiles + their samples
pwetty schema claude         # the tile's JSON Schema (the data contract)
pwetty check claude          # validate template ↔ schema ↔ samples
pwetty render claude --all-states -o /tmp/out   # PNG per sample (needs surfaceless EGL)
echo '{…your json…}' | pwetty render claude --data -   # render YOUR data, eyeball it
```

The data layer's loop: read the schema, emit matching JSON, pipe it through
`render --data` to confirm it looks right, then wire it into the module's `exec`.

`render` needs a GL context: run it with
`EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1` for a headless software
render. The other subcommands are pure CPU.

## Adding a tile

1. `tiles/<name>/` with the four files.
2. Register the preset in `src/tiles/mod.rs`.
3. `mise run tiles` and `mise run render` until both are green.
4. Screenshot the rendered result with `pwetty render` and put it in the tile's
   README — the picture is evidence, so it must come out of the renderer, never
   a mock-up.
