//! A self-owned, surfaceless EGL context (no window, no compositor).
//!
//! We render femtovg into an offscreen image with this, read it back, and
//! composite via Cairo — because a `GtkGLArea` cannot alpha-composite its
//! contents against a translucent waybar in GTK3 (verified on GTK 3.24.52,
//! hardware and software: transparent regions render opaque black). Cairo, by
//! contrast, honors per-pixel alpha.
//!
//! Surfaceless EGL uses the DRM render node directly, so it needs no Wayland
//! display, opens no `card`/DRM-master, and takes no seat.

use khronos_egl as egl;

// EGL_PLATFORM_SURFACELESS_MESA
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

pub struct OffscreenGl {
    egl: egl::Instance<egl::Static>,
    display: egl::Display,
    context: egl::Context,
}

impl OffscreenGl {
    /// Create a surfaceless GLES3 context and make it current.
    pub fn new() -> Result<Self, egl::Error> {
        let egl = egl::Instance::new(egl::Static);

        let display = unsafe {
            egl.get_platform_display(
                PLATFORM_SURFACELESS_MESA,
                std::ptr::null_mut(),
                &[egl::ATTRIB_NONE],
            )
        }?;
        egl.initialize(display)?;
        egl.bind_api(egl::OPENGL_ES_API)?;

        let config_attribs = [
            egl::SURFACE_TYPE,
            egl::PBUFFER_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES3_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let config = egl
            .choose_first_config(display, &config_attribs)?
            .ok_or(egl::Error::BadConfig)?;

        let ctx_attribs = [egl::CONTEXT_MAJOR_VERSION, 3, egl::NONE];
        let context = egl.create_context(display, config, None, &ctx_attribs)?;
        egl.make_current(display, None, None, Some(context))?;

        Ok(Self {
            egl,
            display,
            context,
        })
    }

    /// Make this context current on the calling thread.
    pub fn make_current(&self) -> Result<(), egl::Error> {
        self.egl
            .make_current(self.display, None, None, Some(self.context))
    }
}

impl Drop for OffscreenGl {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}
