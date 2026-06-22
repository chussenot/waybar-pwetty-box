//! Module configuration, deserialized from the waybar `cffi/...` block.
//!
//! Waybar passes each JSON value of the module's config object to the module as
//! string key/value pairs; `waybar-cffi` collects them and hands us a
//! `serde`-deserializable struct via the `Module::Config` associated type.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Logical width of the tile area, in pixels (before HiDPI scaling).
    pub width: i32,
    /// Logical height of the tile area, in pixels (before HiDPI scaling).
    pub height: i32,
    /// Target animation framerate. The render loop ticks at this rate via the
    /// GTK frame clock; set to 0 to render once and stay static.
    pub fps: u32,
    /// Path to the primary text font (TTF/OTF). If unset or unloadable we fall
    /// back to a list of common system fonts.
    pub font_path: Option<String>,
    /// Path to an icon font (e.g. a Nerd Font) used for glyph icons in tiles.
    pub icon_font_path: Option<String>,
    /// Base font size in pixels.
    pub font_size: f32,
    /// Background cleared behind tiles, as a hex color (`#rrggbb` or
    /// `#rrggbbaa`). Omit for a transparent tile (the bar shows through — the
    /// Cairo composite honors per-pixel alpha); set it for an opaque tile.
    pub background: Option<String>,

    // --- content source (see `crate::content`) ---
    /// Static tile text. Supports `\n` for multiline. Ignored if `exec` is set.
    pub text: Option<String>,
    /// Shell command whose stdout becomes the tile text. Re-run every
    /// `interval` seconds.
    pub exec: Option<String>,
    /// Re-run cadence for `exec`, in seconds. `0` runs it once.
    pub interval: u64,
    /// Static icon glyph (e.g. a Nerd Font character), drawn before the text.
    pub icon: Option<String>,
    /// Tile template (minijinja) rendered against the data into Pango markup.
    /// Defaults to `"{{ value }}"`.
    pub format: Option<String>,
    /// Path to a Shadertoy-style GLSL fragment shader (defining `mainImage`)
    /// used as the tile's animated background. Reloaded when the file changes.
    pub background_shader: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 220,
            height: 36,
            fps: 60,
            font_path: None,
            icon_font_path: None,
            font_size: 14.0,
            background: None,
            text: None,
            exec: None,
            interval: 0,
            icon: None,
            format: None,
            background_shader: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn default_values() {
        let c = Config::default();
        assert_eq!(c.width, 220);
        assert_eq!(c.height, 36);
        assert_eq!(c.fps, 60);
        assert!(c.font_path.is_none());
        assert!(c.background.is_none());
    }

    #[test]
    fn partial_json_fills_defaults() {
        // Waybar only passes the keys the user set; the rest must default.
        let c: Config = serde_json::from_str(r#"{ "width": 400, "fps": 0 }"#).unwrap();
        assert_eq!(c.width, 400);
        assert_eq!(c.fps, 0);
        assert_eq!(c.height, 36); // untouched -> default
        assert!(c.font_path.is_none());
    }

    #[test]
    fn full_json_round_trips() {
        let c: Config = serde_json::from_str(
            r##"{ "width": 360, "height": 64, "fps": 30,
                  "font_path": "/x.ttf", "background": "#1e1e2e", "font_size": 18.0 }"##,
        )
        .unwrap();
        assert_eq!(c.width, 360);
        assert_eq!(c.font_path.as_deref(), Some("/x.ttf"));
        assert_eq!(c.background.as_deref(), Some("#1e1e2e"));
        assert_eq!(c.font_size, 18.0);
    }
}
