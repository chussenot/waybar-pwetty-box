//! pwetty-box — a Waybar CFFI module that renders elaborate multiline text/icon
//! tiles on the GPU.
//!
//! Pipeline: Waybar (GTK3) hands us a `GtkContainer` via the CFFI ABI. We add a
//! [`DrawingArea`](gtk::DrawingArea) and render the tiles with femtovg into an
//! offscreen image on our own surfaceless EGL context, then composite that image
//! onto the widget with Cairo.
//!
//! Why not a `GtkGLArea` (the obvious choice)? It cannot alpha-composite its GL
//! contents against a translucent bar in GTK3 — verified on GTK 3.24.52, both
//! hardware and software: transparent regions render as opaque black. Cairo
//! honors per-pixel alpha, so the offscreen-render + Cairo-composite path gives
//! true transparency against a see-through waybar. Waybar is GTK3 and exposes no
//! Vulkan surface, so OpenGL (via femtovg) remains the rendering API.

pub mod config;
pub mod gl;
pub mod offscreen;
pub mod render;
pub mod tile;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use waybar_cffi::gtk::{self, prelude::*};
use waybar_cffi::{waybar_module, InitInfo, Module};

use config::Config;
use offscreen::OffscreenGl;
use render::Renderer;

/// Live rendering state, present once the offscreen GL context is up.
struct Engine {
    gl: OffscreenGl,
    renderer: Renderer,
    start: Instant,
}

/// Shared between `init` and the draw/tick callbacks. GTK is single-threaded, so
/// `Rc`/`RefCell` suffices.
struct Shared {
    engine: RefCell<Option<Engine>>,
    config: Config,
}

pub struct PwettyBox {
    // Keep shared state alive for the module's lifetime; the GTK widget tree
    // (owned by Waybar) holds the closures that reference it.
    _shared: Rc<Shared>,
}

impl Module for PwettyBox {
    type Config = Config;

    fn init(info: &InitInfo, config: Config) -> Self {
        let container = info.get_root_widget();

        let area = gtk::DrawingArea::new();
        area.set_size_request(config.width, config.height);
        area.set_hexpand(false);
        area.set_vexpand(false);

        // Surfaceless EGL needs no window, so the engine can come up at init.
        // femtovg renders into an image target; we composite with Cairo.
        let engine = match OffscreenGl::new() {
            Ok(gl) => match Renderer::new(&config) {
                Ok(renderer) => Some(Engine {
                    gl,
                    renderer,
                    start: Instant::now(),
                }),
                Err(e) => {
                    eprintln!("pwetty-box: renderer init failed: {e:?}");
                    None
                }
            },
            Err(e) => {
                eprintln!("pwetty-box: offscreen GL init failed: {e:?}");
                None
            }
        };

        let shared = Rc::new(Shared {
            engine: RefCell::new(engine),
            config,
        });

        {
            let shared = shared.clone();
            area.connect_draw(move |area, cr| {
                if let Some(engine) = shared.engine.borrow_mut().as_mut() {
                    let scale = area.scale_factor().max(1);
                    let w = (area.allocated_width().max(1) * scale) as u32;
                    let h = (area.allocated_height().max(1) * scale) as u32;

                    if engine.gl.make_current().is_ok() {
                        let time = engine.start.elapsed().as_secs_f32();
                        if let Ok((rw, rh, rgba)) =
                            engine.renderer.capture(w, h, scale as f32, time)
                        {
                            paint_rgba(cr, rw, rh, rgba, scale as f64);
                        }
                    }
                }
                glib_propagation_proceed()
            });
        }

        // Animate by redrawing on the frame clock.
        if shared.config.fps > 0 {
            area.add_tick_callback(|area, _clock| {
                area.queue_draw();
                gtk::glib::ControlFlow::Continue
            });
        }

        container.add(&area);
        area.show();

        PwettyBox { _shared: shared }
    }
}

/// Composite an offscreen RGBA8 buffer (straight alpha, top-left origin) onto the
/// widget's Cairo context, honoring per-pixel alpha against whatever is behind
/// (e.g. a translucent bar). `device_scale` maps the device pixels we rendered at
/// back to the widget's logical coordinate space.
fn paint_rgba(cr: &gtk::cairo::Context, w: usize, h: usize, mut rgba: Vec<u8>, device_scale: f64) {
    use gtk::cairo::{Format, ImageSurface};

    // femtovg gives straight-alpha RGBA; Cairo ARGB32 wants premultiplied, in
    // native-endian byte order (little-endian: B, G, R, A).
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        let r = (px[0] as u32 * a / 255) as u8;
        let g = (px[1] as u32 * a / 255) as u8;
        let b = (px[2] as u32 * a / 255) as u8;
        px[0] = b;
        px[1] = g;
        px[2] = r;
        px[3] = a as u8;
    }

    let stride = 4 * w as i32;
    let surface =
        match ImageSurface::create_for_data(rgba, Format::ARgb32, w as i32, h as i32, stride) {
            Ok(s) => s,
            Err(_) => return,
        };

    // We rendered at device resolution; scale back so it fills the logical area.
    let s = 1.0 / device_scale;
    cr.scale(s, s);
    if cr.set_source_surface(&surface, 0.0, 0.0).is_ok() {
        let _ = cr.paint();
    }
}

#[inline]
fn glib_propagation_proceed() -> gtk::glib::Propagation {
    gtk::glib::Propagation::Proceed
}

waybar_module!(PwettyBox);
