# engine-init-failure-contained

Focus: lifecycle transitions — initialization ordering: engine (EGL/renderer)
init failure must degrade, not abort; and the degradation shape is documented
(permanently blank tiles, no retry).

All suggested assertions are **net-new**; no Antithesis instrumentation exists
anywhere in this codebase (see `existing-assertions.md`).

## Claim

When the offscreen GL engine fails to initialize at `wbcffi_init`
(surfaceless EGL unavailable, femtovg renderer failure), the module still
initializes successfully — no panic escapes the FFI boundary, waybar keeps
running, and every subsequent draw callback completes safely with the engine
absent. The *cost* of this containment is that the tile renders nothing at
all — including pure-Cairo text that needs no GL — permanently, with no retry.
The containment is the invariant to assert; the blank-forever ceiling is the
documented degradation this property makes visible.

## Code paths (verified at f87ec19)

- `src/lib.rs:199-220` — both failure arms handled: `OffscreenGl::new()` Err →
  log + `engine = None` (lib.rs:216-219); `Renderer::new()` Err → log +
  `engine = None` (lib.rs:211-215). `PwettyBox::init` is infallible — it
  always returns an instance.
- `src/lib.rs:239` — the draw callback gates the **entire** draw body
  (background AND the Pango text layer at lib.rs:296-304) on
  `shared.engine.borrow_mut().as_mut()`. Engine `None` → only the hover ring
  can ever draw. No retry path exists anywhere: `engine` is set once at init
  and only ever read.
- `src/offscreen.rs:25-66` — the failure source: `get_platform_display`
  (EGL_PLATFORM_SURFACELESS_MESA), `initialize`, `choose_first_config`,
  `create_context`, `make_current` — each can fail in a container without a
  Mesa surfaceless ICD or with a broken GL stack.
- The one **uncontained** init-path hazard: `src/gl.rs:21-22` —
  `libloading::Library::new("libepoxy.so.0").expect(...)` inside
  `gl::ensure_loaded()`, called from `Renderer::new` (`src/render.rs:48`). A
  panic here crosses the `extern "C"` boundary (waybar-cffi has no
  catch_unwind) → process abort. Practically unreachable in-host: waybar
  itself links libepoxy.so.0, so waybar cannot be running without it loadable
  — but the property's assertion placement should still distinguish "engine
  init failed (contained)" from "init aborted (uncontained)" so a regression
  in this analysis (e.g. a future dependency that panics in `Renderer::new`
  before the Err arms) is caught.
- waybar-cffi 0.1.1 `src/lib.rs:171-186` — a *null* return from `wbcffi_init`
  is a clean per-module skip (waybar logs and continues); but the Rust `init`
  path only returns null when `InitInfo`/config-parse fails, never for engine
  failure. So the module is always present-but-possibly-blank, never skipped.

## Failure scenario

Waybar starts in an environment where surfaceless EGL is unavailable (missing
Mesa ICD, exhausted GL resources, broken driver — or an Antithesis container
variant without the software GL stack):

1. All 10 instances log "offscreen GL init failed" to stderr and come up
   engine-less.
2. Every tile is fully blank forever — including the text, which Cairo/Pango
   could render without any GL. Severity S2: looks like "no sessions
   anywhere"; a live `prompt` is invisible.
3. No crash, no retry, no user-visible error. The bar itself runs normally.

The containment half (no abort) is believed to hold; the blank-including-text
half is the questionable design (sut-analysis §10: "No GL ⇒ no draw at all is
acceptable" — an unproven assumption flagged for human judgment).

## Suggested assertions (net-new)

SUT-side:

- `Reachable`: message **"module init degraded to engine-less mode: offscreen
  GL init failed"** — lib.rs:216-219 arm.
- `Reachable`: message **"module init degraded to engine-less mode: renderer
  init failed"** — lib.rs:211-215 arm. (Two distinct messages: the arms have
  different failure sources and the renderer arm sits *after* the epoxy
  `expect`, so reaching it also proves the panic hazard was passed safely.)
- `AlwaysOrUnreachable`: message **"draw callback completed with engine
  absent"** — asserted at the end of the draw closure when
  `engine.is_none()`; must hold on every engine-less draw, and "never
  executed" is fine on healthy-GL runs. `AlwaysOrUnreachable` is exactly the
  semantics: an invariant on an optional, environment-dependent path.

Workload-side:

- `Always`: message **"waybar stays alive after engine init failure"** — on
  the degraded-environment variant, waybar's PID must survive the full run
  (draw activity, hover events, reloads).

## Fault / harness requirements

- A **container/environment variant**, not a runtime fault: run one Antithesis
  configuration with the software GL stack absent or `EGL_PLATFORM` broken so
  `OffscreenGl::new()` fails deterministically at init. (Runtime GL loss
  mid-run is a different property family — the draw-path `unwrap`s in
  shader.rs, another agent's territory.)
- Nothing else special; the degraded variant should still run the full
  workload (stream data flowing, reload triggers) to exercise engine-less
  draws broadly.

## Key observations

- Engine-less mode interacts with teardown: with `engine = None`, module
  teardown has no femtovg canvas to drop — the p9c abort cannot occur. So the
  degraded variant is *safer* at teardown than the healthy one; don't let a
  passing teardown property on this variant masquerade as coverage for
  `module-teardown-never-aborts-host`.
- The blank-including-text behavior is one `if` away from being much kinder
  (draw the Pango layer unconditionally; it needs no GL), which makes this a
  cheap improvement target if the owner decides the ceiling is a defect.

## Open questions

- Is "engine init failure ⇒ permanently blank tiles including CPU-rendered
  text" accepted-by-design or a defect? `(needs human input)` — decides
  whether a follow-up liveness property ("text renders even without GL")
  should be added after a design decision, and whether the degraded variant
  should carry a failing-by-design marker in triage notes.
- Can `Renderer::new` fail in practice when `OffscreenGl::new` succeeded
  (femtovg `OpenGl::new_from_function` / `Canvas::new` error paths with a
  current context)? If effectively unreachable, the second `Reachable` will
  never fire and could be demoted to documentation; if reachable (e.g. GLES
  version mismatch), the two-arm split earns its keep.

### Investigation Log

#### Is blank-including-text degradation accepted-by-design?

- Examined: `src/lib.rs:231-306` (draw gating), README claims table in
  sut-analysis §4 (no claim covers engine-less rendering), code comments
  around the engine `None` arms (log-only, no TODO/FIXME).
- Found: the gating is structural (one `if let` wraps everything), not
  commented as a decision; sut-analysis §10 lists it as an unproven
  assumption.
- Not found: any statement of intent in docs, comments, or beads.
- Conclusion: tagged `(needs human input)` — intent is not recoverable from
  the repo.
