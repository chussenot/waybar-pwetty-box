# prompt-pulse-visibly-advances

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## What this is

End-to-end **attention liveness**: when a `prompt` state is displayed, the
whole-tile `<pulse>` must actually be *moving* — successive rendered frames
must differ in pulse phase. This is the one thing the product exists to do
(SUT analysis §9: S3 severity, "prompt shown but pulse frozen"; L7), and it is
the terminal observable that every upstream property (gate races, tick-source
existence, redraw budgets, publish latency) only approximates. Pixels are the
contract; everything else is mechanism.

## The chain that must hold (each link cited)

1. Backend emits `state: "prompt"` (prompt-first session cap, agentic-db
   tile.go:74-117 — owned by `prompt-priority-survives-session-cap`).
2. Template wraps the tile in `<pulse>` — but only if the prompt is in
   `sessions[0]` or `sessions[1]` (tiles/claude/tile.json:7, the `prompting`
   set-expression). This is a **cross-repo coupling**: the template checks
   exactly two sessions because the producer caps at two.
3. `content_animates` sees `state='prompt'`/`<pulse` → animating flag set
   (src/content.rs:53-69, 84-92).
4. Tick callback exists (`could_anim`, src/lib.rs:332-340) and queues draws
   throttled to `min_dt` (src/lib.rs:341-361; DEFAULT_ANIM_FPS 30,
   src/lib.rs:507).
5. Draw computes `time = engine.start.elapsed().as_secs_f32()`
   (src/lib.rs:244) and pulse alpha `osc(time, PULSE_PERIOD=1.3, floor)`
   (src/lib.rs:973-981) — opacity oscillation over the whole tile
   (src/lib.rs:552, 1041).
6. GTK frame clock actually delivers ticks (compositor-dependent).

A break anywhere renders a *steady* tile in prompt state. Every link has
either regressed for real (link 4: the dsl cluster, twice in one day) or is
heuristic/unverified.

## The f32 clock leg (uniquely Antithesis-explorable)

`time` is f32 seconds since Engine init. f32 ulp grows with magnitude:
~16ms at ~36h uptime (= one 60fps frame), ~0.125s at ~15 days, ~1s at ~97
days (SUT analysis §10). Once ulp exceeds the frame interval, consecutive
draws compute the *same* `time`; once it approaches PULSE_PERIOD (1.3s), the
pulse staircases and then effectively freezes — while the tick callback keeps
queueing draws at 30fps (heat with zero motion: frozen attention AND wasted
CPU simultaneously). There is no rebase of `start`. Wall-clock testing never
reaches this; a determinized environment that advances CLOCK_MONOTONIC
aggressively can.

## Suggested assertions (net-new)

- `Sometimes` — "a displayed prompt pulse advanced phase between consecutive
  draws": in the draw callback, when the current markup contains `<pulse`,
  compute the pulse alpha and compare to the previous draw's value for this
  widget; fire when they differ. This is the liveness witness — a run where a
  prompt is displayed but this never fires is the product-critical failure.
- `Always` — "the f32 animation clock advances between consecutive animated
  draws": when this widget's previous draw had the animating flag set and the
  frame-clock time advanced, assert `time_f32 != prev_time_f32`. Always fits:
  every evaluated pair must advance; equality is the quantization freeze.
- `Sometimes` — "prompt-state markup was rendered at all": scoping witness so
  the first assertion is known non-vacuous (workload must actually drive a
  session into prompt).

Workload-side complement: the shot.sh feasibility proof (SUT analysis §6)
shows headless screenshots of the real bar work; diffing two screenshots
250ms apart while a prompt is displayed is a pixel-level oracle needing no
SUT changes.

## Open questions

- Does Antithesis's clock-jitter fault affect CLOCK_MONOTONIC (Rust
  `Instant`), and what magnitude can it reach? `(needs human input)` — docs
  exhausted (see Investigation Log): jitter is opt-in via the
  forward-deployed engineer, documented as forward/backward jumps that stack
  (example magnitude 30s), with no statement about which POSIX clocks move;
  idle fast-forward can't accumulate in this never-idle SUT. Default plan:
  build the f32 leg on a SUT-side seam (env-var start-offset added to
  `Engine::start` elapsed) so the Always assertion is exercisable regardless;
  drop the seam only if Antithesis confirms monotonic-affecting jumps of
  hours-scale.
- If the producer's session cap is ever raised above 2, the template's
  `prompting` check (link 2) silently stops pulsing for prompts in
  sessions[2+]. Why it matters: decides whether the assertion should also
  check "payload contains any prompt ⇒ rendered markup contains `<pulse`" as
  a separate cross-repo-coupling guard, which would catch the skew the moment
  either side changes.

### Investigation Log

#### Does the Antithesis environment advance CLOCK_MONOTONIC far enough to reach the ≥36h quantization regime?

2026-07-22.

- Examined: Antithesis docs (markdown):
  https://antithesis.com/docs/configuration/the_antithesis_environment.md,
  https://antithesis.com/docs/concepts/fault_injection.md,
  https://antithesis.com/docs/concepts/fault_injection/fault_types.md, index
  via https://antithesis.com/docs/llms.txt.
