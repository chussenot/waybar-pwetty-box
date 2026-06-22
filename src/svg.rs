//! SVG rasterization for bundled/path icons.
//!
//! Renders an SVG to a square `px`×`px` premultiplied buffer via `resvg`
//! (pure-Rust usvg + tiny-skia), in Cairo's ARGB32 byte order (little-endian
//! BGRA) so the result composites straight onto the tile surface — the same
//! RGBA-buffer path the femtovg layer and span shaders use. The vector source
//! is rasterized at the exact device pixel size, so icons stay crisp.

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

/// The bundled SVG icon set, embedded in the binary (name → SVG source). These
/// are monochrome silhouettes meant to be tinted; pass `color` on `<icon>` to
/// colour them. App logos and other artwork come via `<icon src="path">`.
pub const BUNDLED: &[(&str, &str)] = &[
    ("folder", include_str!("../icons/folder.svg")),
    ("check", include_str!("../icons/check.svg")),
    ("arrow-up", include_str!("../icons/arrow-up.svg")),
    ("bell", include_str!("../icons/bell.svg")),
    ("code", include_str!("../icons/code.svg")),
    ("terminal", include_str!("../icons/terminal.svg")),
    ("gear", include_str!("../icons/gear.svg")),
    ("app", include_str!("../icons/app.svg")),
];

/// Look up a bundled icon's SVG source by name.
pub fn bundled(name: &str) -> Option<&'static str> {
    BUNDLED.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Rasterize `svg` into a `px`×`px` buffer of premultiplied ARGB32 bytes
/// (little-endian: B, G, R, A), fitting the artwork centered. When `tint` is set
/// (RGB in 0..=1), the SVG is used as an alpha mask filled with that colour (a
/// monochrome silhouette); otherwise the artwork's own colours are kept (e.g. a
/// multi-colour app logo). Returns `None` on a parse/allocation failure.
pub fn rasterize_argb32(svg: &[u8], px: u32, tint: Option<(f32, f32, f32)>) -> Option<Vec<u8>> {
    if px == 0 {
        return None;
    }
    let tree = Tree::from_data(svg, &Options::default()).ok()?;
    let mut pixmap = Pixmap::new(px, px)?;

    let size = tree.size();
    let scale = px as f32 / size.width().max(size.height());
    let dx = (px as f32 - size.width() * scale) / 2.0;
    let dy = (px as f32 - size.height() * scale) / 2.0;
    let transform = Transform::from_scale(scale, scale).post_translate(dx, dy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia gives premultiplied RGBA; Cairo ARGB32 wants premultiplied BGRA.
    let mut out = pixmap.data().to_vec();
    for p in out.chunks_exact_mut(4) {
        let (r, b, a) = (p[0], p[2], p[3]);
        match tint {
            Some((tr, tg, tb)) => {
                let af = a as f32;
                p[0] = (tb * af) as u8; // B (premultiplied)
                p[1] = (tg * af) as u8; // G
                p[2] = (tr * af) as u8; // R
                p[3] = a;
            }
            None => {
                p[0] = b; // swap R<->B; G (p[1]) and A unchanged
                p[2] = r;
            }
        }
    }
    Some(out)
}
