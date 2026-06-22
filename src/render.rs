//! femtovg canvas lifecycle and per-frame painting.

use femtovg::renderer::OpenGl;
use femtovg::{Canvas, Color};

use crate::config::Config;
use crate::gl;
use crate::tile::{DemoTile, Fonts, Tile, TileContext};

/// Common system fonts to try when the config doesn't pin one.
const FONT_FALLBACKS: &[&str] = &[
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/Hack-Regular.ttf",
];

/// Owns the GPU canvas, resolved fonts, and the active tiles.
pub struct Renderer {
    canvas: Canvas<OpenGl>,
    fonts: Fonts,
    background: Color,
    tiles: Vec<Box<dyn Tile>>,
}

/// Parse a `#rrggbb` or `#rrggbbaa` hex string into a femtovg [`Color`].
/// Returns `None` for malformed input.
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#').unwrap_or(s);
    let byte = |i: usize| -> Option<u8> { u8::from_str_radix(s.get(i..i + 2)?, 16).ok() };
    match s.len() {
        6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

impl Renderer {
    /// Build the femtovg canvas on the *currently-current* GL context. All
    /// rendering goes through [`Renderer::capture`] into an offscreen image
    /// target, so the default screen framebuffer is never used.
    ///
    /// `content_present`: when a content source is configured, the femtovg layer
    /// only draws the background (the rich text is rendered on top with Pango);
    /// otherwise it renders the animated [`DemoTile`].
    pub fn new(config: &Config, content_present: bool) -> Result<Self, femtovg::ErrorKind> {
        gl::ensure_loaded();
        // SAFETY: a GL context is current.
        let renderer = unsafe { OpenGl::new_from_function(gl::proc_addr) }?;
        let mut canvas = Canvas::new(renderer)?;

        let fonts = Fonts {
            text: load_font(&mut canvas, config.font_path.as_deref(), FONT_FALLBACKS),
            icon: config
                .icon_font_path
                .as_deref()
                .and_then(|p| canvas.add_font(p).ok()),
        };

        let background = config
            .background
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(Color::rgbaf(0.0, 0.0, 0.0, 0.0));

        // With content, the femtovg layer is just the background (Pango draws the
        // text on top); without it, show the animated demo tile.
        let tiles: Vec<Box<dyn Tile>> = if content_present {
            vec![]
        } else {
            vec![Box::new(DemoTile)]
        };

        Ok(Self {
            canvas,
            fonts,
            background,
            tiles,
        })
    }

    /// Render one frame to the current target. `width`/`height` are device
    /// pixels, `time` is seconds.
    pub fn render(&mut self, width: u32, height: u32, dpi: f32, time: f32) {
        self.canvas.set_size(width, height, dpi);
        self.paint_tiles(width, height, time);
        self.canvas.flush();
    }

    /// Clear + paint all tiles into whatever render target is currently bound.
    /// Does NOT call `set_size` (which would reset the target to Screen) so it
    /// can be used after switching to an offscreen image target.
    fn paint_tiles(&mut self, width: u32, height: u32, time: f32) {
        self.canvas.clear_rect(0, 0, width, height, self.background);

        for tile in &self.tiles {
            let mut cx = TileContext {
                canvas: &mut self.canvas,
                width: width as f32,
                height: height as f32,
                time,
                fonts: self.fonts,
            };
            tile.paint(&mut cx);
        }
    }

    /// Render one frame into an offscreen image and read it back as RGBA8
    /// (top-left origin). Used by the offscreen example; needs only a current
    /// GL context (no window/default framebuffer), so it works surfaceless.
    pub fn capture(
        &mut self,
        width: u32,
        height: u32,
        dpi: f32,
        time: f32,
    ) -> Result<(usize, usize, Vec<u8>), femtovg::ErrorKind> {
        // set_size FIRST — it resets the target to Screen — THEN switch to the
        // offscreen image, so the tile paint lands in the image (not Screen,
        // which is FBO 0 and absent in a surfaceless context).
        self.canvas.set_size(width, height, dpi);
        let img = self.canvas.create_image_empty(
            width as usize,
            height as usize,
            femtovg::PixelFormat::Rgba8,
            femtovg::ImageFlags::FLIP_Y,
        )?;
        self.canvas
            .set_render_target(femtovg::RenderTarget::Image(img));
        self.paint_tiles(width, height, time);
        self.canvas.flush();
        let shot = self.canvas.screenshot()?;
        self.canvas.set_render_target(femtovg::RenderTarget::Screen);
        self.canvas.delete_image(img);

        let (w, h) = (shot.width(), shot.height());
        let buf = shot
            .buf()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect();
        Ok((w, h, buf))
    }
}

/// Try the configured path first, then the fallback list.
fn load_font(
    canvas: &mut Canvas<OpenGl>,
    configured: Option<&str>,
    fallbacks: &[&str],
) -> Option<femtovg::FontId> {
    if let Some(path) = configured {
        if let Ok(id) = canvas.add_font(path) {
            return Some(id);
        }
        eprintln!("pwetty-box: failed to load configured font '{path}', trying fallbacks");
    }
    fallbacks.iter().find_map(|p| canvas.add_font(p).ok())
}

#[cfg(test)]
mod tests {
    use super::parse_hex_color;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0 / 255.0
    }

    #[test]
    fn parses_rgb() {
        let c = parse_hex_color("#1e1e2e").expect("valid rgb");
        assert!(approx(c.r, 0x1e as f32 / 255.0));
        assert!(approx(c.g, 0x1e as f32 / 255.0));
        assert!(approx(c.b, 0x2e as f32 / 255.0));
        assert!(approx(c.a, 1.0));
    }

    #[test]
    fn parses_rgba_and_optional_hash() {
        let c = parse_hex_color("ff800040").expect("valid rgba without #");
        assert!(approx(c.r, 1.0));
        assert!(approx(c.g, 0x80 as f32 / 255.0));
        assert!(approx(c.a, 0x40 as f32 / 255.0));
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse_hex_color("#123").is_none()); // wrong length
        assert!(parse_hex_color("#zz0000").is_none()); // non-hex
        assert!(parse_hex_color("").is_none());
    }
}