- Found: the environment page says "The Antithesis simulation can
  'fast-forward' through periods of time when the system is idle" — but this
  SUT is never idle (30fps tick throttle + 60Hz compositor frame timers), so
  idle fast-forward cannot accumulate hours. The fault-types page documents
  a **Clock jitter** fault: "simulates changes to the system clock by moving
  it forward and backwards", jumps "may be temporary (reversed after a set
  duration) or permanent, and multiple jumps stack cumulatively", affecting
  all nodes equally; intrinsics like `__rdtsc()` are unaffected; the worked
  example uses ±30-second jumps. The overview page: clock faults are
  "Forward/backward clock jumps" and "To enable node faults, clock jitter,
  or custom faults, talk to your forward-deployed engineer" (opt-in, not
  default).
- Not found: any statement about which POSIX clocks the jitter moves
  (CLOCK_REALTIME vs CLOCK_MONOTONIC — "backwards" jumps suggest wall-clock
  semantics, since a backwards CLOCK_MONOTONIC would violate its contract,
  but the docs neither confirm nor deny); any documented maximum jump
  magnitude; any mechanism that advances virtual time by tens of hours
  within a run.
- Conclusion: tagged `(needs human input)` — only Antithesis (FDE) can
  confirm whether clock jitter moves CLOCK_MONOTONIC and whether hours-scale
  cumulative forward jumps are configurable. Planning default: the f32
  quantization legs are treated as **not reachable in-run**; implement the
  SUT-side seam (env-var offset added to the Engine clock, e.g.
  `PWETTY_CLOCK_OFFSET_SECS` applied in the draw-time `elapsed()`
  computation) so the `Always` f32-advance assertion is exercisable
  deterministically. The seam is also strictly more controllable than clock
  faults (can pin the run at 36h/15d/97d regimes).

#### Does the GTK frame clock tick reliably under the harness compositor (cage/niri, llvmpipe, headless)?

2026-07-22.

- Examined: GTK3 docs
  https://docs.gtk.org/gtk3/method.Widget.add_tick_callback.html; GTK issue
  https://gitlab.gnome.org/GNOME/gtk/-/issues/2511 ("GTK3 becoming
  unresponsive to mouse input events without frame callbacks"); GdkFrameClock
  internals writeup
  https://http503.gvatas.in/2023/08/02/unveiling-the-hidden-magic-of-gtk-frameclock/;
  Mozilla bug https://bugzilla.mozilla.org/show_bug.cgi?id=1542808 (Wayland
  frame callbacks as vsync; hidden windows receive no frame events); wlroots
  https://raw.githubusercontent.com/swaywm/wlroots/master/backend/headless/output.c
  (frame timer) and .../include/backend/headless.h
  (`HEADLESS_DEFAULT_REFRESH (60 * 1000)`); niri
  https://raw.githubusercontent.com/YaLTeR/niri/main/src/backend/winit.rs
  (event-driven redraw, `request_redraw()` on unfinished animations).
- Found: GTK3's frame clock on Wayland is **frame-callback driven**: after a
  commit, the clock freezes until the compositor's `frame.done` arrives
  (that callback is GTK's vsync). Compositors send frame callbacks when they
  repaint, and repaint is damage-driven — so for a *mapped, visible,
  animating* surface the loop is self-sustaining: tick → queue_draw → commit
  with damage → compositor repaints → frame.done → next tick. The outer
  clock is real even with no display: wlroots headless outputs emit frame
  events from an event-loop timer at 60Hz, and niri (winit) redraws on
  window frame events from its parent. Known failure class: hidden/occluded
  surfaces get no frame callbacks and the GTK3 clock stalls entirely (GTK
  #2511; Mutter suppresses callbacks to occluded surfaces) — not our shape,
  the bar is an always-visible layer surface.
- Not found: any unconditional (damage-independent) frame-callback cadence
  guarantee in niri or cage; a GTK3 timer fallback when a frame callback is
  lost (there is none — a lost callback stalls the clock until the next
  damage/commit).
- Conclusion: resolved for design purposes — ticks ARE delivered
  continuously for a visible bar *as long as something damages it each
  cycle*, and the pulse animation damages it by construction, so link 6
  holds when links 3-5 hold. Delivery is damage-driven, not free-running:
  a run where the tick source is broken (the very regression this property
  hunts) shows as no draws at all, which the assertions must treat as
  failure signal, not vacuity. Consequences: (1) the SUT-side draw-counter/
  phase assertions are the primary oracle (screenshot cadence can't
  distinguish compositor throttling from a frozen pulse and adds llvmpipe
  cost); (2) the harness canary (1s clock module in the same bar window)
  doubles as a damage source guaranteeing ≥1Hz frame-clock service
  independent of the plugin, de-risking the residual lost-callback stall;
  (3) the previously suggested 10-second draw-counter probe run remains a
  cheap in-run sanity check but no longer gates the design.
