---
title: Testing, screenshots and CI
status: active
date: 2026-08-29
---

# Testing, screenshots and CI

One command, locally and in CI:

```bash
mise run check
```

It runs, sequentially (shared `target/` — parallel legs race):

| Leg | Command | Gates |
|---|---|---|
| `fmt-check` | `cargo fmt -- --check` | formatting |
| `lint` | `cargo clippy --all-targets -- -D warnings` | including `examples/`, where the offscreen render paths live |
| `test` | `cargo test` | unit tests + the tile gate (`tests/tile_gate.rs`) |
| `tiles` | `pwetty check` | every bundled tile: template ↔ schema ↔ samples |
| `render` | `pwetty render --all-states` (headless) | every sample of every tile actually paints |
| `docs` | `scripts/check-docs.py` | front matter, ADR shape, dead links and anchors |

`fmt-check` was held out of `check` for a long time, and how it got back in is
worth keeping: rustfmt wanted two comment-alignment changes in `lib.rs` that
made the code worse — it read a two-line explanation of `halo` as a
continuation of the trailing comment on the binding above and reflowed it to
column 26, burying it. A gate that is red on arrival is one people switch off,
and a gate you go green by degrading a comment is worse than none.

The fix was not to accept that output. Moving the neighbouring note onto its own
line makes the `halo` comment a top-level block again, so it reads correctly
*and* formats clean. That is the shape of the answer to look for whenever a
formatter and a comment disagree.

`restart` is not a leg either — a check that kills your bar is one you stop
running.

## Unit tests

`cargo test`: the markup router + `render_template` binding (`markup.rs`),
content (`build_markup`/`parse_data`, stream framing and recovery), config,
`parse_hex_color`, and the tile gate — all pure logic, no GL/GTK context needed.

## Vision tests (offscreen → PNG)

Pure CPU, safe anywhere. **Verify headless before showing anybody anything.**

```bash
# every bundled sample, every tile — the same leg CI runs
mise run render

# data → template → tile (JSON data bound into a multi-line template)
cargo run --example render_data -- out.png            # default nas dashboard

# a tile background shader, one frame
EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  cargo run --example render_shader -- out.png examples/shaders/aurora.glsl [time]

# rich text via Pango/Cairo
cargo run --example render_text -- out.png

# content path (markup + <box>/<glow> effects, optional icon arg) via draw_content
cargo run --example render_content -- out.png "CPU <glow color='#f38ba8'>96%</glow>" 44

# the femtovg demo tile
EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  cargo run --example render_tile -- out.png [seconds]
```

Inspect the PNGs by eye — these caught a transparency bug no unit test could.

The harness must be **faithful** to the live path: `render_content` runs the
femtovg background *before* the content/effects, so GL-state interactions (like
the glow-over-dirty-FBO glitch) actually reproduce offscreen. If a headless
harness can't reproduce what the live widget does, fix the harness, don't skip
the check.

## Live waybar

`test/`: `cage` (headless) → `niri` (nested) → `waybar` → `grim`, driven by
[`test/shot.sh`](../test/shot.sh). ⚠️ Runs a nested compositor stack; read the
safety notes in that script and prefer a separate TTY. Never point it at a live
session — a `pkill niri` teardown logs the user out.

To *demo* the module, run a secondary waybar at the bottom of the screen
(`waybar -c <demo-config>`) on the live session instead, and tear it down by
matching its unique config path.

## CI

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) has two jobs:

- **check** — installs the system packages `mise run deps:list-ci` prints (the
  same list `mise run deps` checks for), then runs `mise run check`. Identical
  command to the local one, on purpose: a CI that runs a different script is a
  CI whose failures you cannot reproduce. The rendered PNGs are uploaded as an
  artifact so a paint regression is visible, not just red.
- **commits** — pull requests only: `cog check` over the PR range
  (`BASE..HEAD`), plus an assertion that every commit subject carries a scope.
  It never walks history before the baseline tag; see
  [versioning.md](versioning.md).

Actions are pinned by commit SHA and moved by
[dependabot](../.github/dependabot.yml). Pinning `cocogitto/cocogitto-action`
also pins the `cog` binary version, since the action carries it in its own
`version.env`.

The render leg works in CI because the render path is genuinely headless:
`EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1` on Mesa's llvmpipe, no
display, no seat, no DRM master. It is exercised, not assumed.
