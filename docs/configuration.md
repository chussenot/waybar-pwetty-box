---
title: Module configuration
status: active
date: 2026-08-29
---

# Module configuration

Everything below lives inside the waybar `cffi/pwetty` block and is parsed by
`src/config.rs`. A worked file is in
[`examples/waybar-config.jsonc`](../examples/waybar-config.jsonc).

Minimal:

```jsonc
"modules-right": ["cffi/pwetty"],
"cffi/pwetty": {
  "module_path": "/abs/path/to/target/release/libpwetty_box.so",
  "width": 360, "height": 64, "fps": 60
}
```

Reload waybar (`killall -SIGUSR2 waybar`, or restart — see
[ADR-0002](adr/0002-install-restart-contract.md) for which one you actually
need). You should see an animated gradient pill with a label and an icon glyph
— the demo tile proving the pipeline.

## Keys

| Key | Default | Meaning |
|---|---|---|
| `width` / `height` | `220` / `36` | Tile size in logical pixels. |
| `fps` | `60` | Animation framerate; `0` = static/content-driven (redraw only when content changes). |
| `text` | _(unset)_ | Static data for the template. Use for fixed content. |
| `exec` | _(unset)_ | Shell command; its stdout is the tile's **data** (JSON if parseable, else plain text). |
| `interval` | `0` | Re-run cadence for `exec`, in seconds (`0` = run once). Ignored when `stream` is set. |
| `stream` | `false` | **Push mode** for `exec` (see below). |
| `icon` | _(unset)_ | Glyph prepended to the content, sized + vertically centered on the text. |
| `format` | `"{{ value }}"` | **Template** ([minijinja](https://github.com/mitsuhiko/minijinja)) rendered against the data → a Pango-markup string. See [markup.md](markup.md). |
| `font_size` | `14.0` | Base text size in pixels (per-span sizes via markup override it). |
| `background` | _(transparent)_ | Tile background as `#rrggbb` / `#rrggbbaa`. Leave unset for a **transparent** tile (the bar shows through); set it for an opaque background. |
| `background_shader` | _(unset)_ | Path to a Shadertoy-style GLSL fragment shader rendered as the tile's animated background. Hot-reloaded on file change. See [shaders.md](shaders.md). |
| `shader_uniforms` | _(unset)_ | Map of `float` uniform → template (e.g. `{ "u_load": "{{ cpu.pct }}" }`), resolved from the data so the shader reacts to it. |
| `font_path` / `icon_font_path` | _(system)_ | Fonts for the **demo tile** (femtovg) only; content tiles render via Pango using system fonts. |
| `tile` | _(unset)_ | Name of a bundled preset to layer underneath this config. See [tiles.md](tiles.md). |
| `tile_file` | _(unset)_ | Path to an external preset JSON — iterate on a tile without rebuilding. |

With no `text`/`exec`, the module renders the animated demo tile. With either
set, it renders a content tile; `exec` refreshes on a background thread, so a
slow command never blocks the bar.

## Poll vs. push

`interval` polls: the command is re-run every N seconds and its stdout replaces
the data.

`stream: true` pushes: the command is spawned **once** and each newline-delimited
stdout line is fresh data (sub-150ms repaint). The command stays alive and prints
one JSON object per line whenever something changes; on EOF/exit the last content
is kept and the command respawns after a ~1s backoff. Reach for it when the
source is event-driven (a niri desktop-switch watcher, a fleet tick) and needs to
update faster than a poll would.

Stream input is bounded on purpose — over-long lines and invalid UTF-8 are
dropped and the reader recovers rather than growing without limit; see the
`content::tests::stream_recovers_*` tests.
