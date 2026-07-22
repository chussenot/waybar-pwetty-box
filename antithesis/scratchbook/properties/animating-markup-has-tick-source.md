# animating-markup-has-tick-source

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## What this is

The SUT has **two independent animation detectors that are only
coincidentally consistent** (SUT analysis F3):

- `could_anim` (src/lib.rs:332-339): a **template-literal** substring scan
  (`<tickerbox`, `<status`, `<pulse`, `<bg`) over `config.format`, evaluated
  **once at init**, OR forced by `fps > 0` / `background_shader`. It decides
  whether a frame-clock tick callback is *ever registered*
  (src/lib.rs:340-362). If false, no tick source exists for the widget's
  entire lifetime.
- `content_animates` (src/content.rs:53-69): a **rendered-markup** scan
  (states + `<pulse`, `<tickerbox`, `<bg`), evaluated on every publish
  (content.rs:84-92). It decides whether the (already-registered) tick
  callback queues draws.

If markup animates (`content_animates` true) but the template never contained
the literal substrings (`could_anim` false), the animating flag is set — and
there is no tick callback to read it. The tile redraws only on data change
(the 150ms dirty poll, src/lib.rs:366-374): a `<pulse>` or blinking `<status>`
is displayed **frozen at one phase, indefinitely**. That is the S3 severity
failure — prompt shown, attention signal dead — with zero errors anywhere.

This is deliberately distinct from the sibling property
`animating-gate-matches-stored-content` (flag-vs-stored-markup *race*, bounded
divergence): in the F3 case flag and markup **agree** (both "animating"), so
that property's mismatch check passes while the tile is frozen. Its evidence
file explicitly leaves F3 unowned. This property owns it.

## How the disagreement is reached

1. **Markup-passthrough templates**: any `tile_file` template of the form
   `{{ value | safe }}` (producer sends finished Pango markup) contains none
   of the four literals; the producer's `<pulse>…</pulse>` arrives via data.
   **Confirmed reachable at f87ec19** (investigation 2026-07-22): minijinja
   registers the `safe` filter *unconditionally* in `get_builtin_filters()`
   (outside even the `builtins` feature gate, defaults.rs), and markup.rs:110
   builds a plain `Environment::new()` with no filter restriction — only
   `set_auto_escape_callback(Html)`. Probed end-to-end through the production
   `render_template` path (`examples/render_data.rs`, offscreen):
   `{{ value | safe }}` against data `"<span foreground=\"#ff0000\">hi</span>"`
   emits the markup **unescaped** (quotes and tags intact); a nonexistent
   filter errors, proving the filter table is live. No bundled preset or doc
   uses `| safe` (grep over tiles/, examples/, README: zero hits), so it is
   undocumented-but-unforbidden: the workload must ship a passthrough tile
   variant, and the frozen-pulse violation is reachable today.
2. **Composed/dynamic templates**: tags assembled from template expressions
   (`<{{ tag }}…`, includes, macros) defeat the literal scan the same way.
3. **Detector drift over time**: `content_animates` and `could_anim` are
   maintained by hand in two files. The dsl regression cluster (two
   re-regressions in one day, SUT analysis §5: 4269a03 quote-style substring
   bug, dd610c4) shows exactly this class of heuristic drifting. Any future
   animated tag added to `content_animates` but not `could_anim` silently
   creates the frozen class for templates using only that tag.
4. **Already-latent double omission — now a confirmed violation class**:
   `<glow` appears in **neither** detector (it is only in the draw-path
   `needs_gl` gate, src/lib.rs:255-258), yet the glow shader **is visually
   time-varying** (investigation 2026-07-22): `GLOW_SRC` (src/shader.rs:27-34)
   modulates its alpha by `0.85 + 0.15 * sin(iTime * 2.5)` — a ~2.5s-period
   pulse, doc comment "gently pulsing coloured blob". `iTime` is bound per
   render (shader.rs:239-241) from `fx.time` (draw_glow, lib.rs:1622), which
   is wall-clock at draw time (`engine.start.elapsed()`, lib.rs:244). Offscreen
   probe (render_shader on a copy of GLOW_SRC): center alpha 153 at the sine
   peak vs 107 at the trough — a clearly visible swing. So a glow-only tile
   freezes at whatever phase its last data-driven draw sampled. Both detectors
   agree "static", which means an assertion using the SUT's own
   `content_animates` as its oracle would inherit the blind spot and pass on
   the frozen glow: **the property's oracle must be
   `content_animates(markup) || markup.contains("<glow")`**, and `<glow`
   belongs in both detectors as the fix.

