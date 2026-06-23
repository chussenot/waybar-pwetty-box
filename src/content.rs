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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use crate::config::Config;
use crate::markup;

/// What a tile currently displays: a Pango-markup string and the float uniforms
/// (resolved from the data) for the background shader.
#[derive(Debug, Clone, Default)]
pub struct TileContent {
    pub markup: String,
    pub uniforms: Vec<(String, f32)>,
}

/// Thread-safe, cloneable handle to the current tile content.
#[derive(Clone)]
pub struct ContentStore {
    inner: Arc<Inner>,
}

struct Inner {
    content: Mutex<TileContent>,
    dirty: AtomicBool,
    /// Whether the current content has a *moving* element (blinking status,
    /// `<pulse>`, or a marquee) — i.e. whether it needs per-frame redraws. A
    /// static tile (idle/empty/plain wrapped text) is `false` and only repaints
    /// on a content change (the dirty flag), keeping the bar cool.
    animating: AtomicBool,
}

/// Whether rendered tile `markup` contains a continuously-animated element.
/// Idle/`empty`/`shell`-less static content is not animated; a blinking status
/// (`working`/`prompt`/`shell`), a `<pulse>`, or a `<tickerbox>` is.
pub fn content_animates(markup: &str) -> bool {
    markup.contains("state=\"working\"")
        || markup.contains("state=\"prompt\"")
        || markup.contains("state=\"shell\"")
        || markup.contains("<pulse")
        || markup.contains("<tickerbox")
}

impl ContentStore {
    pub fn new(initial: TileContent) -> Self {
        let animating = content_animates(&initial.markup);
        Self {
            inner: Arc::new(Inner {
                content: Mutex::new(initial),
                dirty: AtomicBool::new(true),
                animating: AtomicBool::new(animating),
            }),
        }
    }

    /// Replace the content and mark it dirty (a redraw is due).
    pub fn set(&self, content: TileContent) {
        self.inner
            .animating
            .store(content_animates(&content.markup), Ordering::Release);
        if let Ok(mut guard) = self.inner.content.lock() {
            *guard = content;
        }
        self.inner.dirty.store(true, Ordering::Release);
    }

    /// Whether the current content animates (see [`content_animates`]).
    pub fn animating(&self) -> bool {
        self.inner.animating.load(Ordering::Acquire)
    }

    /// The current markup string (cheap clone for per-frame paint).
    pub fn markup(&self) -> String {
        self.inner
            .content
            .lock()
            .map(|g| g.markup.clone())
            .unwrap_or_default()
    }

    /// The current shader uniform values (resolved from the data).
    pub fn uniforms(&self) -> Vec<(String, f32)> {
        self.inner
            .content
            .lock()
            .map(|g| g.uniforms.clone())
            .unwrap_or_default()
    }

    /// Clear and return the dirty flag — true if content changed since last call.
    pub fn take_dirty(&self) -> bool {
        self.inner.dirty.swap(false, Ordering::AcqRel)
    }
}

/// How much larger than the base text an icon glyph is drawn.
const ICON_SCALE: f64 = 1.3;

/// Parse a command's output as JSON; if it isn't valid JSON, treat the whole
/// output as a string value (so plain-text commands still work via `{{ value }}`).
fn parse_data(output: &str) -> Value {
    serde_json::from_str(output).unwrap_or_else(|_| Value::String(output.to_string()))
}

/// Parse a resolved uniform value: `true`/`false` → 1/0, otherwise a float
/// (unparseable → 0).
fn to_f32(s: &str) -> f32 {
    match s.trim() {
        "true" => 1.0,
        "false" => 0.0,
        other => other.parse().unwrap_or(0.0),
    }
}

/// Resolve each `shader_uniforms` template against the data into a float.
fn build_uniforms(spec: &HashMap<String, String>, data: &Value) -> Vec<(String, f32)> {
    spec.iter()
        .filter_map(|(name, tmpl)| {
            markup::render_template(tmpl, data)
                .ok()
                .map(|s| (name.clone(), to_f32(&s)))
        })
        .collect()
}

/// Build a tile's markup: render the `template` against `data`, then prepend the
/// `icon` glyph if set (sized + vertically centered; `base_px` = text size).
/// A template error is surfaced inline so it's visible on the bar.
fn build_markup(template: &str, icon: &Option<String>, data: &Value, base_px: f64) -> String {
    let body = match markup::render_template(template, data) {
        Ok(s) => s,
        Err(e) => format!(
            "<span foreground=\"#f38ba8\">template error: {}</span>",
            markup::escape(&e.to_string())
        ),
    };
    match icon {
        Some(i) if !i.is_empty() => {
            format!("{}  {body}", markup::icon_span(i, base_px, ICON_SCALE))
        }
        _ => body,
    }
}

