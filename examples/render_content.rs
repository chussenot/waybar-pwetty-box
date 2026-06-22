//! Offscreen vision harness for the CONTENT path (Pango markup + custom `<box>`
//! effect), rendered via the exact `draw_content` compose used in the live
//! widget — but onto a Cairo image surface, so it's pure CPU and safe anywhere.
//!
//!   cargo run --example render_content -- out.png "<markup>" [font_size]
//!
//! Default markup exercises spans (color/size/weight), multiline, and a `<box>`.

use std::fs::File;

use pwetty_box::config::Config;
use waybar_cffi::gtk::cairo::{Context, Format, ImageSurface};

const W: i32 = 760;
const H: i32 = 150;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/claude-1000/content-sample.png".into());
    let markup = std::env::args().nth(2).unwrap_or_else(|| {
        "<span size='xx-large' weight='bold' foreground='#89b4fa'>CPU</span> \
         <box bg='#a6e3a180'>42%</box>\n\
         <span size='small' foreground='#9399b2'>8 cores · 3.4 GHz</span>"
            .into()
    });
    let font_size: f32 = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(34.0);
    // Optional icon glyph (arg 4): prepended via the same centering the `icon`
    // config uses, so we can vision-check icon/text vertical alignment.
    let markup = match std::env::args().nth(4) {
        Some(icon) if !icon.is_empty() => format!(
            "{}  {markup}",
            pwetty_box::markup::icon_span(&icon, font_size as f64, 1.3)
        ),
        _ => markup,
    };
    // Animation time (arg 5), so time-driven effects (ticker) can be captured at
    // different frames.
    let time: f32 = std::env::args()
        .nth(5)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.5);

    let surface = ImageSurface::create(Format::ARgb32, W, H).expect("surface");
    let cr = Context::new(&surface).expect("cairo context");

    // A translucent dark backing, like a bar.
    cr.set_source_rgba(
        0x1e as f64 / 255.0,
        0x1e as f64 / 255.0,
        0x2e as f64 / 255.0,
        0.85,
    );
    let _ = cr.paint();

    let config = Config {
        font_size,
        ..Config::default()
    };

    // GL context for span shaders (e.g. <glow>).
    let gl = pwetty_box::offscreen::OffscreenGl::new().expect("surfaceless EGL");
    gl.make_current().expect("make current");

    // Faithful to the live content-tile path: render the femtovg background layer
    // first (it dirties GL state), THEN the content + span effects — so this
    // offscreen render exercises the same multi-renderer sequence as the live
    // widget, which is where GL-state bugs (like the glow glitch) surface.
    if let Ok(mut renderer) = pwetty_box::render::Renderer::new(&config, true) {
        if let Ok((rw, rh, rgba)) = renderer.capture(W as u32, H as u32, 1.0, time) {
            pwetty_box::composite_rgba(&cr, rw, rh, rgba, 1.0);
        }
    }

    let mut cache = pwetty_box::shader::ShaderCache::new();
    let mut fx = pwetty_box::EffectCtx {
        shaders: &mut cache,
        time,
        frame: 0,
        scale: 1.0,
    };
    pwetty_box::draw_content(&cr, &markup, W as f64, H as f64, &config, Some(&mut fx));

    drop(cr);
    let mut f = File::create(&out).expect("create png");
    surface.write_to_png(&mut f).expect("write png");
    eprintln!("wrote {out} ({W}x{H})");
}
