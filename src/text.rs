//! Rich text rendering via Pango/Cairo.
//!
//! Lays out Pango markup and paints it onto a Cairo context (the same surface
//! the femtovg layer composites to), and resolves the pixel rectangle of a
//! plain-text byte range so custom effects can be positioned under/over a span.

use std::cell::RefCell;
use std::collections::HashMap;

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

/// The **truly-rendered** vertical ink extent of `markup` at `size_px` in
/// `family`, as `(top, height)` in px relative to the layout's top — found by
/// rasterizing the run to a scratch surface and scanning its alpha. Unlike
/// `Layout::pixel_extents`, this reflects what's actually drawn, which matters
/// for **bitmap fonts** (e.g. Terminus) where Pango's reported extents follow
/// the requested size, not the snapped/scaled bitmap. Used to size inline
/// symbols to the digit beside them. Returns `None` if nothing inked.
pub fn ink_extent(markup: &str, family: &str, size_px: f64) -> Option<(f64, f64)> {
    use cairo::{Context, Format, ImageSurface};

    let desc = |layout: &pango::Layout| {
        let mut d = pango::FontDescription::new();
        d.set_family(family);
        d.set_absolute_size(size_px * pango::SCALE as f64);
        layout.set_font_description(Some(&d));
        layout.set_width(-1);
    };

    // Measure the layout box first, then rasterize at exactly that size.
    let tmp = ImageSurface::create(Format::ARgb32, 1, 1).ok()?;
    let cr0 = Context::new(&tmp).ok()?;
    let probe = pangocairo::functions::create_layout(&cr0);
    probe.set_markup(markup);
    desc(&probe);
    let (lw, lh) = probe.pixel_size();
    let (lw, lh) = (lw.max(1), lh.max(1));
    drop(cr0);

    let mut surface = ImageSurface::create(Format::ARgb32, lw, lh).ok()?;
    {
        let cr = Context::new(&surface).ok()?;
        let layout = pangocairo::functions::create_layout(&cr);
        layout.set_markup(markup);
        desc(&layout);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.move_to(0.0, 0.0);
        pangocairo::functions::show_layout(&cr, &layout);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let w = lw as usize;
    let h = lh as usize;
    let data = surface.data().ok()?;
    let (mut top, mut bot) = (None, 0usize);
    for y in 0..h {
        let row = &data[y * stride..y * stride + w * 4];
        if row.chunks_exact(4).any(|p| p[3] > 16) {
            top.get_or_insert(y);
            bot = y;
        }
    }
    let top = top?;
    Some((top as f64, (bot - top + 1) as f64))
}

/// Lay out `markup` **word-wrapped** to `width` px (multi-line, static — no
/// scrolling). Returns the layout; the caller reads its pixel height and paints
/// it at a top-left origin. Used for tile bodies (e.g. a wrapped window title)
/// where wrapping replaces a scrolling marquee — static text means no per-frame
/// repaint.
pub fn layout_wrapped(
    cr: &cairo::Context,
    markup: &str,
    width: f64,
    style: &TextStyle,
) -> pango::Layout {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_markup(markup);
    let mut desc = pango::FontDescription::new();
    desc.set_family(&style.font_family);
    desc.set_absolute_size(style.size_px * pango::SCALE as f64);
    layout.set_font_description(Some(&desc));
    layout.set_width((width.max(1.0) * pango::SCALE as f64) as i32);
    layout.set_wrap(pango::WrapMode::WordChar);
    layout
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

/// Paint a soft dark glow/outline behind `layout` at logical `(x, y)`, for
/// contrast when text sits over a busy or light background (e.g. a watermark
/// app icon). Call this *before* [`paint`]: it lays the glyph outlines down as a
/// feathered near-black halo (two widening strokes plus a fill), then the caller
/// paints the real coloured text on top. `w` is the halo thickness in logical px.
pub fn halo(cr: &cairo::Context, layout: &pango::Layout, x: f64, y: f64, w: f64) {
    cr.move_to(x, y);
    pangocairo::functions::layout_path(cr, layout);
    cr.set_line_join(cairo::LineJoin::Round);
    // Widest + faintest first, then tighter + stronger, then fill the glyph body
    // — a cheap feathered shadow that hugs each glyph.
    cr.set_source_rgba(0.0, 0.0, 0.02, 0.22);
    cr.set_line_width(w * 2.4);
    let _ = cr.stroke_preserve();
    cr.set_source_rgba(0.0, 0.0, 0.02, 0.5);
    cr.set_line_width(w);
    let _ = cr.stroke_preserve();
    cr.set_source_rgba(0.0, 0.0, 0.02, 0.7);
    let _ = cr.fill();
}

/// Rasterized halo+text runs: key → (surface, logical offset of its origin
/// relative to the layout's paint origin).
type HaloCache = HashMap<HaloKey, (cairo::ImageSurface, f64, f64)>;

/// (markup, family, size×64, wrap width, halo w×64, rgba8888, device scale×64,
/// sub-pixel phase×64).
type HaloKey = (String, String, u32, i32, u32, u32, u32, (u32, u32));

/// Entries beyond this drop the whole cache (cheap, and content churn — folder
/// names, idle ages — would otherwise grow it without bound).
const HALO_CACHE_MAX: usize = 512;

thread_local! {
    static HALO_CACHE: RefCell<HaloCache> = RefCell::new(HashMap::new());
}

/// [`halo`] then [`paint`] `layout` at `(x, y)`, from a cached raster. The halo
/// strokes every glyph outline three times — ~90% of a tile's draw time when
/// done per frame — yet a run only changes when its markup does. So rasterize
/// it once (at `scale` device px) onto a transparent surface and blit that on
/// later frames. `markup` must be what `layout` was built from (it keys the
/// cache, along with the font, wrap width, halo width, colour and scale).
#[allow(clippy::too_many_arguments)]
pub fn paint_haloed(
    cr: &cairo::Context,
    layout: &pango::Layout,
    markup: &str,
    x: f64,
    y: f64,
    w: f64,
    style: &TextStyle,
    scale: f64,
) {
    let q = |v: f64| (v * 64.0).round() as u32;
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    let c = style.color;
    let rgba = b(c.0) << 24 | b(c.1) << 16 | b(c.2) << 8 | b(c.3);
    // The blit must land on whole device pixels (a fractional offset resamples
    // the raster — visibly blurry text). So split the device-space origin into
    // an integer part (where we blit) and a sub-pixel phase, quantized to 1/4 px
    // and baked into the raster — the same glyph positioning as a direct draw.
    let (dev_x, dev_y) = cr.user_to_device(x, y);
    let phase = |v: f64| ((v - v.floor()) * 4.0).round() / 4.0;
    let (px, py) = (phase(dev_x), phase(dev_y));
    let key = (
        markup.to_string(),
        style.font_family.clone(),
        q(style.size_px),
        layout.width(),
        q(w),
        rgba,
        q(scale),
        (q(px), q(py)),
    );
    let hit = HALO_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&key) {
            if cache.len() >= HALO_CACHE_MAX {
                cache.clear();
            }
            let raster = rasterize_haloed(layout, w, style, scale, (px, py))?;
            cache.insert(key.clone(), raster);
        }
        cache.get(&key).cloned()
    });
    match hit {
        // `(ox, oy)`: whole-px offset of the raster's top-left from the layout
        // origin's integer pixel.
        Some((surface, ox, oy)) => {
            let _ = cr.save();
            cr.identity_matrix();
            cr.translate((dev_x - px).round() + ox, (dev_y - py).round() + oy);
            cr.scale(1.0 / scale, 1.0 / scale);
            if cr.set_source_surface(&surface, 0.0, 0.0).is_ok() {
                let _ = cr.paint();
            }
            let _ = cr.restore();
        }
        // Couldn't allocate a raster: draw directly (the slow, exact path).
        None => {
            halo(cr, layout, x, y, w);
            paint(cr, layout, x, y, style);
        }
    }
}

