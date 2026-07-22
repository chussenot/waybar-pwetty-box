# animating-gate-matches-stored-content

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## Code paths

- The flag: `Inner.animating: AtomicBool`, `src/content.rs:47`, computed by
  `content_animates()` (content.rs:53-69, a string heuristic over the rendered
  markup) and stored as **step 1** of `set()` (content.rs:85-87) — *before*
  the content it describes is written (step 2, content.rs:88-90).
- The gate: tick callback `src/lib.rs:351-361` — `s.animating()` (line 352)
  decides whether the frame clock queues draws at all. This is the mechanism
  behind safety claim S11 ("static tile queues nothing — keeps the bar cool")
  and liveness claim L7 (prompt attention pulse is never starved).
- Gate existence is itself conditional: the tick callback is only installed if
  `could_anim` (template-literal scan, lib.rs:332-340) was true at init. The
  claude preset's format contains `<status`/`<pulse>`/`<tickerbox`, so the
  flagship tile always has the gate.

## Failure scenario

With a single writer, flag and content can only disagree while a `set()` is
in flight — a window of a few instructions, stretched arbitrarily by an
Antithesis thread pause on the reader thread. Both directions are
user-visible, and they are the two documented regression classes of this exact
subsystem (bug dsl and its two re-regressions, SUT analysis §5):

- **Frozen attention (S3 severity):** transition working → idle-6. Step 1
  stores `animating = false`; writer pauses before step 2. Stored/displayed
  content is still the *working* markup (blinking status), but the gate is
  off — the blink freezes mid-phase for the duration of the pause. The
  product's core attention mechanism stops moving while showing an active
  state.
- **Heat runaway (S5 severity):** transition prompt → idle-6 with the pause
  after step 1 in the *other* polarity (flag stored true for incoming
  animating content while old static content is displayed) redraws static
  pixels at 30fps — the dsl heat bug reintroduced by scheduling instead of by
  a heuristic typo.

Both self-heal ≤1 frame after the writer resumes (dirty poll + tick both
re-read). The invariant is therefore a *bounded-divergence* claim, not exact
agreement.

## Key observations

- This property intentionally checks flag-vs-**stored-markup** coherence. It
  does NOT cover the separate F3 hazard (template-scan `could_anim` vs
  rendered-markup `content_animates` disagreeing, so no tick callback exists
  at all) — that is a data-driven gating gap, not a race, and belongs to
  whichever agent owns animation gating. Placing the assertion in the 150ms
  dirty-poll callback (always installed for content tiles, lib.rs:366-374)
  rather than the tick callback means the check still runs when the tick
  callback was never created — the assertion would then *also* flag F3-style
  persistent mismatch as a side effect, which is signal, not noise.
- Cost: evaluating `content_animates(markup())` in the poll callback is a
  lock + string clone + substring scan at 6.7Hz per instance — negligible,
  and avoids adding 60Hz work to the tick path.
- Single-writer topology (verified: set() callers are content.rs:210 and
  content.rs:275 only, one thread per store) is what limits divergence to
  in-flight set() windows. Two concurrent set() calls could leave a
  *permanent* mismatch (flag from writer A, content from writer B); the
  bounded-divergence assertion is exactly the guard that would catch a future
  topology change.

## Suggested instrumentation (net-new)

In the 150ms dirty-poll callback, per instance:

1. Compute `mismatch = store.animating() != content_animates(&store.markup())`.
   Track the wall-clock start of the current mismatch streak.
   `Always(mismatch_duration_ms < 500, "animating flag reconverges with stored markup within 500ms")`
   — 500ms ≈ 3 poll periods; transient in-flight windows pass, stretched ones
   fail.
2. `Sometimes(mismatch, "animating flag transiently disagreed with stored markup")`
   — proves the window is reachable at all; without this firing, a green run
   of assertion 1 is vacuous.

## Open questions

- Is 500ms the right divergence bound, or should it be derived from the poll
  period constant so a future poll-interval change doesn't silently loosen the
  property? Cheap to parameterize; decide at implementation time.
- The pause-after-step-1 polarity requires content_animates(old) !=
  content_animates(new) across a single publish; how often does the claude
  backend actually emit such transitions (working→idle-6 requires an hour of
  decay unless sessions are added/removed)? If rare in a 10-minute Antithesis
  run, the workload needs a synthetic producer that toggles
  animating/static content aggressively for assertion 2 to ever fire.
  `(partial: transitions exist in the state machine — working/prompt/shell ↔
  idle-6 and session add/remove — but their frequency under the real backend
  in a short run is unmeasured)`

### Correction (synthesis)

The Key observations claim that placing the assertion in the dirty-poll
callback means it "would then *also* flag F3-style persistent mismatch (tick
callback never installed) as a side effect" is incorrect. In that failure mode
the animating flag and the stored markup AGREE — `content_animates` returns
true for the rendered markup and the flag is stored true, i.e. both say
"animate" — so the flag-vs-markup mismatch check passes while the pulse is
frozen (no tick callback exists to drive it). That failure class is owned by
the separate property `animating-markup-has-tick-source`. (Erratum identified
by the wildcard discovery agent.)

### Investigation Log

#### How often does the real backend emit transitions that flip content_animates in a short run?

Investigated 2026-07-22.

- Examined: `content_animates` (src/content.rs:53-69) and the `set()` call
  sites (content.rs:210, 275); the tick gate (src/lib.rs:351-361) and
  `could_anim` init scan (lib.rs:332-340); the claude backend's state model as
  recorded in sut-analysis §5 (working/prompt/shell states, idle decay levels,
  session add/remove).
- Found: the required transitions exist in the state machine —
  working/prompt/shell ↔ idle-6 flips `content_animates`, and session
  add/remove changes the rendered markup class. The polarity assertion 2 needs
  is therefore reachable in principle.
- Not found: any measurement of how often such flips occur under the real
  backend within a ~10-minute Antithesis run. working → idle-6 requires about
  an hour of decay unless sessions are added/removed, so the natural frequency
  in a short run is plausibly near zero — but this is unmeasured.
- Conclusion: tagged `(partial: ...)` — mechanism confirmed from code,
  frequency unmeasured. Resolve by observing assertion 2's fire rate in a
  first run, or preempt with a synthetic producer that toggles
  animating/static content aggressively.
