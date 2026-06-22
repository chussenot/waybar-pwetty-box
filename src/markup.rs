//! Tile content markup processing.
//!
//! Tile content is Pango markup that may also contain *custom* effect tags
//! (e.g. `<box>`, `<glow>`, `<shader>`). Pango's parser rejects unknown tags, so
//! we split them out here: standard tags pass through to Pango untouched, custom
//! tags become [`EffectSpan`]s our own renderers handle, while their inner text
//! still flows into the laid-out text so Pango positions it.
//!
//! These functions are GTK-free, so they unit-test cleanly.

use minijinja::{AutoEscape, Environment};
use serde_json::Value;

/// A custom (non-Pango) tag, covering a byte range of the laid-out plain text
/// (offsets into [`Processed::plain`], which matches Pango's layout text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSpan {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub start: usize,
    pub end: usize,
}

/// An inline embed — a sized element placed *in* the text flow (e.g.
/// `<tickerbox>`). It reserves a box (via a Pango shape attribute over a single
/// placeholder char at `start`) that surrounding text flows around; its `inner`
/// markup is drawn into that box by the caller, not laid out inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Embed {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    /// Inner Pango markup, rendered into the reserved box.
    pub inner: String,
    /// Byte offset (in [`Processed::plain`]/markup) of the placeholder char.
    pub start: usize,
}

/// The placeholder character an embed occupies in the laid-out text (the Unicode
/// object-replacement character, 3 bytes in UTF-8).
pub const EMBED_PLACEHOLDER: char = '\u{FFFC}';

/// Result of processing tile content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Processed {
    /// Pango-safe markup (standard tags only) to hand to Pango.
    pub markup: String,
    /// Plain text with all tags removed; effect/embed offsets index this.
    pub plain: String,
    /// Custom effect tags (decorations behind a span).
    pub effects: Vec<EffectSpan>,
    /// Inline embeds (sized elements placed in the text flow).
    pub embeds: Vec<Embed>,
    /// Whether the content was wrapped in `<pulse>…</pulse>` — the whole tile's
    /// opacity should oscillate (an attention signal). The wrapper itself emits
    /// no markup; its children render normally.
    pub pulse: bool,
    /// Whether the content contained an `<active/>` marker — this desktop is the
    /// focused one; the renderer draws an accent panel behind the tile.
    pub active: bool,
}

/// Structural tag that flags the whole tile to pulse (see [`Processed::pulse`]).
pub const PULSE_TAG: &str = "pulse";
/// Structural marker tag that flags the tile as the active desktop.
pub const ACTIVE_TAG: &str = "active";

/// Escape for safe insertion into Pango markup — text *and* attribute values
/// (hence quotes too).
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Render a tile `template` (Jinja-ish: `{{ expr }}`, `{% if %}`, filters)
/// against the JSON `data`, producing a Pango-markup string. Bound values are
/// HTML/XML-autoescaped, so they're safe in markup text and attributes alike,
/// while the template's own markup is left intact.
///
/// A JSON object exposes its fields at the top level (`{{ host }}`,
/// `{{ cpu.pct }}`); any non-object value (e.g. a plain-text command's output)
/// is available as `{{ value }}`.
pub fn render_template(template: &str, data: &Value) -> Result<String, minijinja::Error> {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    if data.is_object() {
        env.render_str(template, data)
    } else {
        env.render_str(template, minijinja::context! { value => data })
    }
}

/// Pango units per device pixel (a stable Pango constant).
const PANGO_SCALE: f64 = 1024.0;

/// Wrap an `icon` glyph in a Pango span sized `scale`× the base text and lowered
/// (via `rise`) so it sits **vertically centered** on the text line instead of
/// on the shared baseline — where a larger glyph would otherwise ride high.
/// `base_px` is the tile's base text size in pixels.
pub fn icon_span(icon: &str, base_px: f64, scale: f64) -> String {
    let pct = (scale * 100.0).round().max(1.0) as i32;
    // Lower the glyph by half the extra height the larger size adds, so the
    // scaled glyph box centers on the base text box (rise is up-positive).
    let rise = (-((scale - 1.0) / 2.0) * base_px * PANGO_SCALE).round() as i32;
    format!(
        "<span size=\"{pct}%\" rise=\"{rise}\">{}</span>",
        escape(icon)
    )
}

