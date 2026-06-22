# pwetty-box

A [Waybar](https://github.com/Alexays/Waybar) **CFFI module** (Rust `cdylib`) that
draws elaborate, multiline text/icon **tiles** on the GPU.

Waybar loads the compiled `.so` in-process and hands the module a GTK widget. The
module renders each tile with [femtovg](https://github.com/femtovg/femtovg) (a GPU
2D canvas: paths, gradients, multiline text, font-glyph icons) into an **offscreen
image**, then composites that onto a `GtkDrawingArea` with **Cairo** — which gives
true per-pixel transparency against a translucent bar (see below).

## Interoperability constraints (why it's built this way)

Verified against the installed `waybar` binary (`v0.15.0`) and GTK `3.24.52`:

| Constraint | Consequence |
|---|---|
| Waybar links **GTK3** (`libgtk-3`, `libgtkmm-3.0`); the CFFI ABI's `get_root_widget()` returns a GTK3 `GtkContainer*` | We are a GTK3 in-process widget. (`waybar-cffi` binds gtk-rs 0.18.) |
| Waybar links **no Vulkan**, and GTK3 has no Vulkan surface widget | Rendering is OpenGL (via femtovg). We create our own **surfaceless EGL** context on the DRM render node — no window, no seat, no DRM-master. |
| **`GtkGLArea` cannot alpha-composite against a translucent bar** in GTK3 (verified hardware *and* software on 3.24.52: transparent regions render as opaque black) | We do **not** use `GtkGLArea`. Instead we render femtovg offscreen, read it back, and composite via **Cairo** onto a `GtkDrawingArea` — Cairo honors per-pixel alpha, so the tile is genuinely transparent against a see-through bar. Cost is a small GPU→CPU readback per frame, negligible for a bar tile. |

## Build

The crate's MSRV is **1.88** (`waybar-cffi` needs ≥1.85, `femtovg` ≥1.88). A
`rust-toolchain.toml` pins the toolchain to `1.92`.

```bash
cargo build --release
# -> target/release/libpwetty_box.so
```

## Use in Waybar

See [`examples/waybar-config.jsonc`](examples/waybar-config.jsonc). Minimal:

```jsonc
"modules-right": ["cffi/pwetty"],
"cffi/pwetty": {
  "module_path": "/abs/path/to/target/release/libpwetty_box.so",
  "width": 360, "height": 64, "fps": 60
}
```

Reload Waybar (`killall -SIGUSR2 waybar`, or restart). You should see an animated
gradient pill with a label and an icon glyph — the demo tile proving the pipeline.

### Config options

All keys live inside the `cffi/pwetty` block (parsed by `src/config.rs`):

| Key | Default | Meaning |
|---|---|---|
| `width` / `height` | `220` / `36` | Tile size in logical pixels. |
| `fps` | `60` | Animation framerate; `0` renders once (static). |
| `font_path` | _(system fallback)_ | TTF/OTF for text. |
| `icon_font_path` | _(unset)_ | Icon font (e.g. a Nerd Font) for glyph icons. |
| `font_size` | `14.0` | Base font size in pixels. |
| `background` | _(transparent)_ | Tile background as `#rrggbb` / `#rrggbbaa`. Leave unset for a **transparent** tile (the bar shows through); set it for an opaque tile background. |

## Architecture / where to extend

```
src/
  lib.rs        CFFI Module impl — adds a GtkDrawingArea to Waybar's container;
                its `draw` callback renders the tile offscreen and composites it
                with Cairo (per-pixel alpha). GTK glue lives only here.
  offscreen.rs  OffscreenGl: a self-owned surfaceless EGL context (render node;
                no window/seat/DRM-master) for running femtovg headless.
  gl.rs         Points the `epoxy` crate at the in-process libepoxy.
  render.rs     femtovg Canvas lifecycle, font loading, `capture()` (render a
                frame to an offscreen image + read back RGBA), `parse_hex_color`.
  config.rs     serde Config deserialized from the `cffi/...` block.
  tile.rs       >>> THE SEAM <<< the `Tile` trait + `TileContext` (geometry,
                animation clock, fonts) + a `draw_multiline` helper. Add real
                tiles here; `DemoTile` is the placeholder.
```

Render flow per frame: `DrawingArea::draw` → make the EGL context current →
`Renderer::capture` (femtovg paints all tiles into an offscreen image, reads back
RGBA) → premultiply to Cairo `ARGB32` → paint onto the widget. Animation is driven
off the GTK frame clock (`add_tick_callback` → `queue_draw`) when `fps > 0`.

To add a tile: implement `Tile` in `tile.rs` (or a new module) and register it in
`Renderer::new` (`render.rs`). `TileContext` gives you the femtovg `Canvas`, the
draw size, a `time` clock for animation, and resolved fonts.

## Testing & screenshots

- **Unit tests** (`cargo test`): config deserialization and `parse_hex_color`
  (pure logic — no GL context needed).
- **Offscreen render** (`examples/render_tile.rs`): renders the tile to a PNG via
  the surfaceless EGL context. Force software GL so it can't touch your display:
  ```bash
  EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    cargo run --example render_tile -- out.png [seconds]
  ```
  This spawns no compositor and opens no Wayland/DRM device — safe to run anywhere.
- **Live waybar** (`test/`): `cage` (headless) → `niri` (nested) → `waybar` →
  `grim`, driven by `test/shot.sh`. ⚠️ This runs a nested compositor stack; read
  the safety notes in `test/shot.sh` before running it (prefer running it from a
  separate TTY).

## Notes

- femtovg fills paths through the **stencil buffer**; its offscreen image targets
  attach one automatically, so no GTK GL-area stencil setup is needed.
- Credit/inspiration: [waybar_shader_widget](https://codeberg.org/Frieder_Hannenheim/waybar_shader_widget)
  (a pure-GLSL Shadertoy-style sibling using `GtkGLArea` + the `gl` crate — it
  renders opaque full-bleed shaders, so it never hit the transparency limitation).
