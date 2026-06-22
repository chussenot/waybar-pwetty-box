//! Rich text rendering via Pango/Cairo.
//!
//! Lays out Pango markup and paints it onto a Cairo context (the same surface
//! the femtovg layer composites to), and resolves the pixel rectangle of a
//! plain-text byte range so custom effects can be positioned under/over a span.

use waybar_cffi::gtk::{cairo, pango};

/// Base text style for a tile (overridable per-span via Pango markup).
#[derive(Debug, Clone)]
pub struct TextStyle {
    pub font_family: String,
    pub size_px: f64,
    /// RGBA, components in 0.0..=1.0.
    pub color: (f64, f64, f64, f64),
    pub align_center: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "sans".into(),
            size_px: 16.0,
            color: (0.95, 0.95, 1.0, 1.0),
            align_center: false,
        }
    }
}

/// A pixel rectangle (x, y, w, h).
pub type Rect = (f64, f64, f64, f64);

/// Lay out `markup` for a `width`×`height` tile (device px) with `style`.
/// Returns the layout and the (x, y) top-left at which it should be painted
/// (vertically centered, left-padded). Does NOT paint — so callers can draw
/// effects behind the text first.
pub fn layout(
    cr: &cairo::Context,
    markup: &str,
    width: f64,
    height: f64,
    style: &TextStyle,
) -> (pango::Layout, f64, f64) {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_markup(markup);

    let mut desc = pango::FontDescription::new();
    desc.set_family(&style.font_family);
    desc.set_absolute_size(style.size_px * pango::SCALE as f64);
    layout.set_font_description(Some(&desc));

    // Wrap to the tile width so multiline markup folds inside the tile.
    layout.set_width((width * pango::SCALE as f64) as i32);

    if style.align_center {
        layout.set_alignment(pango::Alignment::Center);
    }

    // Centered text already lays out horizontally within the wrap width, so it
    // needs no left pad; left-aligned text gets a small pad.
    let ox = if style.align_center {
        0.0
    } else {
        height * 0.12
    };
    // Vertically center the laid-out block within the tile, clamped to the top.
    let oy = ((height - layout.pixel_size().1 as f64) / 2.0).max(0.0);

    (layout, ox, oy)
}

/// Lay out `markup` as a single, unwrapped line for a tile of `height` device px.
/// Returns the layout, the vertical offset to paint it at (centered), and the
/// line's pixel width — used by the scrolling ticker.
pub fn layout_line(
    cr: &cairo::Context,
    markup: &str,
    height: f64,
    style: &TextStyle,
) -> (pango::Layout, f64, f64) {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_markup(markup);

    let mut desc = pango::FontDescription::new();
    desc.set_family(&style.font_family);
    desc.set_absolute_size(style.size_px * pango::SCALE as f64);
    layout.set_font_description(Some(&desc));

    layout.set_width(-1); // no wrapping → a single line
    let (lw, lh) = layout.pixel_size();
    let oy = ((height - lh as f64) / 2.0).max(0.0);
    (layout, oy, lw as f64)
}

/// Pixel rect of plain-text byte range `[start, end)` within `layout` painted at
/// origin `(ox, oy)`.
///
/// If the span wraps across lines (start and end land on different layout
/// lines), the returned rect is a single bounding box anchored on the start
/// line — adequate for v1 single-line effect positioning.
pub fn span_rect(layout: &pango::Layout, ox: f64, oy: f64, start: usize, end: usize) -> Rect {
    let scale = pango::SCALE as f64;
    let start_pos = layout.index_to_pos(start as i32);
    let end_pos = layout.index_to_pos(end as i32);

    let x = ox + start_pos.x() as f64 / scale;
    let y = oy + start_pos.y() as f64 / scale;
    let h = start_pos.height() as f64 / scale;
    let w = (end_pos.x() - start_pos.x()).abs() as f64 / scale;

    (x, y, w, h)
}

/// Paint a previously laid-out `layout` at `(x, y)` using `style.color` as the
/// default foreground (spans with an explicit `foreground` in the markup
/// override it).
pub fn paint(cr: &cairo::Context, layout: &pango::Layout, x: f64, y: f64, style: &TextStyle) {
    cr.set_source_rgba(style.color.0, style.color.1, style.color.2, style.color.3);
    cr.move_to(x, y);
    pangocairo::functions::show_layout(cr, layout);
}
