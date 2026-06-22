//! Tile content and where it comes from.
//!
//! A tile's content is a **Pango-markup string** (which may also contain custom
//! effect tags — see [`crate::markup`]). It's produced by a *source*:
//! - **static**: a fixed `text` value;
//! - **command**: the stdout of a shell command, re-run on an interval.
//!
//! The value is substituted into the `format` markup template (escaped, so
//! command output can't inject markup) and an optional `icon` glyph is
//! prepended. Commands run on a background thread and publish into a
//! [`ContentStore`], so a slow command never blocks the GTK main loop; the
//! widget polls the dirty flag and redraws when it flips.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::markup;

/// What a tile currently displays: a Pango-markup string.
#[derive(Debug, Clone, Default)]
pub struct TileContent {
    pub markup: String,
}

/// Thread-safe, cloneable handle to the current tile content.
#[derive(Clone)]
pub struct ContentStore {
    inner: Arc<Inner>,
}

struct Inner {
    content: Mutex<TileContent>,
    dirty: AtomicBool,
}

impl ContentStore {
    pub fn new(initial: TileContent) -> Self {
        Self {
            inner: Arc::new(Inner {
                content: Mutex::new(initial),
                dirty: AtomicBool::new(true),
            }),
        }
    }

    /// Replace the content and mark it dirty (a redraw is due).
    pub fn set(&self, content: TileContent) {
        if let Ok(mut guard) = self.inner.content.lock() {
            *guard = content;
        }
        self.inner.dirty.store(true, Ordering::Release);
    }

    /// The current markup string (cheap clone for per-frame paint).
    pub fn markup(&self) -> String {
        self.inner
            .content
            .lock()
            .map(|g| g.markup.clone())
            .unwrap_or_default()
    }

    /// Clear and return the dirty flag — true if content changed since last call.
    pub fn take_dirty(&self) -> bool {
        self.inner.dirty.swap(false, Ordering::AcqRel)
    }
}

/// How much larger than the base text an icon glyph is drawn.
const ICON_SCALE: f64 = 1.3;

/// Build a tile's markup: substitute `value` into the `format` markup template
/// (value escaped) and prepend the `icon` glyph if set. The icon is wrapped so
/// it renders larger and vertically centered on the text (`base_px` = text size).
fn build_markup(format: &str, icon: &Option<String>, value: &str, base_px: f64) -> String {
    let body = markup::apply_format(format, value);
    match icon {
        Some(i) if !i.is_empty() => {
            format!("{}  {body}", markup::icon_span(i, base_px, ICON_SCALE))
        }
        _ => body,
    }
}

/// Build a [`ContentStore`] for the configured source, if any (`text`/`exec`).
/// For `exec`, spawns a background refresh thread. Returns `None` when no content
/// source is configured (the caller falls back to the demo tile).
pub fn from_config(config: &Config) -> Option<ContentStore> {
    let format = config.format.clone().unwrap_or_else(|| "{}".to_string());
    let icon = config.icon.clone();
    let base_px = config.font_size as f64;

    if let Some(exec) = config.exec.clone() {
        let store = ContentStore::new(TileContent::default());
        let interval = config.interval;
        let publish = store.clone();
        // Detached: lives for the process (waybar modules are process-lifetime).
        thread::spawn(move || loop {
            let raw = run_command(&exec);
            publish.set(TileContent {
                markup: build_markup(&format, &icon, &raw, base_px),
            });
            if interval == 0 {
                break;
            }
            thread::sleep(Duration::from_secs(interval));
        });
        return Some(store);
    }

    config.text.as_deref().map(|text| {
        ContentStore::new(TileContent {
            markup: build_markup(&format, &icon, text, base_px),
        })
    })
}

/// Run `cmd` via `sh -c` and return its trimmed stdout (empty on failure).
fn run_command(cmd: &str) -> String {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{build_markup, ContentStore, TileContent};

    #[test]
    fn build_markup_substitutes_escapes_and_prefixes_icon() {
        // Value is escaped into the (markup) template.
        assert_eq!(build_markup("{}", &None, "a&b", 30.0), "a&amp;b");
        // Template markup is preserved around the escaped value.
        assert_eq!(build_markup("<b>{}</b>", &None, "x", 30.0), "<b>x</b>");
        // Icon glyph is prepended as a (centered) span, body follows.
        let with_icon = build_markup("{}", &Some("I".into()), "x", 30.0);
        assert!(with_icon.contains("<span"), "icon wrapped: {with_icon}");
        assert!(with_icon.ends_with("x"), "body follows icon: {with_icon}");
        // Empty icon is ignored.
        assert_eq!(build_markup("{}", &Some(String::new()), "x", 30.0), "x");
    }

    #[test]
    fn store_tracks_dirty() {
        let s = ContentStore::new(TileContent::default());
        assert!(s.take_dirty(), "new store starts dirty");
        assert!(!s.take_dirty(), "cleared after read");
        s.set(TileContent { markup: "x".into() });
        assert!(s.take_dirty(), "dirty again after set");
        assert_eq!(s.markup(), "x");
    }
}
