//! Offscreen frame benchmark: N `claude` tiles drawn through the live compose
//! path ([`pwetty_box::draw_content`]) at a fixed rate, the way a bar full of
//! modules would — but with no waybar, compositor or display. Reports per-tile
//! draw time (p50/p99) and the CPU one second of frames costs.
//!
//!   EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
//!     cargo run --release --example bench_frames [-- tiles fps seconds]
//!
//! Defaults mirror the live layout: 40 tiles, 30 fps, 10 s. Tiles cycle through
//! `tiles/claude/samples/*.json`, so blinking/working/prompt/idle all appear.
//! Wrap in `perf record -g` to see where the time goes.

use std::time::{Duration, Instant};

use pwetty_box::config;
use pwetty_box::content;
use pwetty_box::shader::ShaderCache;
use pwetty_box::{draw_content, EffectCtx};
use waybar_cffi::gtk::cairo::{Context, Format, ImageSurface};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, d: u64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (tiles, fps, secs) = (arg(0, 40) as usize, arg(1, 30), arg(2, 10));

    let cfg = config::resolve(serde_json::json!({ "tile": "claude", "width": 179 }));
    let samples_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tiles/claude/samples");
    let mut samples: Vec<_> = std::fs::read_dir(samples_dir)
        .expect("samples dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    samples.sort();
    let markups: Vec<String> = samples
        .iter()
        .map(|p| {
            let data = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
            content::markup_for(&cfg, &data)
        })
        .collect();

    let (w, h) = (cfg.width, cfg.height);
    let mut cache = ShaderCache::new();
    let mut draws: Vec<Duration> = Vec::with_capacity(tiles * (fps * secs) as usize);
    let frames = fps * secs;
    let start = Instant::now();
    let mut cpu_busy = Duration::ZERO;

    for f in 0..frames {
        let t0 = Instant::now();
        let time = f as f32 / fps as f32;
        for i in 0..tiles {
            // A fresh surface per tile per frame, like GTK hands a draw callback.
            let surface = ImageSurface::create(Format::ARgb32, w, h).unwrap();
            let cr = Context::new(&surface).unwrap();
            let d0 = Instant::now();
            let mut fx = EffectCtx {
                shaders: &mut cache,
                time,
                frame: f as i32,
                scale: 1.0,
            };
            draw_content(&cr, &markups[i % markups.len()], w as f64, h as f64, &cfg, Some(&mut fx));
            draws.push(d0.elapsed());
        }
        let busy = t0.elapsed();
        cpu_busy += busy;
        // Pace to the target rate so the run resembles the live cadence.
        let budget = Duration::from_secs_f64(1.0 / fps as f64);
        if busy < budget {
            std::thread::sleep(budget - busy);
        }
    }

    draws.sort();
    let pct = |p: f64| draws[((draws.len() - 1) as f64 * p) as usize];
    let wall = start.elapsed();
    println!(
        "tiles={tiles} fps={fps} secs={secs} samples={}",
        markups.len()
    );
    println!(
        "per-tile draw: p50={:.2?} p99={:.2?} max={:.2?}",
        pct(0.5),
        pct(0.99),
        draws[draws.len() - 1]
    );
    println!(
        "busy {:.2?} of {:.2?} wall => ~{:.0}% of one core",
        cpu_busy,
        wall,
        100.0 * cpu_busy.as_secs_f64() / wall.as_secs_f64()
    );
}