/// Draw [`halo`] + [`paint`] of `layout` onto a fresh transparent surface sized
/// to its ink (plus the halo's reach) at `scale` device px, with the layout
/// origin at sub-pixel `phase`. Returns the surface and its (whole-px) top-left
/// relative to the layout origin's integer pixel.
fn rasterize_haloed(
    layout: &pango::Layout,
    w: f64,
    style: &TextStyle,
    scale: f64,
    phase: (f64, f64),
) -> Option<(cairo::ImageSurface, f64, f64)> {
    let (ink, logical) = layout.pixel_extents();
    // Union of ink + logical boxes, grown by the widest stroke's half-width;
    // whole px, so the raster stays pixel-aligned when blitted.
    let pad = (w * 1.2).ceil() + 2.0;
    let x0 = ink.x().min(logical.x()) as f64 - pad;
    let y0 = ink.y().min(logical.y()) as f64 - pad;
    let x1 = (ink.x() + ink.width()).max(logical.x() + logical.width()) as f64 + pad + 1.0;
    let y1 = (ink.y() + ink.height()).max(logical.y() + logical.height()) as f64 + pad + 1.0;
    let sw = ((x1 - x0) * scale).ceil().max(1.0) as i32;
    let sh = ((y1 - y0) * scale).ceil().max(1.0) as i32;

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, sw, sh).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        cr.scale(scale, scale);
        // The layout was built against another context; re-sync it to this one
        // (same font options/resolution), then draw exactly as the direct path.
        pangocairo::functions::update_layout(&cr, layout);
        let (lx, ly) = (phase.0 - x0, phase.1 - y0);
        halo(&cr, layout, lx, ly, w);
        paint(&cr, layout, lx, ly, style);
    }
    surface.flush();
    Some((surface, x0, y0))
}
