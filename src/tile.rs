//! The tile abstraction — this is the expressivity seam.
//!
//! A [`Tile`] paints itself onto a femtovg canvas given a [`TileContext`]
//! (geometry, animation clock, fonts). Concrete tiles — the "elaborate
//! multiline text/icon" widgets — get added here later; [`DemoTile`] exists to
//! prove the whole pipeline (waybar → GtkGLArea → femtovg → screen) end to end.

use femtovg::renderer::OpenGl;
use femtovg::{Align, Baseline, Canvas, Color, FontId, Paint, Path};

/// Fonts available to tiles. Resolved once at canvas creation.
#[derive(Clone, Copy, Default)]
pub struct Fonts {
    /// Primary text font.
    pub text: Option<FontId>,
    /// Icon glyph font (e.g. a Nerd Font), if configured.
    pub icon: Option<FontId>,
}

/// Everything a tile needs to paint one frame.
pub struct TileContext<'a> {
    pub canvas: &'a mut Canvas<OpenGl>,
    /// Drawable area in device pixels.
    pub width: f32,
    pub height: f32,
    /// Seconds since module start — drive animations from this.
    pub time: f32,
    pub fonts: Fonts,
}

/// A self-painting widget tile.
pub trait Tile {
    fn paint(&self, cx: &mut TileContext);
}

/// Draw `text` as multiple lines, honoring `\n`, top-left anchored at (x, y).
/// Returns the y baseline after the last line. A small helper tiles can reuse.
pub fn draw_multiline(
    cx: &mut TileContext,
    x: f32,
    mut y: f32,
    text: &str,
    paint: &Paint,
    line_height: f32,
) -> f32 {
    for line in text.split('\n') {
        let _ = cx.canvas.fill_text(x, y, line, paint);
        y += line_height;
    }
    y
}

/// Placeholder tile: an animated gradient pill with a couple of text lines and
/// an icon glyph. Replace/extend with real tiles via the [`Tile`] trait.
pub struct DemoTile;

impl Tile for DemoTile {
    fn paint(&self, cx: &mut TileContext) {
        let (w, h, t) = (cx.width, cx.height, cx.time);

        // Animated rounded-rectangle background with a moving gradient.
        let mut bg = Path::new();
        let pad = 2.0;
        let radius = (h - 2.0 * pad) * 0.35;
        bg.rounded_rect(pad, pad, w - 2.0 * pad, h - 2.0 * pad, radius);

        let phase = t * 0.6;
        let c0 = Color::hsl(phase % 1.0, 0.55, 0.45);
        let c1 = Color::hsl((phase + 0.18) % 1.0, 0.6, 0.55);
        let grad = Paint::linear_gradient(pad, pad, w - pad, h - pad, c0, c1);
        cx.canvas.fill_path(&bg, &grad);

        // Icon glyph (uses the icon font if present, else the text font).
        let icon_font = cx.fonts.icon.or(cx.fonts.text);
        if let Some(f) = icon_font {
            let mut p = Paint::color(Color::white());
            p.set_font(&[f]);
            p.set_font_size(h * 0.5);
            p.set_text_baseline(Baseline::Middle);
            p.set_text_align(Align::Left);
            //  is a Nerd Font glyph; renders as a box if the font lacks it.
            let _ = cx.canvas.fill_text(h * 0.32, h * 0.5, "\u{f0e7}", &p);
        }

        // Two-line label.
        if let Some(f) = cx.fonts.text {
            let mut p = Paint::color(Color::rgbf(0.97, 0.97, 1.0));
            p.set_font(&[f]);
            p.set_font_size(h * 0.30);
            p.set_text_baseline(Baseline::Alphabetic);
            let lh = h * 0.34;
            let label = format!("pwetty-box\n{:.1}s", t);
            draw_multiline(cx, h * 0.72, h * 0.42, &label, &p, lh);
        }
    }
}
