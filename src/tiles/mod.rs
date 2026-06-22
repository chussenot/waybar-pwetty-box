//! Bundled tile presets — the "pretty half" of a tile config, shipped *inside*
//! the module. A waybar module references one by name (`"tile": "claude"`) and
//! pwetty merges it underneath the module's own config (the module wins; see
//! [`crate::config::resolve`]). Each preset also carries its data contract
//! (JSON Schema) plus mocked sample payloads and docs, which the `pwetty` CLI
//! surfaces for the downstream agent that supplies the real data.
//!
//! Preset config is plain JSON (no comments) so it parses with `serde_json`
//! directly — no extra dependency. Everything is `include_str!`-embedded, so the
//! presets travel with the `.so`.

/// A bundled tile: its visual config preset plus its documented data contract.
pub struct TilePreset {
    /// Lookup name (the `"tile"` value).
    pub name: &'static str,
    /// Visual config (plain JSON), merged *under* the waybar module config.
    pub config: &'static str,
    /// Draft-07 JSON Schema of the data object the template binds to.
    pub schema: &'static str,
    /// Human-readable binding contract (Markdown).
    pub doc: &'static str,
    /// Named mocked data payloads (JSON), for `pwetty check`/`render` and tests.
    pub samples: &'static [(&'static str, &'static str)],
}

macro_rules! tile_dir {
    ($p:literal) => {
        concat!("../../tiles/claude/", $p)
    };
}

const CLAUDE: TilePreset = TilePreset {
    name: "claude",
    config: include_str!(tile_dir!("tile.json")),
    schema: include_str!(tile_dir!("schema.json")),
    doc: include_str!(tile_dir!("README.md")),
    samples: &[
        ("working", include_str!(tile_dir!("samples/working.json"))),
        ("prompt", include_str!(tile_dir!("samples/prompt.json"))),
        ("idle", include_str!(tile_dir!("samples/idle.json"))),
        ("shell", include_str!(tile_dir!("samples/shell.json"))),
        ("window", include_str!(tile_dir!("samples/window.json"))),
    ],
};

macro_rules! empty_dir {
    ($p:literal) => {
        concat!("../../tiles/empty/", $p)
    };
}

/// Compact tile for an empty (windowless) desktop.
const EMPTY: TilePreset = TilePreset {
    name: "empty",
    config: include_str!(empty_dir!("tile.json")),
    schema: include_str!(empty_dir!("schema.json")),
    doc: include_str!(empty_dir!("README.md")),
    samples: &[
        ("empty", include_str!(empty_dir!("samples/empty.json"))),
        ("active", include_str!(empty_dir!("samples/active.json"))),
    ],
};

/// All bundled presets.
const PRESETS: &[TilePreset] = &[CLAUDE, EMPTY];

/// Look up a bundled preset by name.
pub fn get(name: &str) -> Option<&'static TilePreset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// All bundled presets (for `pwetty list`).
pub fn all() -> &'static [TilePreset] {
    PRESETS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_preset_is_registered_and_well_formed() {
        let p = get("claude").expect("claude preset present");
        // Config and schema are valid JSON.
        serde_json::from_str::<serde_json::Value>(p.config).expect("config is JSON");
        serde_json::from_str::<serde_json::Value>(p.schema).expect("schema is JSON");
        assert_eq!(p.samples.len(), 5);
        for (name, json) in p.samples {
            serde_json::from_str::<serde_json::Value>(json)
                .unwrap_or_else(|e| panic!("sample {name} is JSON: {e}"));
        }
    }

    #[test]
    fn empty_preset_is_registered_and_well_formed() {
        let p = get("empty").expect("empty preset present");
        serde_json::from_str::<serde_json::Value>(p.config).expect("config is JSON");
        serde_json::from_str::<serde_json::Value>(p.schema).expect("schema is JSON");
        assert_eq!(p.samples.len(), 2);
    }

    #[test]
    fn unknown_preset_is_none() {
        assert!(get("nope").is_none());
    }
}
