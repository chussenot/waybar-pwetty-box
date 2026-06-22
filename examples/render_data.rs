//! Offscreen vision harness for the DATA path: bind a JSON data object to a
//! minijinja template, then render the result via `draw_content`. Pure CPU
//! (Cairo + Pango), safe to run anywhere.
//!
//!   cargo run --example render_data -- out.png 'TEMPLATE' 'JSON' [font_size]

use std::fs::File;

use pwetty_box::config::Config;
use waybar_cffi::gtk::cairo::{Context, Format, ImageSurface};

const W: i32 = 760;
const H: i32 = 210;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/claude-1000/data-sample.png".into());
    let template = std::env::args().nth(2).unwrap_or_else(|| {
        "<span size='xx-large' weight='bold'>{{ host }}</span>\n\
         <span foreground='{{ cpu.color }}'>CPU {{ cpu.pct }}%</span>  \
         <span foreground='{{ mem.color }}'>MEM {{ mem.used }}</span>\n\
         <span size='small' foreground='#9399b2'>↓ {{ net.down }}  ↑ {{ net.up }} MB/s</span>\
         {% if cpu.pct >= 90 %}  <span foreground='#f38ba8' weight='bold'>⚠</span>{% endif %}"
            .into()
    });
    let json = std::env::args().nth(3).unwrap_or_else(|| {
        r##"{"host":"nas","cpu":{"pct":82,"color":"#fab387"},
            "mem":{"used":"7.1G","color":"#a6e3a1"},
            "net":{"down":"1.2","up":"0.3"}}"##
            .into()
    });
    let font_size: f32 = std::env::args()
        .nth(4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(26.0);

    let data: serde_json::Value = serde_json::from_str(&json).expect("valid JSON arg");
    let markup = pwetty_box::markup::render_template(&template, &data).expect("template render");

    let surface = ImageSurface::create(Format::ARgb32, W, H).expect("surface");
    let cr = Context::new(&surface).expect("cairo context");
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
    pwetty_box::draw_content(&cr, &markup, W as f64, H as f64, &config);

    drop(cr);
    let mut f = File::create(&out).expect("create png");
    surface.write_to_png(&mut f).expect("write png");
    eprintln!("wrote {out} ({W}x{H})\n--- bound markup ---\n{markup}");
}
