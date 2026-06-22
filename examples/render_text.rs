//! Offscreen, software-only render of rich-text markup to a PNG.
//!
//! Pure Cairo + Pango — no GL/EGL, no GPU, no display server — so it is safe to
//! run anywhere. It lays out a sample Pango markup string and paints it onto an
//! ARGB32 image surface, then writes the surface to a PNG.
//!
//!   cargo run --example render_text [-- out.png]
//!
//! Note: cairo's own `write_to_png` lives behind the crate's `png` feature,
//! which this build does not enable, so we read back the ARGB32 surface data
//! and encode the PNG with the `image` dev-dependency instead.

use pwetty_box::text::{self, TextStyle};
use waybar_cffi::gtk::cairo;

const WIDTH: i32 = 760;
const HEIGHT: i32 = 150;

fn main() {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, WIDTH, HEIGHT)
        .expect("create image surface");

    {
        let cr = cairo::Context::new(&surface).expect("create cairo context");

        // Dark bar-like background (#1e1e2e).
        cr.set_source_rgb(
            0x1e as f64 / 255.0,
            0x1e as f64 / 255.0,
            0x2e as f64 / 255.0,
        );
        cr.paint().expect("paint background");

        let style = TextStyle {
            font_family: "sans".into(),
            size_px: 34.0,
            color: (0.95, 0.95, 1.0, 1.0),
            align_center: false,
        };

        let markup = "<span size='xx-large' weight='bold' foreground='#89b4fa'>CPU</span>  \
                      <span foreground='#a6e3a1'>42%</span>\n\
                      <span size='small' foreground='#9399b2'>8 cores · 3.4 GHz</span>";

        let (layout, ox, oy) = text::layout(&cr, markup, WIDTH as f64, HEIGHT as f64, &style);
        text::paint(&cr, &layout, ox, oy, &style);
    } // drop the context so the surface's data is no longer borrowed.

    surface.flush();

    // Read back ARGB32 (premultiplied, native-endian: B, G, R, A on LE) and
    // convert to straight-alpha RGBA for the PNG encoder.
    let stride = surface.stride() as usize;
    let w = WIDTH as usize;
    let h = HEIGHT as usize;
    let mut rgba = Vec::with_capacity(w * h * 4);
    {
        let data = surface.data().expect("borrow surface data");
        for row in 0..h {
            let base = row * stride;
            for col in 0..w {
                let px = base + col * 4;
                let b = data[px] as u32;
                let g = data[px + 1] as u32;
                let r = data[px + 2] as u32;
                let a = data[px + 3] as u32;
                // Un-premultiply.
                let (ru, gu, bu) = if a == 0 {
                    (0, 0, 0)
                } else {
                    (
                        (r * 255 / a).min(255) as u8,
                        (g * 255 / a).min(255) as u8,
                        (b * 255 / a).min(255) as u8,
                    )
                };
                rgba.push(ru);
                rgba.push(gu);
                rgba.push(bu);
                rgba.push(a as u8);
            }
        }
    }

    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/claude-1000/text-sample.png".into());
    image::RgbaImage::from_raw(WIDTH as u32, HEIGHT as u32, rgba)
        .expect("image from raw")
        .save(&out)
        .expect("save png");

    println!("{out}");
}