/// Render a tile's markup for `config` against arbitrary `data` — the same
/// template + icon composition the live content path uses, but one-shot. Used by
/// the `pwetty` CLI to render a tile against a sample payload.
pub fn markup_for(config: &Config, data: &Value) -> String {
    let template = config.format.as_deref().unwrap_or("{{ value }}");
    build_markup(template, &config.icon, data, config.font_size as f64)
}

/// Build a [`ContentStore`] for the configured source, if any (`text`/`exec`).
/// For `exec`, spawns a background refresh thread. Returns `None` when no content
/// source is configured (the caller falls back to the demo tile).
pub fn from_config(config: &Config) -> Option<ContentStore> {
    let template = config
        .format
        .clone()
        .unwrap_or_else(|| "{{ value }}".to_string());
    let icon = config.icon.clone();
    let base_px = config.font_size as f64;
    let uniforms = config.shader_uniforms.clone().unwrap_or_default();

    if let Some(exec) = config.exec.clone() {
        let store = ContentStore::new(TileContent::default());
        let interval = config.interval;
        let publish = store.clone();
        let (template, icon, uniforms) = (template.clone(), icon.clone(), uniforms.clone());
        // Detached: lives for the process (waybar modules are process-lifetime).
        thread::spawn(move || loop {
            let data = parse_data(&run_command(&exec));
            publish.set(TileContent {
                markup: build_markup(&template, &icon, &data, base_px),
                uniforms: build_uniforms(&uniforms, &data),
            });
            if interval == 0 {
                break;
            }
            thread::sleep(Duration::from_secs(interval));
        });
        return Some(store);
    }

    config.text.as_deref().map(|text| {
        let data = parse_data(text);
        ContentStore::new(TileContent {
            markup: build_markup(&template, &icon, &data, base_px),
            uniforms: build_uniforms(&uniforms, &data),
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
    use super::{build_markup, build_uniforms, parse_data, ContentStore, TileContent};

    #[test]
    fn build_markup_binds_data_and_prefixes_icon() {
        use serde_json::json;
        // Scalar value bound via {{ value }}, escaped.
        assert_eq!(
            build_markup("{{ value }}", &None, &json!("a&b"), 30.0),
            "a&amp;b"
        );
        // Object field bound; literal markup preserved.
        assert_eq!(
            build_markup("<b>{{ n }}</b>", &None, &json!({ "n": 42 }), 30.0),
            "<b>42</b>"
        );
        // Icon glyph prepended as a (centered) span, body follows.
        let with_icon = build_markup("{{ value }}", &Some("I".into()), &json!("x"), 30.0);
        assert!(with_icon.contains("<span"), "icon wrapped: {with_icon}");
        assert!(with_icon.ends_with("x"), "body follows icon: {with_icon}");
        // Empty icon is ignored.
        assert_eq!(
            build_markup("{{ value }}", &Some(String::new()), &json!("x"), 30.0),
            "x"
        );
    }

    #[test]
    fn parse_data_json_or_plain() {
        assert!(parse_data(r#"{"a":1}"#).is_object());
        assert_eq!(
            parse_data("hello"),
            serde_json::Value::String("hello".into())
        );
    }

    #[test]
    fn store_tracks_dirty() {
        let s = ContentStore::new(TileContent::default());
        assert!(s.take_dirty(), "new store starts dirty");
        assert!(!s.take_dirty(), "cleared after read");
        s.set(TileContent {
            markup: "x".into(),
            ..Default::default()
        });
        assert!(s.take_dirty(), "dirty again after set");
        assert_eq!(s.markup(), "x");
    }

    #[test]
    fn build_uniforms_resolves_floats() {
        use serde_json::json;
        use std::collections::HashMap;
        let mut spec = HashMap::new();
        spec.insert("u_load".to_string(), "{{ pct }}".to_string());
        spec.insert("u_hot".to_string(), "{{ pct >= 90 }}".to_string());
        let u = build_uniforms(&spec, &json!({ "pct": 95 }));
        let get = |n: &str| u.iter().find(|(k, _)| k == n).map(|(_, v)| *v);
        assert_eq!(get("u_load"), Some(95.0));
        assert_eq!(get("u_hot"), Some(1.0)); // bool true -> 1.0
    }
}