/// Process tile content into Pango-safe markup plus extracted effects and embeds.
/// `effect_tags` are decorations drawn behind a span (e.g. `box`, `glow`);
/// `embed_tags` are inline sized elements (e.g. `tickerbox`) — each becomes one
/// placeholder char in the flow (reserved via a shape attribute by the caller),
/// with its inner markup pulled out to render into the reserved box. Everything
/// else is standard Pango markup. Malformed input falls back to escaped text.
pub fn process(content: &str, effect_tags: &[&str], embed_tags: &[&str]) -> Processed {
    let wrapped = format!("<r>{content}</r>");
    let doc = match roxmltree::Document::parse(&wrapped) {
        Ok(doc) => doc,
        Err(_) => {
            return Processed {
                markup: escape(content),
                plain: content.to_string(),
                ..Default::default()
            };
        }
    };

    let mut out = Processed::default();
    walk_children(doc.root_element(), effect_tags, embed_tags, &mut out);
    out
}

fn collect_attrs(node: roxmltree::Node) -> Vec<(String, String)> {
    node.attributes()
        .map(|a| (a.name().to_string(), a.value().to_string()))
        .collect()
}

/// Re-serialize an element's children as standard Pango markup — for an embed's
/// inner content (rendered separately). Nested tags are treated as standard.
fn serialize_inner(node: roxmltree::Node) -> String {
    let mut s = String::new();
    for child in node.children() {
        if child.is_text() {
            s.push_str(&escape(child.text().unwrap_or("")));
        } else if child.is_element() {
            let tag = child.tag_name().name();
            s.push('<');
            s.push_str(tag);
            for attr in child.attributes() {
                s.push(' ');
                s.push_str(attr.name());
                s.push_str("=\"");
                s.push_str(&escape(attr.value()));
                s.push('"');
            }
            s.push('>');
            s.push_str(&serialize_inner(child));
            s.push_str("</");
            s.push_str(tag);
            s.push('>');
        }
    }
    s
}

