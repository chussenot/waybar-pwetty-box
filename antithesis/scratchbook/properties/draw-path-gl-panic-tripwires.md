# draw-path-gl-panic-tripwires

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## The gap

Attack surface #2 in `sut-analysis.md` §12 — host abort via draw-path
panics — has confirmed abort sites but no dedicated property. Every one of
these panics unwinds into a GTK signal handler / glib trampoline with no
`catch_unwind` anywhere, i.e. aborts the whole waybar process (S1, every
bar on the desktop):

- `src/shader.rs:277` — `gl.create_texture().unwrap()` in
  `ensure_target`, called from `ShaderPass::render` **per frame** whenever
  the target size changes (first frame, scale change, bar resize).
- `src/shader.rs:300` — `gl.create_framebuffer().unwrap()`, same path.
- `src/gl.rs:21-22` (linux arm; 24-25 non-linux) —
  `.expect("libepoxy.so.0 should be loadable…")` inside a `Once`: fires on
  the first `ensure_loaded()`, reachable from init AND lazily from the draw
  path via ShaderPass compile.
- (Contrast: `create_program` failures in `link` propagate as `Result`,
  shader.rs:313-328 — only the texture/FBO pair and the epoxy load are
  panic-on-failure.)

There is no `glGetError`/`glCheckFramebufferStatus` anywhere in the
codebase; "GL object creation never fails" is an unproven assumption
(sut-analysis §10).

## Pattern precedent

`contentstore-mutex-never-poisoned`: encode the panic-freedom /
cannot-fail argument as `Unreachable` tripwires with unique messages, so
the day the argument breaks, triage gets a named property violation
instead of a raw SIGABRT core.

## Suggested assertions (net-new)

Instrument-first — convert each `unwrap()`/`expect()` to a match whose Err
arm fires the assertion **then panics with the same message** (behavior
unchanged; the SDK emits before the abort):

1. `Unreachable("draw path: shader target texture creation failed")` —
   shader.rs:277 Err arm.
2. `Unreachable("draw path: shader target framebuffer creation failed")` —
   shader.rs:300 Err arm.
3. `Unreachable("gl bootstrap: libepoxy failed to load")` — gl.rs:21-25
   Err arm (one message; the cfg arms cannot both build into one binary).
4. Companion `Sometimes("shader target was (re)created")` — top of the
   recreate branch in `ensure_target` (shader.rs:272-276): proves the
   risky path runs (it must, on every size change) so the Unreachables are
   demonstrably armed rather than dead code.

## Injectability limits (honest)

No GL fault injector exists in the harness or the tenant. Under
llvmpipe/surfaceless Mesa, `glGenTextures`/`glGenFramebuffers` effectively
fail only under memory exhaustion or absurd allocation counts; missing
libepoxy is an environment-variant fault (deliberately broken image), not
a runtime one. So at f87ec19 these assertions will very likely never fire
— their value is:

- **Ride-along tripwire**: zero-cost insurance that converts a future
  abort (GPU reset handling on real hardware, a Mesa behavior change, an
  exhaustion endpoint) into a named finding with replay.
- **Exhaustion escalation endpoint**: `shader-recompile-gl-object-leak`
  documents ~216k leaked GL objects/hour in the broken-shader-file state
  and carries the open question "does Mesa name exhaustion ever fail
  creation?" — if it does, THESE are the sites that abort. The two
  properties compose: the leak property drives, this one names the
  landing.
- **Environment-variant leg**: a no-epoxy image makes assertion 3
  deterministic and confirms the whole abort-reporting chain works (a
  cheap one-run calibration of "do Unreachables survive an abort").

## Failure scenario

The background-shader file is left broken (per-frame recompile retry
state); GL object names accumulate for an hour of compressed run; a
creation call finally fails; `.unwrap()` panics inside the draw signal;
waybar SIGABRTs; every bar on the desktop dies. Without instrumentation:
a core dump. With it: a named property violation pointing at the exact
site, plus the leak property's trail showing why.

## Open questions

- Does Mesa/llvmpipe GL name exhaustion ever return failure from
  create_texture/create_framebuffer, or grow until OOM? (Shared verbatim
  with `shader-recompile-gl-object-leak`; decides whether the escalation
  endpoint is reachable inside a run.)
- Should the Err arms degrade instead of panic (skip the shader layer for
  that frame, as `engine-init-failure-contained` does at init)? That is a
  fix decision, not an instrumentation one; the assertions are placed to
  survive it (the Unreachable stays on the failure arm either way).
  `(needs human input)`

### Investigation Log

#### Are these really the only panic-on-GL-failure sites on the draw path?

- Examined: src/shader.rs (all unwrap/expect occurrences), src/gl.rs,
  src/render.rs and src/offscreen.rs error paths referenced by
  sut-analysis §7 item 1-2.
- Found: shader.rs:277 and :300 are the only `.unwrap()` on GL object
  creation in the per-frame path; `link`/`compile` propagate Result;
  offscreen/renderer init failures are handled (engine-less mode,
  lib.rs:200-220). gl.rs expects are init-or-first-use.
- Conclusion: the three sites above are the complete set for this
  property; femtovg-internal panics (e.g. its own object creation) are
  outside the plugin's code and covered only by the generic abort
  observation in `module-teardown-never-aborts-host` / workload
  `finally_no_crash`.
