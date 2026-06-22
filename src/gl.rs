//! GL function-pointer bootstrap.
//!
//! Waybar already links `libepoxy.so.0`, which provides dynamic dispatch over
//! whatever GL/GLES implementation the EGL context resolves to. We point the
//! `epoxy` crate at that in-process library once, then femtovg's `OpenGl`
//! renderer pulls each `glXxx` pointer through `epoxy::get_proc_addr`.

use std::os::raw::c_void;
use std::sync::Once;

static LOAD: Once = Once::new();

/// Wire up `epoxy` to the process-wide libepoxy. Idempotent and cheap to call
/// from every `realize` (it only does work the first time).
pub fn ensure_loaded() {
    LOAD.call_once(|| {
        // SAFETY: we open the same libepoxy waybar already mapped. We
        // intentionally leak the handle so the resolved function pointers stay
        // valid for the lifetime of the process.
        #[cfg(target_os = "linux")]
        let lib = unsafe { libloading::Library::new("libepoxy.so.0") }
            .expect("libepoxy.so.0 should be loadable (waybar links it)");
        #[cfg(not(target_os = "linux"))]
        let lib = unsafe { libloading::Library::new("libepoxy.0.dylib") }
            .expect("libepoxy should be loadable");

        let lib = Box::leak(Box::new(lib));
        epoxy::load_with(|name| {
            // SAFETY: name comes from epoxy/femtovg asking for a known symbol.
            unsafe {
                lib.get::<*const c_void>(name.as_bytes())
                    .map(|sym| *sym)
                    .unwrap_or(std::ptr::null())
            }
        });
    });
}

/// The loader femtovg's `OpenGl::new_from_function` expects.
pub fn proc_addr(name: &str) -> *const c_void {
    epoxy::get_proc_addr(name)
}
