//! Offscreen vision harness for a tile background shader: compile a Shadertoy-
//! style `.glsl` file and render one frame (fixed `iTime`) to a PNG. Pure GPU
//! offscreen (surfaceless EGL) — no compositor, safe anywhere.
//!
//!   cargo run --example render_shader -- out.png examples/shaders/aurora.glsl [time]

use pwetty_box::offscreen::OffscreenGl;
use pwetty_box::shader::ShaderPass;

const W: i32 = 760;
const H: i32 = 150;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/claude-1000/shader-sample.png".into());
    let path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "examples/shaders/aurora.glsl".into());
    let time: f32 = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.5);

    // Extra float uniforms as `name=value` args (4th onward), e.g. u_load=0.9.
    let uniforms: Vec<(String, f32)> = std::env::args()
        .skip(4)
        .filter_map(|a| {
            let (n, v) = a.split_once('=')?;
            Some((n.to_string(), v.parse().ok()?))
        })
        .collect();

    let src = std::fs::read_to_string(&path).expect("read shader file");

    let gl = OffscreenGl::new().expect("surfaceless EGL");
    gl.make_current().expect("make current");

    let mut pass = ShaderPass::new(&src).unwrap_or_else(|e| panic!("shader compile:\n{e}"));
    let rgba = pass.render(W, H, time, 0, &uniforms);

    image::RgbaImage::from_raw(W as u32, H as u32, rgba)
        .expect("image from raw")
        .save(&out)
        .expect("save png");
    eprintln!("wrote {out} ({W}x{H}) @ t={time}");
}
