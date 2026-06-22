//! Offscreen vision harness for the CONTENT path (Pango markup + custom `<box>`
//! effect), rendered via the exact `draw_content` compose used in the live
//! widget — but onto a Cairo image surface, so it's pure CPU and safe anywhere.
//!
//!   cargo run --example render_content -- out.png "<markup>" [font_size]
//!
//! Default markup exercises spans (color/size/weight), multiline, and a `<box>`.

use pwetty_box::config::Config;

fn main() {
    // Tile size: defaults to a roomy 760×150, overridable via env so the same
    // harness can vision-check real tile dimensions (e.g. 300×50).
    let w: i32 = std::env::var("PWETTY_W")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(760);
    let h: i32 = std::env::var("PWETTY_H")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(150);
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

    let config = Config {
        width: w,
        height: h,
        font_size,
        font_family: std::env::var("PWETTY_FONT").ok(),
        align: std::env::var("PWETTY_ALIGN").ok(),
        ..Config::default()
    };

    // Render via the shared offscreen compose path used by the live widget and
    // the `pwetty` CLI (femtovg background + Cairo content on a dark backing).
    pwetty_box::render_png(&config, &markup, time, std::path::Path::new(&out)).expect("render");
    eprintln!("wrote {out} ({w}x{h})");
}
