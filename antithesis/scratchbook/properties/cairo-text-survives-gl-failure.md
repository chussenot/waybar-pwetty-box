# cairo-text-survives-gl-failure

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## What this is

The code *documents* that pure-Cairo content tiles are independent of GL
("Pure-Cairo content tiles … skip the EGL make_current entirely",
src/lib.rs:250-254), but the architecture holds text rendering hostage to GL
health at two places:

1. **Init coupling**: if `OffscreenGl::new()` fails (src/lib.rs:197-220,
   offscreen.rs:25-66), `engine` is `None` — and the **entire draw body** is
   gated on `if let Some(engine)` (src/lib.rs:239). The Pango/Cairo text layer
   (src/lib.rs:296-304) lives *inside* that gate. A claude tile — which with
   no background shader, no `<bg>`, no `<glow>` never needs GL at all
   (`needs_gl` false, src/lib.rs:255-258) — renders **permanently blank**
   because an EGL context it would never have used failed to initialize. Only
   the hover ring (outside the gate, src/lib.rs:310-319) still draws.
2. **Per-frame coupling**: when `needs_gl` is true and `make_current()` fails
   (src/lib.rs:260), the whole layer-1+layer-2 body is skipped — including the
   text. One GL fault erases the *data*, not just the decoration. No retry, no
   fallback, blank until waybar restarts (SUT analysis §7 "GL" row, §10 "'No
   GL ⇒ no draw at all is acceptable'").

The only thing the Cairo path actually consumes from `Engine` is
`engine.start` (the time source) and the span-shader cache — neither requires
a live EGL context for plain text.

This is an implicit guarantee the system provides in prose but not in code —
"the tile shows your session state as long as text rendering works" — and no
ensemble focus owns GPU/EGL degradation fidelity (Focus 3 is producer-side
recovery; Focus 5 is leaks/boundedness).

## Failure scenario

Antithesis injects resource pressure (ENOMEM, fd exhaustion) during waybar
startup or module reload-reinit, exactly while `eglGetPlatformDisplay` /
`eglInitialize` / `eglCreateContext` runs (offscreen.rs:28-59). Result at
f87ec19: 10 silently blank tiles for the life of the process, stderr-only
diagnostics, while the producer chain runs healthily underneath — the maximal
silent-staleness state (S2) with a *healthy* data chain. Alternatively a
mid-run `make_current` failure on a shader tile blanks that tile's text each
frame the fault persists.

## Suggested assertions (net-new)

- `Always` — "a content tile with markup renders its text layer on every
  draw": at the end of the draw callback, assert
  `markup.is_none() || text_layer_drawn`, where `text_layer_drawn` is set
  when the `draw_content` call (src/lib.rs:303) executes. Always fits: it
  must hold on every single draw; the engine-None and make_current-failure
  branches are exactly the violations. At f87ec19 this fails by construction
  under GL fault injection — that is the finding.
- `Sometimes` — "a draw completed with needs_gl false on a content tile":
  witness that the pure-Cairo fast path is actually exercised by the
  workload (it should dominate for claude tiles), anchoring the claim that
  text never needed GL in the first place.
- `Unreachable` — "engine absent while content markup is available": the
  init-coupling state specifically (engine None but a live ContentStore
  publishing markup); reaching it means the plugin knowingly discards live
  session data every frame. Unreachable fits: it is a persistent bad state,
  not an optional path.

## Key observations

- The p9c-fix branch (30100f9) addresses teardown, not this: init failure and
  per-frame make_current failure remain blank-forever paths after that merge.
- Fixing the property is cheap (hoist `draw_content` out of the engine gate,
  synthesize `time` from a widget-local Instant when engine is absent) — the
  property is worth asserting *because* the fix is plausible; it defines the
  degradation contract the code comment already implies.
- Interaction with `config-resolve-preserves-tile-identity`: a degraded
  config that *adds* fps 60 forces `needs_gl` only via the demo path;
  a healthy claude config never needs GL — making the blank-text outcome
  purely collateral damage.

## Open questions

- Which EGL failure modes are actually injectable in the harness (surfaceless
  Mesa + llvmpipe is pure CPU: no /dev/dri dependency, so the realistic
  faults are memory pressure and fd exhaustion during driver dlopen)? Why it
  matters: determines whether the Always assertion can be violated in-run or
  only via a start-with-fault topology; if neither works, the property needs
  a fault seam (e.g. an env var forcing `OffscreenGl::new()` to fail) to be
  testable.
- Can `make_current` fail transiently (vs only-ever-fatally) on surfaceless
  Mesa? Why it matters: transient failure would make the per-frame leg a
  flicker-and-recover behavior (weaker property, bounded blank duration);
  fatal-only makes blank-forever the sole outcome and the Always assertion
  the right shape.
- Is "blank tile on GL failure" an accepted degradation for a personal tool?
  `(needs human input)` — same design-contract judgment the SUT analysis
  flags for silent staleness generally; the property encodes the stricter
  reading of the code's own comment.

### Investigation Log

#### Is "blank tile on GL failure" an accepted degradation for a personal tool?

Investigated 2026-07-22.

- Examined: the engine gate and draw body (src/lib.rs:239-319 — init gate at
  :239, per-frame `make_current` skip at :260, text layer :296-304, hover
  ring :310-319), `OffscreenGl::new` (offscreen.rs:25-66), the code's own
  comment at src/lib.rs:250-254, the p9c-fix branch (30100f9), sut-analysis §7
  ("GL" row) and §10; README and AGENTS.md for any statement of intended
  degradation behavior.
- Found: the mechanism as described in this file — init failure and per-frame
  `make_current` failure both blank the text layer with stderr-only
  diagnostics; the comment at lib.rs:250-254 implies pure-Cairo tiles are
  GL-independent while the structure contradicts it; sut-analysis §10 records
  "no GL ⇒ no draw at all is acceptable" only as an unproven assumption, not
  a decision.
- Not found: any statement of intended behavior — no doc, code comment,
  TODO/FIXME, or bead declares blank-on-GL-failure either accepted or a bug.
- Conclusion: tagged `(needs human input)` — intent question for the owner;
  the code prose and the code structure point in opposite directions and only
  the owner can say which is the contract.
