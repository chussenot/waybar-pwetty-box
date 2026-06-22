# pwetty-box

A [Waybar](https://github.com/Alexays/Waybar) **CFFI module** (Rust `cdylib`) that
draws elaborate, multiline text/icon **tiles** on the GPU.

Waybar loads the compiled `.so` in-process and hands the module a GTK widget. Each
tile is drawn as **two composited layers** onto a `GtkDrawingArea`:

- a **GPU layer** — [femtovg](https://github.com/femtovg/femtovg) rendered into an
  offscreen image on our own surfaceless EGL context (backgrounds, gradients, and
  future shader effects);
- a **text layer** — rich **Pango markup** drawn with PangoCairo on top.

Both go through **Cairo**, which gives true per-pixel transparency against a
translucent bar (see below) and lets **custom effect tags** (e.g. `<box>`) bridge
the two — positioned via the Pango layout, drawn by the GPU/Cairo layer.

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
| `fps` | `60` | Animation framerate; `0` = static/content-driven (redraw only when content changes). |
| `text` | _(unset)_ | Static value, substituted into `format` (escaped). Use for fixed content. |
| `exec` | _(unset)_ | Shell command; its stdout is the value substituted into `format`. |
| `interval` | `0` | Re-run cadence for `exec`, in seconds (`0` = run once). |
| `icon` | _(unset)_ | Glyph prepended to the text (rendered via Pango font fallback). |
| `format` | `"{}"` | **Pango markup** template; `{}` = the escaped `text`/`exec` value. May contain custom effect tags (see below). |
| `font_size` | `14.0` | Base text size in pixels (per-span sizes via markup override it). |
| `background` | _(transparent)_ | Tile background as `#rrggbb` / `#rrggbbaa`. Leave unset for a **transparent** tile (the bar shows through); set it for an opaque background. |
| `font_path` / `icon_font_path` | _(system)_ | Fonts for the **demo tile** (femtovg) only; content tiles render via Pango using system fonts. |

With no `text`/`exec`, the module renders the animated demo tile. With either set,
it renders a content tile; `exec` refreshes on a background thread, so a slow
command never blocks the bar.

### Rich content & custom effects

`format` is a **Pango markup** template, so content can be styled with the usual
Pango spans — per-span colour, size, weight, font, plus `\n` for multiple lines:

```jsonc
"format": "<span size='xx-large' weight='bold' foreground='#89b4fa'>{}</span>\n<span size='small' foreground='#9399b2'>load</span>"
```

The substituted `{}` value is Pango-escaped, so command output can't break the
markup. On top of standard Pango tags, **custom effect tags** are extracted and
drawn by our own renderer, positioned via the Pango layout:

```jsonc
"format": "vol <box bg='#f38ba8cc'>{}</box>"   // rounded highlight behind the value
```

Currently `<box bg='#rrggbb[aa]'>` (a rounded highlight) is implemented; the same
seam (`markup::process` → effect span → `text::span_rect` → draw) is where GPU
shader effects (`<glow>`, `<shader>`) plug in next.

## Architecture / where to extend

```
src/
  lib.rs        CFFI Module impl — adds a GtkDrawingArea to Waybar's container.
                Its `draw` callback composes two layers: femtovg GPU layer +
                Pango text layer (`draw_content`), with `<box>` effects between.
  offscreen.rs  OffscreenGl: a self-owned surfaceless EGL context (render node;
                no window/seat/DRM-master) for running femtovg headless.
  gl.rs         Points the `epoxy` crate at the in-process libepoxy.
  render.rs     femtovg Canvas lifecycle, `capture()` (render to an offscreen
                image + read back RGBA), `parse_hex_color`.
  config.rs     serde Config deserialized from the `cffi/...` block.
  content.rs    TileContent + ContentStore (thread-safe) + sources (static text
                or a command refreshed on a background thread) → a markup string.
  markup.rs     >>> EFFECT SEAM <<< pure XML routing: split content into
                Pango-safe markup + extracted custom-tag EffectSpans. Escaping,
                `apply_format`. (Heavily unit-tested.)
  text.rs       Pango/Cairo: lay out + paint markup; `span_rect` locates a span.
  tile.rs       femtovg `Tile` trait + `TileContext` + the animated `DemoTile`
                (shown when no content source is configured).
```

Render flow per frame: `DrawingArea::draw` → (1) make the EGL context current,
`Renderer::capture` the femtovg background → premultiply → Cairo paint; (2)
`draw_content`: `markup::process` the content → `text::layout` (Pango) → draw each
effect span behind the text → `text::paint`. Redraws come from the frame clock
(`fps > 0`) and/or a content dirty-flag poll.

**To add a custom effect** (e.g. `<glow>`, `<shader>`): add the tag name to
`EFFECT_TAGS` in `lib.rs`, then handle it in `draw_content` — `text::span_rect`
gives you the pixel rect of its text, into which you draw (Cairo, or a femtovg
shader pass composited like the background layer).

## Testing & screenshots

- **Unit tests** (`cargo test`): the markup router (`markup.rs`, 14 tests),
  content/`build_markup`, config deserialization, `parse_hex_color` — all pure
  logic, no GL/GTK context needed.
- **Vision tests** (offscreen → PNG, pure CPU, safe anywhere):
  ```bash
  # rich text via Pango/Cairo
  cargo run --example render_text -- out.png
  # full content path (Pango markup + <box> effect) via draw_content
  cargo run --example render_content -- out.png "<box bg='#a6e3a180'>42%</box>" 40
  # the femtovg demo tile (surfaceless GL — force software so it can't touch your display)
  EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    cargo run --example render_tile -- out.png [seconds]
  ```
  Inspect the PNGs by eye — these caught a transparency bug no unit test could.
- **Live waybar** (`test/`): `cage` (headless) → `niri` (nested) → `waybar` →
  `grim`, driven by `test/shot.sh`. ⚠️ Runs a nested compositor stack; read the
  safety notes in `test/shot.sh` (prefer a separate TTY).

## Notes

- femtovg fills paths through the **stencil buffer**; its offscreen image targets
  attach one automatically, so no GTK GL-area stencil setup is needed.
- Credit/inspiration: [waybar_shader_widget](https://codeberg.org/Frieder_Hannenheim/waybar_shader_widget)
  (a pure-GLSL Shadertoy-style sibling using `GtkGLArea` + the `gl` crate — it
  renders opaque full-bleed shaders, so it never hit the transparency limitation).
