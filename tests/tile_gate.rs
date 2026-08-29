//! The tile gate: the structural half of this repository's contract system.
//!
//! `pwetty check` (via `mise run tiles`) proves a tile's template, schema and
//! samples agree with each other; `mise run render` proves they paint. Neither
//! notices a tile directory that exists on disk but was never registered, or a
//! registered tile missing a sample file, because both walk the *registry*.
//! This walks the tree instead, from the other end.
//!
//! See docs/tiles.md.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pwetty_box::tiles;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tiles_dir() -> PathBuf {
    repo_root().join("tiles")
}

/// Tile directories present on disk, by name.
fn tile_dirs() -> BTreeSet<String> {
    std::fs::read_dir(tiles_dir())
        .expect("tiles/ exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

fn registered() -> BTreeSet<String> {
    tiles::all().iter().map(|p| p.name.to_string()).collect()
}

/// Every tile directory keeps the four-file layout.
///
/// `tile.json` (the pretty half), `schema.json` (the data contract),
/// `samples/` (one payload per state) and `README.md` (the binding contract,
/// which the CLI prints). A tile missing any of the four is a tile a producer
/// cannot be written against.
#[test]
fn every_tile_keeps_the_four_file_layout() {
    let mut problems = Vec::new();
    for name in tile_dirs() {
        let dir = tiles_dir().join(&name);
        for file in ["tile.json", "schema.json", "README.md"] {
            if !dir.join(file).is_file() {
                problems.push(format!("tiles/{name}/{file} is missing"));
            }
        }
        let samples = dir.join("samples");
        if !samples.is_dir() {
            problems.push(format!("tiles/{name}/samples/ is missing"));
            continue;
        }
        let count = std::fs::read_dir(&samples)
            .expect("samples/ readable")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .count();
        if count == 0 {
            problems.push(format!("tiles/{name}/samples/ holds no .json payloads"));
        }
    }
    assert!(problems.is_empty(), "tile layout: {problems:#?}");
}

/// Schemas are draft-07 and stay permissive at the top level.
///
/// `additionalProperties: true` is the additive rule: a producer that starts
/// emitting a field we do not render yet must not be rejected here. See
/// docs/studies/quivive-tile-contribution.md — the `agents` object had this
/// wrong, and would have rejected a change the producer's contract allows.
#[test]
fn every_schema_is_draft_07_and_additive() {
    let mut problems = Vec::new();
    for preset in tiles::all() {
        let name = preset.name;
        let schema: serde_json::Value =
            serde_json::from_str(preset.schema).expect("schema parses as JSON");

        match schema.get("$schema").and_then(|v| v.as_str()) {
            Some(s) if s.contains("draft-07") => {}
            other => problems.push(format!("{name}: $schema is {other:?}, want draft-07")),
        }
        if schema.get("additionalProperties") != Some(&serde_json::Value::Bool(true)) {
            problems.push(format!(
                "{name}: top-level additionalProperties must be true (the additive rule)"
            ));
        }
        if !schema.get("properties").is_some_and(|p| p.is_object()) {
            problems.push(format!("{name}: schema declares no `properties`"));
        }
    }
    assert!(problems.is_empty(), "schemas: {problems:#?}");
}

/// A tile directory that is not registered ships nothing, and a registered
/// preset with no directory cannot exist (`include_str!` would not compile) —
/// so this only has one direction to catch, and it is the silent one.
#[test]
fn every_tile_directory_is_registered() {
    let unregistered: Vec<_> = tile_dirs().difference(&registered()).cloned().collect();
    assert!(
        unregistered.is_empty(),
        "tiles/ holds directories with no TilePreset in src/tiles/mod.rs: {unregistered:?} \
         — an unregistered tile is not in the .so, so `pwetty` cannot see it"
    );
}

/// Every sample file on disk is reachable through the registry.
///
/// The registry lists samples by hand, one `include_str!` each, so a sample
/// added to `samples/` without a line in `src/tiles/mod.rs` is embedded
/// nowhere and rendered by nothing.
#[test]
fn every_sample_file_is_registered() {
    let mut problems = Vec::new();
    for preset in tiles::all() {
        let dir = tiles_dir().join(preset.name).join("samples");
        let on_disk: BTreeSet<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json"))
            .map(|n| n.trim_end_matches(".json").to_string())
            .collect();
        let listed: BTreeSet<String> = preset.samples.iter().map(|(n, _)| n.to_string()).collect();
        for missing in on_disk.difference(&listed) {
            problems.push(format!(
                "tiles/{}/samples/{missing}.json is not listed in src/tiles/mod.rs",
                preset.name
            ));
        }
    }
    assert!(problems.is_empty(), "unregistered samples: {problems:#?}");
}

// ---------------------------------------------------------------------------
// Schema validation
// ---------------------------------------------------------------------------

/// Compile a preset's schema, or panic saying which one failed.
fn compile(name: &str, schema_src: &str) -> (boon::Schemas, boon::SchemaIndex) {
    let schema: serde_json::Value =
        serde_json::from_str(schema_src).expect("schema parses as JSON");
    let mut schemas = boon::Schemas::new();
    let mut compiler = boon::Compiler::new();
    let url = format!("mem:{name}.json");
    compiler
        .add_resource(&url, schema)
        .unwrap_or_else(|e| panic!("{name}: schema is not a usable resource: {e}"));
    let sref = compiler
        .compile(&url, &mut schemas)
        .unwrap_or_else(|e| panic!("{name}: schema does not compile: {e}"));
    (schemas, sref)
}

/// Every bundled sample satisfies its own preset's schema.
///
/// `pwetty check` only ever proved a sample *renders*. Nothing checked it
/// against the contract it is meant to illustrate, so a sample could drift from
/// the documented shape — or the schema could forbid a shape we actually ship —
/// and the gate would stay green. The samples are the contract's worked
/// examples; the two have to agree.
#[test]
fn every_sample_validates_against_its_schema() {
    for p in tiles::all() {
        let (schemas, sref) = compile(p.name, p.schema);
        for (name, json) in p.samples {
            let data: serde_json::Value = serde_json::from_str(json).expect("sample is JSON");
            if let Err(e) = schemas.validate(&data, sref) {
                panic!("{}: sample '{name}' violates its schema.json:\n{e}", p.name);
            }
        }
    }
}

/// The flip side, and the reason the test above is not enough on its own.
///
/// The additive rule pushes every schema toward permissiveness
/// (`additionalProperties: true`, `required` kept minimal), and a schema that
/// has drifted all the way to accepting anything would pass
/// `every_sample_validates_against_its_schema` while documenting nothing. These
/// are payloads the contract says are wrong; if one starts validating, the
/// schema has stopped being a contract.
#[test]
fn claude_schema_rejects_malformed_payloads() {
    let p = tiles::get("claude").expect("claude preset present");
    let (schemas, sref) = compile(p.name, p.schema);

    let bad = [
        (
            "no shortcut",
            serde_json::json!({ "sessions": [{ "state": "working" }] }),
        ),
        // The anyOf: a payload has to be one of items / sessions / flat app.
        (
            "claude desktop with no sessions",
            serde_json::json!({ "shortcut": 1 }),
        ),
        (
            "window with no app",
            serde_json::json!({ "shortcut": 1, "is_claude": false }),
        ),
        (
            "empty sessions",
            serde_json::json!({ "shortcut": 1, "sessions": [] }),
        ),
        (
            "three sessions",
            serde_json::json!({ "shortcut": 1, "sessions": [
                { "state": "working" }, { "state": "idle" }, { "state": "shell" }
            ]}),
        ),
        (
            "unknown session state",
            serde_json::json!({ "shortcut": 1, "sessions": [{ "state": "brewing" }] }),
        ),
        (
            "idle_level past the fade table",
            serde_json::json!({ "shortcut": 1, "sessions": [
                { "state": "idle", "idle_level": 9 }
            ]}),
        ),
        // The items[] shapes. `anyOf` over two row kinds is the part most able
        // to rot into "accepts anything", so each way of being neither a
        // session nor an app is named here.
        (
            "empty items",
            serde_json::json!({ "shortcut": 1, "items": [] }),
        ),
        (
            "three rows",
            serde_json::json!({ "shortcut": 1, "items": [
                { "state": "working" }, { "state": "idle" }, { "app": "Firefox" }
            ]}),
        ),
        (
            "a row that is neither a session nor an app",
            serde_json::json!({ "shortcut": 1, "items": [{ "folder": "api" }] }),
        ),
        (
            "an app row with no app",
            serde_json::json!({ "shortcut": 1, "items": [{ "kind": "app", "app_icon": "code" }] }),
        ),
        (
            "a session row with an unknown state",
            serde_json::json!({ "shortcut": 1, "items": [
                { "kind": "session", "state": "brewing" }
            ]}),
        ),
    ];

    let mut accepted = Vec::new();
    for (label, data) in &bad {
        if schemas.validate(data, sref).is_ok() {
            accepted.push(*label);
        }
    }
    assert!(
        accepted.is_empty(),
        "claude schema.json accepts payloads the contract calls malformed: {accepted:?} \
         — a schema that accepts everything documents nothing"
    );
}

// ---------------------------------------------------------------------------
// Cross-repo golden sync
// ---------------------------------------------------------------------------

/// Tiles whose samples are byte-identical copies of a producer's goldens.
///
/// `(tile, producer repo dir, goldens path within it)`. The producer's copy is
/// the source of truth; neither side is ever hand-edited. See
/// docs/studies/quivive-tile-contribution.md for the drift that produced this
/// rule.
const GOLDEN_SOURCES: &[(&str, &str, &str)] = &[("quivive", "quivive", "tests/goldens")];

/// Where a sibling checkout of `repo` might be: next to this one, or wherever
/// `PWETTY_GOLDENS_<REPO>` points.
fn sibling_checkout(repo: &str) -> Option<PathBuf> {
    let var = format!("PWETTY_GOLDENS_{}", repo.to_uppercase().replace('-', "_"));
    if let Ok(p) = std::env::var(&var) {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    let sibling = repo_root().parent()?.join(repo);
    sibling.is_dir().then_some(sibling)
}

/// Samples must be byte-identical to the producer's goldens.
///
/// **Skips loudly when the sibling checkout is absent.** That is the whole
/// design: a silent skip reads as a pass, and "the samples look right" reading
/// as "the samples are verified" is precisely the condition that let the
/// quivive samples drift from the emitter in the first place. If this test
/// prints a SKIPPED line, nothing was checked — run it with the checkout
/// present, or with `PWETTY_GOLDENS_QUIVIVE=/path/to/quivive`, before trusting
/// a sample.
#[test]
fn samples_are_byte_identical_to_producer_goldens() {
    let mut checked = 0usize;
    let mut skipped = Vec::new();
    let mut problems = Vec::new();

    for (tile, repo, goldens) in GOLDEN_SOURCES {
        let Some(root) = sibling_checkout(repo) else {
            skipped.push(format!(
                "{tile}: no `{repo}` checkout beside this repo and no \
                 PWETTY_GOLDENS_{} set",
                repo.to_uppercase().replace('-', "_")
            ));
            continue;
        };
        let golden_dir = root.join(goldens);
        if !golden_dir.is_dir() {
            skipped.push(format!(
                "{tile}: {} has no {goldens}/ — checkout too old, or the goldens moved",
                root.display()
            ));
            continue;
        }

        let preset = tiles::get(tile).expect("golden-backed tile is registered");
        for (name, embedded) in preset.samples {
            let golden: &Path = &golden_dir.join(format!("{name}.json"));
            match std::fs::read_to_string(golden) {
                Ok(bytes) if bytes == *embedded => checked += 1,
                Ok(_) => problems.push(format!(
                    "tiles/{tile}/samples/{name}.json differs from {} — \
                     regenerate from the producer, never hand-edit either side",
                    golden.display()
                )),
                Err(e) => problems.push(format!("{}: {e}", golden.display())),
            }
        }
    }

    if !skipped.is_empty() {
        let mut out = String::new();
        for line in &skipped {
            out.push_str(&format!("SKIPPED golden sync — {line}\n"));
        }
        out.push_str(&format!(
            "SKIPPED golden sync — {} tile(s) UNVERIFIED; a skip is not a pass\n",
            skipped.len()
        ));
        shout(&out);
    }
    assert!(problems.is_empty(), "golden drift: {problems:#?}");
    if checked > 0 {
        eprintln!("golden sync: {checked} sample(s) byte-identical to their producer");
    }
}

/// Write straight to fd 2, bypassing libtest's output capture.
///
/// `eprintln!` is captured and then *discarded* for a passing test, which would
/// make the skip above silent in exactly the run where it matters — a green
/// `cargo test` on a machine with no sibling checkout. A skip nobody sees is a
/// pass, and this test exists because a sample that merely looked right was
/// treated as one that had been checked.
fn shout(msg: &str) {
    use std::io::Write;
    let _ = std::io::stderr().lock().write_all(msg.as_bytes());
}
