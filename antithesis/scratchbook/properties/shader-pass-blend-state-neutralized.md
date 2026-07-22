# shader-pass-blend-state-neutralized

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## Historical bug validated (per validating-claims)

Commit **75886b1 "Fix glow GL-state leak; make the headless harness
faithful"** (bead bpe) — mechanism confirmed from the diff and the bead
record: femtovg's background capture leaves `GL_BLEND` **enabled** with
uninitialized contents in the shared context's FBO; the glow ShaderPass
then blended its output over that garbage → flickering white-out in the
live widget. The fix is in `ShaderPass::render`: force deterministic state
before drawing — `gl.disable(glow::BLEND)` + `clear_color` + `clear`
(src/shader.rs:229-234 at f87ec19, with the code comment naming femtovg
explicitly). The same commit made the offscreen harness faithful
(examples/render_content now runs the femtovg layer *before*
content/effects) because the original harness could not reproduce the bug.

The structural condition persists at HEAD: femtovg and the raw ShaderPass
share one EGL context per module; the demo-tile and `<glow>`/`<bg>` paths
run femtovg-then-ShaderPass within a frame (src/lib.rs:264-304), and S8
("Deterministic GL state between renderers") is a claimed guarantee with
no automated enforcement.

## What is honestly assertable at runtime

There is **no pixel oracle** in a live run (shader output is arbitrary and
time-varying) and **no GL fault injector** — the bug class is
deterministic given the render ordering, not timing- or fault-dependent.
What CAN be asserted cheaply:

- the hazardous precondition still occurs (femtovg really does leave
  BLEND on — if it ever stops, the guard is dead code and the property is
  vacuous);
- the neutralization actually took effect before the draw (catches the
  plausible refactor: someone "simplifies away" the seemingly-redundant
  disable/clear block, whose necessity is invisible without the bpe
  context).

## Suggested assertions (net-new, both in `ShaderPass::render`)

1. `Sometimes("shader pass entered with blend left enabled by a prior renderer")`
   — `gl.is_enabled(glow::BLEND)` sampled at render() entry, **before**
   the disable (shader.rs:225-ish). Fires on demo-tile/glow frames today;
   proves the guard is load-bearing and gives triage the interleaving
   witness.
2. `Always("shader pass draw begins with blending disabled")` —
   `!gl.is_enabled(glow::BLEND)` immediately before `draw_arrays`
   (shader.rs:251-252). Survives the refactor that removes
   `gl.disable(BLEND)` while instrumentation remains; fires the first
   frame the neutralization is gone AND assertion 1's precondition holds.

Cost: one `glIsEnabled` each per shader frame — negligible even at 30fps
under llvmpipe.

## Known limits (recorded honestly)

- Does not survive a refactor that deletes code AND instrumentation
  together; no runtime assertion can.
- Does not verify the `clear` half of the fix (no cheap query for "FBO
  was cleared"); a regression that keeps the disable but drops the clear
  would show garbage only on the first frame per resize and is invisible
  to this property. The faithful offscreen harness
  (examples/render_content + a reference image) remains the *primary*
  regression guard for the full fix; this property is the cheap in-run
  tripwire, not a replacement.
- Antithesis's fault/timing exploration adds little here — the value is
  riding along on workloads that already render demo/glow/bg-shader
  frames (config-variant tile #3 covers all three).

## Alternative: recorded-exclusion rationale (if the evaluator prefers)

If the two assertions above are judged not worth SUT instrumentation, the
honest exclusion text is:

> **bpe/S8 inter-renderer GL state handoff — excluded.** The bug class
> (femtovg leaves GL_BLEND on; ShaderPass blends over garbage) is
> deterministic given the render ordering and independent of timing or
> faults, so Antithesis exploration adds nothing over a single rendered
> frame. There is no runtime pixel oracle (shader output is arbitrary)
> and no GL fault injector. The regression guard that actually
> reproduces the failure is the faithful offscreen harness introduced by
> the fix itself (examples/render_content runs femtovg before
> content/effects, 75886b1); pinning it with a reference-image check in
> CI covers the class better than any run-time assertion. S8 remains
> listed as a claimed guarantee; its enforcement is delegated to that
> harness.

## Failure scenario

A cleanup PR removes the "redundant" disable/clear block (its comment
reads like defensive noise without the bpe history). Every glow-bearing
prompt tile — the attention-critical state — renders flickering white-out
again. With assertion 2 in place, the first Antithesis frame after the
regression names the property; without it, the failure is cosmetic-lookng
S5/S3 noise rediscovered by eyeball.

## Open questions

- Is `is_enabled` reliable under llvmpipe/epoxy through glow at this call
  point (it should be a trivial state query, but the epoxy
  null-pointer-on-missing-symbol behavior noted in sut-analysis makes one
  local sanity check worthwhile before trusting it in a run)?
- Property vs exclusion: keep the two-assertion tripwire, or take the
  exclusion text and pin the offscreen harness in CI instead?
  Both are defensible; the tripwire costs two lines and one query per
  shader frame. `(needs human input)`

### Investigation Log

#### Was the bpe white-out really a femtovg blend-state leak, fixed at shader.rs render?

- Examined: `git show 75886b1` (shader.rs diff, render_content harness
  diff, bead bpe issue record in .beads/issues.jsonl), current
  shader.rs:225-234 and lib.rs:264-304 (frame ordering: femtovg capture /
  glow effects around the shader pass), sut-analysis §4 S8 and §10
  ("femtovg blend-state neutralization" guard note).
- Found: the fix adds disable(BLEND) + clear in ShaderPass::render with a
  comment naming femtovg; the bead record and commit message describe the
  observed white-out and that the pre-fix offscreen harness could not
  reproduce it (clean context); the harness was made faithful in the same
  commit.
- Conclusion: mechanism confirmed from the fix; the property asserts the
  precondition (assertion 1) and the neutralization outcome (assertion 2)
  rather than re-testing pixels, and the exclusion alternative is recorded
  for the evaluator.