/// Recursively walk the children of `node`, appending to `out`.
fn walk_children(
    node: roxmltree::Node,
    effect_tags: &[&str],
    embed_tags: &[&str],
    out: &mut Processed,
) {
    for child in node.children() {
        if child.is_text() {
            let text = child.text().unwrap_or("");
            out.markup.push_str(&escape(text));
            out.plain.push_str(text);
        } else if child.is_element() {
            let tag = child.tag_name().name();
            if tag == ACTIVE_TAG {
                // Active-desktop marker: flag it; render any children normally.
                out.active = true;
                walk_children(child, effect_tags, embed_tags, out);
            } else if tag == PULSE_TAG {
                // Attention wrapper: flag the tile to pulse; emit no tag of its
                // own — its children render normally.
                out.pulse = true;
                walk_children(child, effect_tags, embed_tags, out);
            } else if embed_tags.contains(&tag) {
                // Inline embed: one placeholder char in the flow; inner pulled out.
                let start = out.plain.len();
                out.markup.push(EMBED_PLACEHOLDER);
                out.plain.push(EMBED_PLACEHOLDER);
                out.embeds.push(Embed {
                    tag: tag.to_string(),
                    attrs: collect_attrs(child),
                    inner: serialize_inner(child),
                    start,
                });
            } else if effect_tags.contains(&tag) {
                // Custom effect tag: don't emit the tag, only its children.
                let start = out.plain.len();
                walk_children(child, effect_tags, embed_tags, out);
                let end = out.plain.len();
                out.effects.push(EffectSpan {
                    tag: tag.to_string(),
                    attrs: collect_attrs(child),
                    start,
                    end,
                });
            } else {
                // Standard Pango tag: re-serialize and recurse.
                out.markup.push('<');
                out.markup.push_str(tag);
                for attr in child.attributes() {
                    out.markup.push(' ');
                    out.markup.push_str(attr.name());
                    out.markup.push_str("=\"");
                    out.markup.push_str(&escape(attr.value()));
                    out.markup.push('"');
                }
                out.markup.push('>');
                walk_children(child, effect_tags, embed_tags, out);
                out.markup.push_str("</");
                out.markup.push_str(tag);
                out.markup.push('>');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_special_chars() {
        assert_eq!(escape("a & b < c > d"), "a &amp; b &lt; c &gt; d");
        assert_eq!(escape("plain"), "plain");
        // Quotes too, so values are safe inside attributes.
        assert_eq!(escape(r#"q"' "#), "q&quot;&#39; ");
    }

    #[test]
    fn inline_embed_reserves_placeholder_and_pulls_inner() {
        // A label, an inline tickerbox embed, and a trailing value.
        let p = process(
            r#"<b>NOW</b> <tickerbox width="200"><b>x</b> y</tickerbox> z"#,
            &["box"],
            &["tickerbox"],
        );
        // The embed becomes one placeholder char in the flow; surrounding markup
        // (the bold label, the trailing text) is preserved.
        assert!(p.markup.starts_with("<b>NOW</b> "));
        assert!(p.markup.ends_with(" z"));
        assert!(p.markup.contains(EMBED_PLACEHOLDER));
        assert_eq!(p.effects.len(), 0);
        assert_eq!(p.embeds.len(), 1);

        let e = &p.embeds[0];
        assert_eq!(e.tag, "tickerbox");
        assert_eq!(e.attrs, vec![("width".to_string(), "200".to_string())]);
        assert_eq!(e.inner, "<b>x</b> y"); // inner markup pulled out verbatim
                                           // The placeholder sits where "NOW " ends in the plain text.
        assert_eq!(
            &p.plain[e.start..e.start + EMBED_PLACEHOLDER.len_utf8()],
            "\u{FFFC}"
        );
    }

    #[test]
    fn template_binds_object_fields() {
        let data = serde_json::json!({ "host": "nas", "cpu": { "pct": 82 } });
        assert_eq!(render_template("{{ host }}", &data).unwrap(), "nas");
        assert_eq!(
            render_template("CPU {{ cpu.pct }}%", &data).unwrap(),
            "CPU 82%"
        );
    }

    #[test]
    fn template_autoescapes_values_but_not_markup() {
        let data = serde_json::json!({ "v": "a<b&c" });
        let out = render_template("<span>{{ v }}</span>", &data).unwrap();
        // Literal markup preserved; the value's special chars escaped.
        assert!(
            out.starts_with("<span>") && out.ends_with("</span>"),
            "{out}"
        );
        assert!(out.contains("a&lt;b&amp;c"), "value escaped: {out}");
        assert!(!out.contains("a<b"), "raw < must not survive: {out}");
    }

    #[test]
    fn template_scalar_is_value_and_conditionals_work() {
        assert_eq!(
            render_template("{{ value }}", &serde_json::json!("hi")).unwrap(),
            "hi"
        );
        let data = serde_json::json!({ "n": 9 });
        assert_eq!(
            render_template("{% if n >= 5 %}big{% else %}small{% endif %}", &data).unwrap(),
            "big"
        );
        // Missing field renders empty.
        assert_eq!(render_template("[{{ nope }}]", &data).unwrap(), "[]");
    }

    #[test]
    fn icon_span_sizes_rises_and_escapes() {
        let s = icon_span("<", 30.0, 1.3);
        assert!(s.contains("size=\"130%\""), "scaled size: {s}");
        assert!(s.contains("rise=\"-"), "larger icon is lowered: {s}");
        assert!(s.contains("&lt;"), "glyph escaped: {s}");
        // Same size as text => no rise needed (baseline is fine).
        assert!(icon_span("x", 30.0, 1.0).contains("rise=\"0\""));
    }

    #[test]
    fn plain_text_passthrough() {
        let p = process("hello & world", &["box"], &[]);
        assert_eq!(p.markup, "hello &amp; world");
        assert_eq!(p.plain, "hello & world");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn standard_bold_passthrough() {
        let p = process("<b>hi</b>", &["box"], &[]);
        assert_eq!(p.markup, "<b>hi</b>");
        assert_eq!(p.plain, "hi");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn standard_span_attrs_preserved() {
        let p = process(r##"<span foreground="#f00">hi</span>"##, &["box"], &[]);
        assert_eq!(p.markup, r##"<span foreground="#f00">hi</span>"##);
        assert_eq!(p.plain, "hi");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn single_custom_tag() {
        let p = process(r##"<box bg="#222">hi</box>"##, &["box"], &[]);
        assert_eq!(p.markup, "hi");
        assert_eq!(p.plain, "hi");
        assert_eq!(p.effects.len(), 1);
        let e = &p.effects[0];
        assert_eq!(e.tag, "box");
        assert_eq!(e.attrs, vec![("bg".to_string(), "#222".to_string())]);
        assert_eq!(e.start, 0);
        assert_eq!(e.end, 2);
        assert_eq!(&p.plain[e.start..e.end], "hi");
    }

    #[test]
    fn custom_tag_with_surrounding_text() {
        let p = process("before <box>X</box> after", &["box"], &[]);
        assert_eq!(p.markup, "before X after");
        assert_eq!(p.plain, "before X after");
        assert_eq!(p.effects.len(), 1);
        let e = &p.effects[0];
        assert_eq!(e.tag, "box");
        assert_eq!(&p.plain[e.start..e.end], "X");
        assert_eq!(e.start, 7);
        assert_eq!(e.end, 8);
    }

    #[test]
    fn standard_around_custom() {
        let p = process("<b><box>x</box></b>", &["box"], &[]);
        assert_eq!(p.markup, "<b>x</b>");
        assert_eq!(p.plain, "x");
        assert_eq!(p.effects.len(), 1);
        let e = &p.effects[0];
        assert_eq!(e.tag, "box");
        assert_eq!(&p.plain[e.start..e.end], "x");
    }

    #[test]
    fn custom_around_standard() {
        let p = process("<box><b>x</b></box>", &["box"], &[]);
        assert_eq!(p.markup, "<b>x</b>");
        assert_eq!(p.plain, "x");
        assert_eq!(p.effects.len(), 1);
        let e = &p.effects[0];
        assert_eq!(e.tag, "box");
        assert_eq!(e.start, 0);
        assert_eq!(e.end, 1);
        assert_eq!(&p.plain[e.start..e.end], "x");
    }

    #[test]
    fn two_sibling_custom_tags() {
        let p = process("<box>aa</box><glow>bbb</glow>", &["box", "glow"], &[]);
        assert_eq!(p.markup, "aabbb");
        assert_eq!(p.plain, "aabbb");
        assert_eq!(p.effects.len(), 2);

        let a = &p.effects[0];
        assert_eq!(a.tag, "box");
        assert_eq!(a.start, 0);
        assert_eq!(a.end, 2);
        assert_eq!(&p.plain[a.start..a.end], "aa");

        let b = &p.effects[1];
        assert_eq!(b.tag, "glow");
        assert_eq!(b.start, 2);
        assert_eq!(b.end, 5);
        assert_eq!(&p.plain[b.start..b.end], "bbb");
    }

    #[test]
    fn malformed_input_falls_back() {
        let p = process("a < b", &["box"], &[]);
        assert_eq!(p.markup, "a &lt; b");
        assert_eq!(p.plain, "a < b");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn nested_custom_in_custom() {
        let p = process("<box>a<glow>b</glow>c</box>", &["box", "glow"], &[]);
        assert_eq!(p.markup, "abc");
        assert_eq!(p.plain, "abc");
        assert_eq!(p.effects.len(), 2);
        // glow span pushed first (inner closes first), then box.
        let glow = p.effects.iter().find(|e| e.tag == "glow").unwrap();
        let bx = p.effects.iter().find(|e| e.tag == "box").unwrap();
        assert_eq!(&p.plain[glow.start..glow.end], "b");
        assert_eq!(&p.plain[bx.start..bx.end], "abc");
    }
}
