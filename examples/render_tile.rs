//! Offscreen, software-only render of the tile to a PNG.
//!
//! Uses the crate's surfaceless [`OffscreenGl`] context (no window, no
//! compositor, no Wayland connection), renders the DemoTile with femtovg into an
//! offscreen image, and writes a PNG. Run it forced to software GL so it never
//! touches the GPU or the display server:
//!
//!   EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
//!     cargo run --example render_tile [-- out.png [seconds]]
//!
//! It spawns no compositor and opens no /dev/dri or Wayland socket, so it
//! cannot affect a running desktop session.

use pwetty_box::config::Config;
use pwetty_box::offscreen::OffscreenGl;
use pwetty_box::render::Renderer;

fn main() {
    let scale = 2.0f32; // 2x for crisp text
    let cfg = Config::default();
    let w = (cfg.width as f32 * scale) as u32;
    let h = (cfg.height as f32 * scale) as u32;

    // Animation phase (seconds) from arg 2, so we can capture different frames.
    let time: f32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);

    let gl = OffscreenGl::new().expect("surfaceless EGL — is Mesa present?");
    gl.make_current().expect("eglMakeCurrent");
    eprintln!("rendering {w}x{h} @ t={time}s");

    let mut renderer = Renderer::new(&cfg).expect("femtovg renderer");
    let (rw, rh, rgba) = renderer.capture(w, h, scale, time).expect("capture");
    // Drop the renderer (frees GL objects) while the context is still current.
    drop(renderer);

    // Composite over the bar background (#1e1e2e) so the PNG looks like the bar.
    let bg = [0x1e_u8, 0x1e, 0x2e];
    let mut out_buf = Vec::with_capacity(rw * rh * 4);
    for px in rgba.chunks_exact(4) {
        let af = px[3] as f32 / 255.0;
        for c in 0..3 {
            out_buf.push(((px[c] as f32 * af) + (bg[c] as f32 * (1.0 - af))) as u8);
        }
        out_buf.push(255);
    }

    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/claude-1000/tile-offscreen.png".into());
    image::RgbaImage::from_raw(rw as u32, rh as u32, out_buf)
        .expect("image from raw")
        .save(&out)
        .expect("save png");
    eprintln!("wrote {out} ({rw}x{rh})");
}
