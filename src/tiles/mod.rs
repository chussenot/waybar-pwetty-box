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
        ("duo", include_str!(tile_dir!("samples/duo.json"))),
        ("window", include_str!(tile_dir!("samples/window.json"))),
        (
            "duo-window",
            include_str!(tile_dir!("samples/duo-window.json")),
        ),
        ("mixed", include_str!(tile_dir!("samples/mixed.json"))),
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
        assert_eq!(p.samples.len(), 8);
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

    /// Every bundled sample must satisfy its own preset's schema.
    ///
    /// `pwetty check` only proves a sample *renders*; it never validates against
    /// the schema, so a sample could drift from the documented contract (or the
    /// schema could forbid a shape we actually ship) and nothing would notice.
    /// The samples are the contract's worked examples — the two must agree.
    #[test]
    fn every_sample_validates_against_its_schema() {
        for p in all() {
            let schema: serde_json::Value = serde_json::from_str(p.schema).expect("schema is JSON");
            let mut schemas = boon::Schemas::new();
            let mut compiler = boon::Compiler::new();
            let url = format!("mem:{}.json", p.name);
            compiler
                .add_resource(&url, schema)
                .unwrap_or_else(|e| panic!("{}: schema is not a usable resource: {e}", p.name));
            let sref = compiler
                .compile(&url, &mut schemas)
                .unwrap_or_else(|e| panic!("{}: schema does not compile: {e}", p.name));

            for (name, json) in p.samples {
                let data: serde_json::Value = serde_json::from_str(json).expect("sample is JSON");
                if let Err(e) = schemas.validate(&data, sref) {
                    panic!("{}: sample '{name}' violates schema.json:\n{e}", p.name);
                }
            }
        }
    }

    /// The flip side: the schema must actually *reject* malformed payloads. A
    /// permissive `anyOf` that accepts everything would pass the test above
    /// while documenting nothing.
    #[test]
    fn claude_schema_rejects_malformed_payloads() {
        let p = get("claude").expect("claude preset present");
        let schema: serde_json::Value = serde_json::from_str(p.schema).expect("schema is JSON");
        let mut schemas = boon::Schemas::new();
        let mut compiler = boon::Compiler::new();
        compiler.add_resource("mem:claude.json", schema).unwrap();
        let sref = compiler.compile("mem:claude.json", &mut schemas).unwrap();

        let bad = [
            (
                "no shortcut",
                serde_json::json!({ "items": [{ "state": "working" }] }),
            ),
            ("no rows at all", serde_json::json!({ "shortcut": 1 })),
            (
                "empty items",
                serde_json::json!({ "shortcut": 1, "items": [] }),
            ),
            (
                "three rows",
                serde_json::json!({ "shortcut": 1, "items": [
                    { "state": "working" }, { "state": "idle" }, { "state": "shell" }
                ] }),
            ),
            (
                "app row missing app_icon",
                serde_json::json!({ "shortcut": 1, "items": [{ "app": "Firefox" }] }),
            ),
            (
                "kind says app but body is a session",
                serde_json::json!({ "shortcut": 1, "items": [{ "kind": "app", "state": "working" }] }),
            ),
            (
                "unknown state",
                serde_json::json!({ "shortcut": 1, "items": [{ "state": "busy" }] }),
            ),
            (
                "idle_level out of range",
                serde_json::json!({ "shortcut": 1, "items": [{ "state": "idle", "idle_level": 9 }] }),
            ),
        ];
        for (why, data) in bad {
            assert!(
                schemas.validate(&data, sref).is_err(),
                "schema should reject {why}: {data}"
            );
        }

        // …while every shape we genuinely ship stays valid, `kind` or not.
        let good = [
            serde_json::json!({ "shortcut": 1, "items": [
                { "state": "working", "folder": "api" }, { "app": "kitty", "app_icon": "terminal" }
            ] }),
            serde_json::json!({ "shortcut": 1, "sessions": [{ "state": "working" }] }),
            serde_json::json!({ "shortcut": 1, "is_claude": false, "app": "VS Code", "app_icon": "code" }),
        ];
        for data in good {
            if let Err(e) = schemas.validate(&data, sref) {
                panic!("schema should accept {data}:\n{e}");
            }
        }
    }
}
