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
pub mod content;
pub mod gl;
pub mod markup;
pub mod offscreen;
pub mod render;
pub mod shader;
pub mod text;
pub mod tile;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

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
    /// Optional tile-level background shader (path + lazily compiled + mtime).
    shader_path: Option<String>,
    shader: Option<shader::ShaderPass>,
    shader_mtime: Option<SystemTime>,
    frame: i32,
}

impl Engine {
    /// Compile the background shader (or recompile if the file changed). A GL
    /// context must be current. Compile errors are logged and leave no shader.
    fn refresh_shader(&mut self) {
        let Some(path) = self.shader_path.clone() else {
            return;
        };
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if self.shader.is_some() && mtime == self.shader_mtime {
            return; // unchanged
        }
        self.shader_mtime = mtime;
        match std::fs::read_to_string(&path) {
            Ok(src) => match shader::ShaderPass::new(&src) {
                Ok(p) => self.shader = Some(p),
                Err(e) => {
                    eprintln!("pwetty-box: shader compile error in '{path}':\n{e}");
                    self.shader = None;
                }
            },
            Err(e) => {
                eprintln!("pwetty-box: cannot read shader '{path}': {e}");
                self.shader = None;
            }
        }
    }
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

        // Content source (static text / refreshing command), if configured.
        let content = content::from_config(&config);

        // Surfaceless EGL needs no window, so the engine can come up at init.
        // femtovg renders into an image target; we composite with Cairo.
        let engine = match OffscreenGl::new() {
            Ok(gl) => match Renderer::new(&config, content.is_some()) {
                Ok(renderer) => Some(Engine {
                    gl,
                    renderer,
                    start: Instant::now(),
                    shader_path: config.background_shader.clone(),
                    shader: None,
                    shader_mtime: None,
                    frame: 0,
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
            let content_draw = content.clone();
            area.connect_draw(move |area, cr| {
                let scale = area.scale_factor().max(1);

                // Layer 1: the GPU background, composited via Cairo. It's either a
                // tile-level shader (when `background_shader` is set) or the
                // femtovg layer (the demo tile / background colour).
                if let Some(engine) = shared.engine.borrow_mut().as_mut() {
                    let w = (area.allocated_width().max(1) * scale) as u32;
                    let h = (area.allocated_height().max(1) * scale) as u32;
                    if engine.gl.make_current().is_ok() {
                        let time = engine.start.elapsed().as_secs_f32();
                        engine.refresh_shader();
                        let frame = engine.frame;
                        let bg: Option<Vec<u8>> = if let Some(sh) = engine.shader.as_mut() {
                            Some(sh.render(w as i32, h as i32, time, frame))
                        } else if engine.shader_path.is_none() {
                            engine
                                .renderer
                                .capture(w, h, scale as f32, time)
                                .ok()
                                .map(|(_, _, rgba)| rgba)
                        } else {
                            None // shader configured but failed to compile
                        };
                        if engine.shader.is_some() {
                            engine.frame = engine.frame.wrapping_add(1);
                        }
                        if let Some(rgba) = bg {
                            paint_rgba(cr, w as usize, h as usize, rgba, scale as f64);
                        }
                    }
                }

                // Layer 2: the Pango text layer, drawn in logical coordinates
                // (GTK rasterizes the Cairo context at device resolution, so the
                // text stays crisp). Per-pixel alpha composites over layer 1.
                if let Some(store) = &content_draw {
                    let w = area.allocated_width().max(1) as f64;
                    let h = area.allocated_height().max(1) as f64;
                    draw_content(cr, &store.markup(), w, h, &shared.config);
                }

                glib_propagation_proceed()
            });
        }

        // Animate by redrawing on the frame clock (for fps>0 or a live shader).
        if shared.config.fps > 0 || shared.config.background_shader.is_some() {
            area.add_tick_callback(|area, _clock| {
                area.queue_draw();
                gtk::glib::ControlFlow::Continue
            });
        }

        // Redraw when a content source publishes new content (e.g. a command
        // refresh). Cheap poll of the dirty flag — content tiles can set fps: 0.
        if let Some(store) = content {
            let area = area.clone();
            gtk::glib::timeout_add_local(Duration::from_millis(150), move || {
                if store.take_dirty() {
                    area.queue_draw();
                }
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
    // save/restore so this transform doesn't leak into the text layer.
    let _ = cr.save();
    let s = 1.0 / device_scale;
    cr.scale(s, s);
    if cr.set_source_surface(&surface, 0.0, 0.0).is_ok() {
        let _ = cr.paint();
    }
    let _ = cr.restore();
}

/// Custom effect tags routed away from Pango (see [`markup`]).
const EFFECT_TAGS: &[&str] = &["box"];

/// Draw the content's Pango markup onto `cr` within a `w`×`h` logical tile,
/// rendering any custom effect tags (currently `<box>`) behind the text.
/// Public so offscreen vision harnesses can exercise the exact compose path.
pub fn draw_content(
    cr: &gtk::cairo::Context,
    content_markup: &str,
    w: f64,
    h: f64,
    config: &Config,
) {
    let processed = markup::process(content_markup, EFFECT_TAGS);

    let style = text::TextStyle {
        font_family: "sans".into(),
        size_px: config.font_size as f64,
        color: (0.95, 0.95, 1.0, 1.0),
        align_center: false,
    };

    let (layout, ox, oy) = text::layout(cr, &processed.markup, w, h, &style);

    // Effects render behind the text.
    for effect in &processed.effects {
        if effect.tag == "box" {
            let rect = text::span_rect(&layout, ox, oy, effect.start, effect.end);
            draw_box(cr, rect, &effect.attrs);
        }
    }

    text::paint(cr, &layout, ox, oy, &style);
}

/// Draw a `<box bg="#rrggbb[aa]">` rounded highlight behind a text span.
fn draw_box(cr: &gtk::cairo::Context, rect: text::Rect, attrs: &[(String, String)]) {
    let (x, y, w, h) = rect;
    let pad = 4.0;
    let (rx, ry, rw, rh) = (x - pad, y - pad, w + 2.0 * pad, h + 2.0 * pad);

    let (r, g, b, a) = attrs
        .iter()
        .find(|(k, _)| k == "bg")
        .and_then(|(_, v)| render::parse_hex_color(v))
        .map(|c| (c.r as f64, c.g as f64, c.b as f64, c.a as f64))
        .unwrap_or((0.35, 0.45, 0.85, 0.55));

    rounded_rect(cr, rx, ry, rw, rh, rh * 0.32);
    cr.set_source_rgba(r, g, b, a);
    let _ = cr.fill();
}

/// Append a rounded-rectangle subpath to `cr`.
fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

#[inline]
fn glib_propagation_proceed() -> gtk::glib::Propagation {
    gtk::glib::Propagation::Proceed
}

waybar_module!(PwettyBox);
