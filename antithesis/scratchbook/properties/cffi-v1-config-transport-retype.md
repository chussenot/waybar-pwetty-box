# cffi-v1-config-transport-retype

Focus: version compatibility — the plugin pins waybar CFFI ABI **version 1**,
whose config transport is documented (by the -sys bindings themselves) as
ambiguous and superseded by v2. Under v1, a *correctly authored* string config
value whose content happens to parse as JSON is silently retyped in transit,
and one retyped field wholesale-collapses the plugin config to the 60fps demo
tile. This is not a user typo: the config is right; the ABI boundary mangles it.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths (every hop validated from primary sources)

1. **The pin.** waybar-cffi 0.1.1 (the crate this repo builds against,
   Cargo.lock) hardcodes the ABI declaration in the `waybar_module!` macro:
   `pub static wbcffi_version: size_t = 1;` (crate src/lib.rs:100, fetched from
   crates.io). Verified in the built artifact: `objdump` of
   `target/release/libpwetty_box.so` shows the exported `wbcffi_version`
   .rodata symbol holds `1`.
2. **The documented deprecation boundary.** waybar-cffi-sys 0.1.1
   src/raw.rs:143 (doc comment on `wbcffi_config_entry.value`): "In ABI
   version 1, this may be either a bare string if the value is a string, or
   the JSON representation of any other JSON object as a string. From ABI
   version 2 onwards, this is always the JSON representation of the value as
   a string."
3. **The host side.** Waybar 0.15.0 `src/modules/cffi.cpp` (fetched from the
   0.15.0 tag; 0.14.0 is identical in this region): accepts `*wbcffi_version
   == 1 || == 2` (line 31); config conversion at lines 61-68 — v1 uses
   `value.isConvertibleTo(stringValue) ? value.asString() :
   value.toStyledString()`, v2 always `toStyledString()`. So under v1 a JSON
   string `"42"` crosses the boundary as the bare bytes `42`; under v2 it
   would cross as `"42"` (quoted). JsonCpp's `isConvertibleTo(stringValue)`
   is also true for null/bool/numeric — a JSON `null` value crosses v1 as the
   empty string.
4. **The crate's lossy recovery.** waybar-cffi 0.1.1 src/config.rs:38-54: the
   crate re-parses each value with `serde_jsonc::from_slice`, falling back to
   `Value::String` on parse error. Its own comment: "waybar provides the value
   as a bare string if it's JSON, but _also_ provides a bare string if it was
   a literal string. All we can do for now is to try to parse it as JSON, then
   fall back on treating it as a string if that fails." So bare `42` →
   `Number(42)`, bare `true` → `Bool(true)`, bare `null`-that-became-`""` →
   `String("")`. The retype is silent and deterministic.
5. **The plugin side.** src/lib.rs:157-163 — `type Config =
   serde_json::Value`; `init` receives the post-transport `raw` Value and
   calls `config::resolve(raw)` (src/config.rs:131-143). String-typed fields:
   `text`, `icon`, `format`, `align`, `font_family`, `background`,
   `font_path`, `icon_font_path`, `exec`, `background_shader`
   (src/config.rs:14-73). A retyped field (e.g. `text` arriving as a number)
   is a hard typed-deserialize error → `serde_json::from_value` fails →
   **whole config replaced by `Config::default()`** (config.rs:139-142):
   `fps: 60`, no exec, no format → the animated demo tile at 60fps
   (SUT analysis §7). Also: `tile`/`tile_file` are read via
   `Value::as_str` (config.rs:95, 104) — a retyped value makes `as_str`
   return `None`, so the preset silently doesn't load and the raw config
   renders as a raw-JSON text tile.

## Failure scenario

User (or workload) authors a perfectly schema-valid waybar module config with
`"text": "42"` (a literal string — e.g. a static tile showing a number), or
`"icon": "0"`, or any string field whose content is valid JSON(C). Waybar 0.14/
0.15/master, seeing the module declare v1, sends the bare content. The crate
re-parses it as a JSON number. `resolve` hits the hard-deserialize arm. The
user's tile is replaced by the 60fps demo tile — wrong content plus a heat
regression — with only a stderr line nobody reads. The same mechanism turns an
authored `null` into `String("")` (subtly different: deserializes fine into an
`Option<String>` as `Some("")` instead of `None` — e.g. `background: Some("")`
then fails hex parsing downstream rather than being treated as unset).

Inverse direction (also transport-version behavior): a *wrong*-typed authored
value like `"width": "220"` (string where the schema wants int) is silently
**repaired** by the v1 round-trip (bare `220` → number). So today's live
deployments may unknowingly depend on v1's repair behavior — a future crate
bump to v2 (always-styled transport) would surface those latent type errors as
new collapses. The boundary is behavioral in both directions.

## Distinction from `config-resolve-preserves-tile-identity`

That property owns the *consequence* arms of `resolve` (fs faults on
`tile_file`, wrong-typed user input, wholesale collapse) with assertions at the
end of `resolve()`. This property owns the *transport* hop before `resolve`
ever runs: correctly-typed input mutating in transit, which their assertions
would attribute to the wrong cause (their "configured exec survives config
resolution" Always would pass — exec paths don't parse as JSON — while the tile
still collapsed). The detection points are disjoint: theirs post-resolve, mine
pre-resolve on the raw Value.

