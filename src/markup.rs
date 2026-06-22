//! Tile content markup processing.
//!
//! Tile content is Pango markup that may also contain *custom* effect tags
//! (e.g. `<box>`, `<glow>`, `<shader>`). Pango's parser rejects unknown tags, so
//! we split them out here: standard tags pass through to Pango untouched, custom
//! tags become [`EffectSpan`]s our own renderers handle, while their inner text
//! still flows into the laid-out text so Pango positions it.
//!
//! All functions here are pure (no GTK), so they unit-test cleanly.

/// A custom (non-Pango) tag, covering a byte range of the laid-out plain text
/// (offsets into [`Processed::plain`], which matches Pango's layout text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSpan {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub start: usize,
    pub end: usize,
}

/// Result of processing tile content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Processed {
    /// Pango-safe markup (standard tags only) to hand to Pango.
    pub markup: String,
    /// Plain text with all tags removed; effect offsets index this.
    pub plain: String,
    /// Custom effect tags extracted from the content.
    pub effects: Vec<EffectSpan>,
}

/// Escape `&`, `<`, `>` for safe insertion into Pango markup.
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Substitute `{}` in `format` with `value`. The value is escaped so command
/// output can't inject markup; the `format` template itself is treated as markup.
pub fn apply_format(format: &str, value: &str) -> String {
    format.replace("{}", &escape(value))
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

/// Process tile content into Pango-safe markup plus extracted effect spans.
/// `custom_tags` lists tag names handled as effects; every other tag is assumed
/// to be standard Pango markup. Malformed input falls back to escaped plain text.
pub fn process(content: &str, custom_tags: &[&str]) -> Processed {
    let wrapped = format!("<r>{content}</r>");
    let doc = match roxmltree::Document::parse(&wrapped) {
        Ok(doc) => doc,
        Err(_) => {
            return Processed {
                markup: escape(content),
                plain: content.to_string(),
                effects: Vec::new(),
            };
        }
    };

    let mut out = Processed::default();
    walk_children(doc.root_element(), custom_tags, &mut out);
    out
}

/// Recursively walk the children of `node`, appending to `out`.
fn walk_children(node: roxmltree::Node, custom_tags: &[&str], out: &mut Processed) {
    for child in node.children() {
        if child.is_text() {
            let text = child.text().unwrap_or("");
            out.markup.push_str(&escape(text));
            out.plain.push_str(text);
        } else if child.is_element() {
            let tag = child.tag_name().name();
            if custom_tags.contains(&tag) {
                // Custom effect tag: don't emit the tag, only its children.
                let start = out.plain.len();
                walk_children(child, custom_tags, out);
                let end = out.plain.len();
                let attrs = child
                    .attributes()
                    .map(|a| (a.name().to_string(), a.value().to_string()))
                    .collect();
                out.effects.push(EffectSpan {
                    tag: tag.to_string(),
                    attrs,
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
                walk_children(child, custom_tags, out);
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
        assert_eq!(escape("&<>"), "&amp;&lt;&gt;");
    }

    #[test]
    fn apply_format_substitutes_escaped() {
        assert_eq!(apply_format("v: {}", "a&b"), "v: a&amp;b");
    }

    #[test]
    fn apply_format_no_placeholder() {
        assert_eq!(apply_format("static", "ignored"), "static");
    }

    #[test]
    fn apply_format_multiple_placeholders() {
        assert_eq!(apply_format("{} and {}", "<x>"), "&lt;x&gt; and &lt;x&gt;");
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
        let p = process("hello & world", &["box"]);
        assert_eq!(p.markup, "hello &amp; world");
        assert_eq!(p.plain, "hello & world");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn standard_bold_passthrough() {
        let p = process("<b>hi</b>", &["box"]);
        assert_eq!(p.markup, "<b>hi</b>");
        assert_eq!(p.plain, "hi");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn standard_span_attrs_preserved() {
        let p = process(r##"<span foreground="#f00">hi</span>"##, &["box"]);
        assert_eq!(p.markup, r##"<span foreground="#f00">hi</span>"##);
        assert_eq!(p.plain, "hi");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn single_custom_tag() {
        let p = process(r##"<box bg="#222">hi</box>"##, &["box"]);
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
        let p = process("before <box>X</box> after", &["box"]);
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
        let p = process("<b><box>x</box></b>", &["box"]);
        assert_eq!(p.markup, "<b>x</b>");
        assert_eq!(p.plain, "x");
        assert_eq!(p.effects.len(), 1);
        let e = &p.effects[0];
        assert_eq!(e.tag, "box");
        assert_eq!(&p.plain[e.start..e.end], "x");
    }

    #[test]
    fn custom_around_standard() {
        let p = process("<box><b>x</b></box>", &["box"]);
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
        let p = process("<box>aa</box><glow>bbb</glow>", &["box", "glow"]);
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
        let p = process("a < b", &["box"]);
        assert_eq!(p.markup, "a &lt; b");
        assert_eq!(p.plain, "a < b");
        assert!(p.effects.is_empty());
    }

    #[test]
    fn nested_custom_in_custom() {
        let p = process("<box>a<glow>b</glow>c</box>", &["box", "glow"]);
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
