---
title: pwetty-box
status: active
date: 2026-08-29
---

# pwetty-box

A [Waybar](https://github.com/Alexays/Waybar) **CFFI module** (Rust `cdylib`)
that draws elaborate, multiline text/icon **tiles** on the GPU — and, more to the
point, a boundary: this module owns how a tile *looks*, and something else
entirely owns what it *says*.

```bash
mise run deps      # check the system libs a build needs
mise run install   # build, put the `pwetty` CLI on PATH, restart waybar onto the new .so
mise run check     # lint, tests, tiles, a headless render, docs
mise tasks         # the rest
```

## Three things worth knowing before the reference

### The pretty half is on this side of the wire

A tile config splits in two. The **pretty half** — geometry, fonts, colours, the
template, the effect tags — ships inside the `.so` as a named preset. The **data
half** is a JSON object some other program emits. A waybar module names the
preset and supplies the command; nothing else.

That boundary is what lets a producer be written against
`pwetty schema <tile>` and iterated with `pwetty render <tile> --data -` without
this repository being rebuilt, and it is why every tile carries a JSON Schema
that the build checks against its own template.

→ [docs/tiles.md](docs/tiles.md), [docs/configuration.md](docs/configuration.md)

### Tiles are data-driven, so the producer never picks a layout

The producer emits **facts** — how many sessions a desktop holds, what each
repo's agent counts are. The template branches on those facts to choose a
layout. There is no `layout` field in any schema, and there is no waybar module
per tile *state*.

The alternative — letting the payload say `"layout": "dual"` — costs a
coordinated release across two repositories every time a layout is renamed, and
produces a field nothing can check.

→ [ADR-0001](docs/adr/0001-data-driven-tiles.md),
[docs/markup.md](docs/markup.md)

### Installing restarts waybar, and proves the inode

`killall -SIGUSR2 waybar` is the command everyone recommends and it **cannot pick
up a rebuilt `.so`**. Waybar reloads its config and recreates CFFI modules on
that signal, but never `dlclose`/`dlopen`s the library — so the old code keeps
running against a deleted inode, and your rebuild looks like it did nothing, for
hours.

So `mise run install` kills waybar, relaunches it detached, and then compares the
mapped inode in `/proc/<pid>/maps` against the file on disk. A restart that
cannot prove it loaded the new code is not a gate.

→ [ADR-0002](docs/adr/0002-install-restart-contract.md)

## Build

MSRV is **1.88** (`waybar-cffi` needs ≥ 1.85, `femtovg` ≥ 1.88);
`rust-toolchain.toml` pins the toolchain to **1.92**, which rustup honours on the
first `cargo` call. `cargo build --release` produces
`target/release/libpwetty_box.so`.

Rendering is femtovg-offscreen composited through Cairo rather than a
`GtkGLArea`, because a `GtkGLArea` cannot alpha-composite against a translucent
bar in GTK3 — the reason, and the rest of the interop constraints, are in
[docs/rendering.md](docs/rendering.md).

## The docs

| | |
|---|---|
| [configuration.md](docs/configuration.md) | every `cffi/pwetty` key; poll vs. push |
| [markup.md](docs/markup.md) | data → template → markup, effect tags, inline embeds, icons |
| [shaders.md](docs/shaders.md) | full-bleed `background_shader`, masked `<bg preset>` |
| [tiles.md](docs/tiles.md) | bundled tiles, the tile gate, the `pwetty` CLI |
| [rendering.md](docs/rendering.md) | the two-layer pipeline and why it is shaped that way |
| [testing.md](docs/testing.md) | `mise run check`, vision tests, CI |
| [versioning.md](docs/versioning.md) | conventional commits, cocogitto, the changelog |
| [documentation.md](docs/documentation.md) | the docs contract the `docs` gate enforces |
| [adr/](docs/adr) | decisions, with alternatives priced |
| [studies/](docs/studies) | field evidence: what actually happened, and what rule it produced |

## Contributing

Conventional commits **with a scope** (`feat(tiles):`, `fix(cli):`), bodies that
explain why. `mise run check` must be green. Versions move only through
`cog bump`. Details in [docs/versioning.md](docs/versioning.md).

## Licence

MIT OR Apache-2.0.