## Suggested assertions (net-new)

- SUT-side `Always` at src/lib.rs:162-163, on the raw Value before
  `config::resolve`: for every key the workload's config-authoring manifest
  declares as string-typed (`text`, `icon`, `format`, `align`, `font_family`,
  `background`, `exec`, `tile`, `tile_file`, ...), if present it is
  `Value::String`. Message: **"string-typed module config values arrive as
  strings across the CFFI boundary"**. `Always` fits: the check runs on every
  module init and must hold on each one; under the pinned v1 ABI it is
  expected to *fail* whenever the workload authors a JSON-lookalike string —
  that failure is the finding (the plugin sits on the deprecated side of a
  documented transport boundary).
- Workload-side `Sometimes` on the config generator: a module was launched
  whose config contains at least one string field with JSON-parseable content
  (numeric string, `"true"`, `"null"`). Message: **"a JSON-lookalike string
  config value was exercised through the CFFI transport"**. Ensures the
  interesting input class is actually explored, since most natural strings
  (paths, templates, hex colors) don't parse as JSON and would never trip the
  Always.

## Antithesis topology notes

Fully testable with the existing headless stack (cage + niri + real waybar
dlopening the real .so — proven by test/shot.sh per SUT analysis §6). The
workload varies module configs across waybar restarts/reloads. Running waybar
0.14.0 vs 0.15.0 as two image variants confirms host-version invariance of the
v1 arm (code identical; low priority). The remediation boundary — a plugin
build declaring v2 — is a one-line patch to the vendored macro output and makes
a good A/B companion image: same workload, assertion expected to hold.

## Key observations

- The crate author knew (config.rs comment: "deserialisation into the
  configuration type should then fail") — the collapse-to-default at
  config.rs:139-142 converts that "should fail visibly" into "silently becomes
  a different tile".
- waybar has supported v2 since before 0.14.0; the ecosystem crate never moved.
  The plugin is pinned to v1 *transitively* — nothing in this repo chose v1.
- GTK4-host skew (two GTK majors in one process) examined and ruled out as a
  property: no released GTK4 waybar exists, and GTK itself aborts
  deterministically on dual-major registration — an immediate, non-timing
  failure with nothing for Antithesis to explore.

## Resolved (2026-07-22)

- **JsonCpp `null → ""` is universal across the supported matrix.** Waybar
  0.15.0's meson.build requires `jsoncpp >= 1.9.2`. JsonCpp's
  `Value::asString()` contains `case nullValue: return "";` and
  `isConvertibleTo(stringValue)` includes `type() == nullValue` — verified
  verbatim at tags 1.7.4, 1.9.2, and master, so every version in the
  supported range behaves identically. The workload can hard-assert the
  `null → Some("")` sub-case alongside the string-retype cases.
- **The retype is not latent in the live deployment.** Inspected
  `~/.config/waybar/config.jsonc` (read-only): all 20 `cffi/pwetty#N` blocks'
  string values are `module_path` (absolute path), `tile` (`"claude"`),
  `exec` (`claude-status tile-watch [--output eDP-1] N`), and `on-click`
  (`niri msg ...`) — none parse as JSON. Non-string values (`corner_radius:
  0`, `stream: true`, `width: 179/151`, `interval: 1`) are correctly typed
  and round-trip v1 unchanged. So the property guards future config edits
  (and the v2-migration inverse direction), not a live production bug.

## Open questions

None.

### Investigation Log

#### Does JsonCpp `asString()` on JSON null yield `""` across supported versions?

2026-07-22.

- Examined: waybar 0.15.0 `src/modules/cffi.cpp` (fetched from the 0.15.0
  tag — v1 arm confirmed as `value.isConvertibleTo(stringValue) ?
  value.asString() : value.toStyledString()`, version check `*wbcffi_version
  == 1 || == 2`); waybar 0.15.0 `meson.build` (jsoncpp dependency line);
  jsoncpp `src/lib_json/json_value.cpp` at tags 1.7.4, 1.9.2, and master.
- Found: meson requires `jsoncpp >= 1.9.2`. All three jsoncpp revisions have
  `case nullValue: return "";` in `asString()` and count `nullValue` as
  convertible-to-string, i.e. the behavior is stable from well before the
  supported floor through current master.
- Not found: any jsoncpp release in that range changing either function (the
  two endpoints plus the floor are identical; an in-range regression would
  contradict both endpoints).
- Conclusion: resolved — universal; no partial tag needed, the null sub-case
  is hard-assertable.

#### Does the live deployment's config contain JSON-lookalike string values?

2026-07-22.

- Examined: `/home/chussenot/.config/waybar/config.jsonc` (read-only; the
  file is `config.jsonc`, not `config`), all three bars, all 20
  `cffi/pwetty#N` module blocks.
- Found: every string value in the pwetty blocks is a path, preset name, or
  command line — none is valid JSON(C). All JSON-typed values (numbers,
  bools) are schema-correct, so nothing depends on v1's silent "repair"
  direction either.
- Not found: any `"42"`-shaped, `"true"`-shaped, or `"null"`-shaped string
  value anywhere in the pwetty config surface.
- Conclusion: resolved — retype not latent in production today; property
  remains a guard on future config edits and on a future crate bump to v2.