Note autoescape largely blocks *data* from spoofing the detectors in the
honest direction (quotes and `<` are escaped, so a window title containing
`state='working'` or `<pulse` does not match content_animates' patterns) —
the risk is template-shaped, not injection-shaped.

## Suggested assertions (net-new)

- `Always` — "animating markup always has a frame-clock tick source":
  in the per-instance 150ms dirty-poll callback (installed for every content
  tile regardless of `could_anim`, src/lib.rs:366-374), assert
  `!(content_animates(&store.markup()) || store.markup().contains("<glow"))
  || tick_installed`, where `tick_installed` is the `could_anim` boolean
  captured at init and threaded into the closure. The `<glow` term is part of
  the *assertion's* oracle, not the SUT's — glow is confirmed time-varying
  (scenario 4) and missing from both detectors, so using bare
  `content_animates` would inherit that blind spot. Always fits: evaluated
  every poll, must hold every time; a violation is a permanently frozen
  animation, not a transient race. At f87ec19 this assertion is expected to
  fail for both a passthrough-`<pulse>` tile and a glow-only tile.
- `Sometimes` — "a content publish flipped animating false→true after a
  static period": confirms the workload actually exercises the
  static→animating transition on which the property is non-vacuous.

## Why it matters

`could_anim=false` means the failure is *structural and permanent* — unlike
the race property, no amount of waiting self-heals it. The product's single
purpose (surface "Claude is waiting") renders as a calm frozen tile. And the
assertion doubles as a tripwire against detector drift (scenario 3), which the
bug history says is the realistic long-term path to reintroducing it.

## Open questions

- Is markup-passthrough (`| safe`) an *intended* pattern the maintainer wants
  to keep, or should the plugin restrict the filter set?
  `(partial: mechanically resolved 2026-07-22 — the filter is enabled and
  passes markup unescaped end-to-end, so the violation is reachable today and
  the workload ships a passthrough variant regardless; only the design-intent
  half remains, and it no longer gates the property's scope)`
- Where exactly to source `tick_installed` — plumb `could_anim` into the
  dirty-poll closure, or expose it on the store? Implementation detail;
  either preserves the assertion semantics.

### Investigation Log

#### Are `<glow>` shaders visually time-varying?

2026-07-22.

- Examined: `src/shader.rs:24-34` (`GLOW_SRC`), `src/shader.rs:214-250`
  (`ShaderPass::render` uniform binding), `src/lib.rs:231-303` (draw callback
  `time`/`EffectCtx`), `src/lib.rs:1588-1628` (`draw_glow`),
  `src/lib.rs:332-339` (`could_anim`), `src/content.rs:53-69`
  (`content_animates`). Offscreen probe via
  `cargo run --release --example render_shader` on a byte-identical copy of
  `GLOW_SRC` (surfaceless EGL, no compositor).
- Found: glow alpha is `smoothstep(...) * 0.6 * (0.85 + 0.15*sin(iTime*2.5))`
  — explicit time dependence, ~2.5s period; `iTime` is set every render from
  `engine.start.elapsed()` sampled at draw time. Probe at the sine peak
  (t=0.628) vs trough (t=1.885): center pixel alpha 153 vs 107 — visible.
  `<glow` is absent from both `could_anim` and `content_animates`.
- Not found: any tick source a glow-only tile would get; any bundled preset
  using `<glow>` (tiles/claude and tiles/empty don't).
- Conclusion: resolved YES — glow is time-varying, the double omission is a
  real frozen-animation class, and the property's oracle is extended to
  `content_animates(m) || m.contains("<glow")` (body and assertion updated).

#### Are `| safe` / markup-passthrough templates reachable in the supported config surface?

2026-07-22.

- Examined: `src/markup.rs:109-117` (`render_template` builds a plain
  `Environment::new()` + HTML autoescape, no filter restriction); minijinja
  `defaults.rs` `get_builtin_filters()` (crate source on disk, v2.12.0;
  Cargo.lock pins 2.21.0) — `safe` is registered unconditionally, above the
  `#[cfg(feature = "builtins")]` block; grep for `safe` filter usage across
  `tiles/`, `examples/`, `README.md`. End-to-end probe:
  `render_data` example (drives the production `render_template`) with
  template `{{ value | safe }}` and markup-bearing string data.
- Found: probe emits the data's markup unescaped (tags and quotes intact);
  a control with a nonexistent filter panics with a template error, showing
  the filter table is authoritative. No bundled preset, example, or README
  pattern uses `| safe`.
- Not found: anything in config resolution or template handling that
  restricts filters; any documentation blessing or forbidding passthrough.
- Conclusion: mechanically resolved — passthrough is reachable today within
  the supported config surface (`format` is arbitrary template text), so the
  workload must ship a passthrough tile variant; whether it is *intended*
  usage remains a design-intent question (tagged partial above) but no longer
  changes the property's scope.
