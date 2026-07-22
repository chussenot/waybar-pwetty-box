# neighbor-modules-stay-live

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## What this is

Host-neutrality: the plugin runs **in-process on waybar's single GTK main
loop**, which also services every other module on every bar (clock, tray,
workspaces) and all input. The obvious-but-never-asserted property: *while
this plugin's data chain is tortured, the rest of the bar keeps being a bar* —
other modules keep updating, the main loop keeps dispatching. Crash-freedom
(owned by the lifecycle/teardown properties) is necessary but not sufficient:
a **wedged or stalled** main loop leaves waybar alive in `ps` while the whole
desktop's status bar is frozen. No ensemble focus owns the aliveness of the
*neighbors*; each owns one mechanism that could kill them.

This is the umbrella observable over several per-mechanism properties — it
catches main-loop stalls from *any* cause, including ones no one predicted.

## Known stall mechanisms on the main thread (each cited)

- **Synchronous `fs::read` of a data-controlled icon path** in the draw
  callback (src/lib.rs:1248-1252, 1300-1315) — a FIFO path blocks forever;
  mechanism owned by `icon-src-read-bounded-nonblocking`, but that property
  guards the one call site; this one measures the blast radius.
- **Per-frame shader-file retry storm**: `refresh_shader` (src/lib.rs:54-79)
  re-stats, re-reads, and re-compiles a *failing* shader file on **every
  draw** (guard requires `shader.is_some()`, line 62), and a shader tile is
  force-animated (`forced`, src/lib.rs:332) at 30-60fps — 30-60 synchronous
  file reads + GLSL compiles per second on the main thread; inject fs latency
  on that path and every draw stalls the loop.
- **Per-frame GL render + `glReadPixels` readback** (src/lib.rs:267-277) on
  llvmpipe is CPU work on the main thread, ×10 instances.
- **Unbounded markup layout**: a huge stream line becomes a huge Pango layout
  in the draw path (input-size property owned by `stream-line-length-bounded`;
  the layout cost lands here).
- 10 instances share the one loop, so any one tile's stall freezes all ten
  plus every non-pwetty module.

## Suggested assertions (net-new)

- SUT-side `Always` — "the pwetty draw callback completes within its stall
  budget": wrap the `connect_draw` closure body (src/lib.rs:231-322) with a
  monotonic timer and assert elapsed < budget (e.g. 250ms — generous for a
  tile draw, far below human-visible bar freeze). Always fits: every draw
  must meet it. Caveat: a *permanently* wedged draw (FIFO read) never reaches
  the assertion — which is why the workload-side check below is the other
  half.
- Workload-side liveness — run waybar with a non-pwetty canary module (e.g. a
  1s clock) beside the 10 tiles in the harness (shot.sh already proves this
  topology screenshots headlessly, SUT analysis §6); screenshot the canary
  region periodically and assert `Sometimes` — "canary module content changed
  while plugin faults were active" — plus a workload check that the canary
  never goes stale longer than N seconds during the fault schedule.
- SUT-side `Sometimes` — "a draw took longer than 50ms": exploration hint
  marking the interesting slow-draw territory so Antithesis steers toward
  schedules that stretch draw time, without failing the run.

Messages are distinct from `icon-src-read-bounded-nonblocking`'s (that
property asserts a precondition guard at one call site; these assert the
loop-level outcome).

## Why it matters

The plugin's stated safety claim S2 is exactly this ("a slow command never
blocks the GTK main loop", src/content.rs:10-12) — but it is implemented only
for the *command* path (detached threads); the draw path re-imports
synchronous I/O the claim thinks it excluded. Severity is S1-adjacent: the
bar is dead to the user, yet no crash dump, no restart (a supervisor watching
the PID sees a healthy process), and every tile shows plausible stale state —
the worst observability posture of any failure in the system.

## Open questions

- What is the right draw budget number, and should it scale with instance
  count (10 draws back-to-back must still fit a frame budget)? Why it
  matters: too tight flags llvmpipe-under-load as false positives; too loose
  misses real jank; a first run with the `Sometimes(>50ms)` marker gives the
  empirical distribution to set it from.
- Can the harness capture per-region screenshots cheaply enough to run the
  canary check at a useful cadence (grim under cage/niri vs full-frame
  compare)? Why it matters: decides whether the workload half is continuous
  or spot-check-only; a spot-check-only canary weakens detection of
  transient (self-healing) wedges but still catches permanent ones.

### Investigation Log

#### Does GTK deliver the draw signal at all for an occluded/unmapped bar in the harness?

2026-07-22.

- Examined: GTK3 docs
  https://docs.gtk.org/gtk3/method.Widget.add_tick_callback.html; GTK issue
  https://gitlab.gnome.org/GNOME/gtk/-/issues/2511; GdkFrameClock-on-Wayland
  writeup
  https://http503.gvatas.in/2023/08/02/unveiling-the-hidden-magic-of-gtk-frameclock/;
  Mozilla bug https://bugzilla.mozilla.org/show_bug.cgi?id=1542808; wlroots
  headless frame timer
  https://raw.githubusercontent.com/swaywm/wlroots/master/backend/headless/output.c;
  harness files `test/shot.sh`, `test/niri.kdl` (static read). Full source
  detail in `prompt-pulse-visibly-advances.md` → Investigation Log.
- Found: on Wayland, GTK3 draw/tick delivery is frame-callback driven and
  frame callbacks are only sent for surfaces the compositor actually
  repaints — hidden/unmapped surfaces receive none and the frame clock
  stalls entirely (GTK #2511; Mutter suppresses callbacks to occluded
  surfaces). In the harness the bar is a mapped, always-on-top layer surface
  on niri's single output, so the occlusion case does not arise; the outer
  pacing clock exists even without a display (wlroots headless outputs tick
  a 60Hz frame timer). Draw delivery is damage-gated, though: with fully
  static content GTK legitimately delivers zero draws — an *idle* window
  for the SUT-side Always, not a stall.
- Not found: any GTK3 fallback timer that keeps ticks flowing without frame
  callbacks (none exists); any niri/cage guarantee of damage-independent
  frame-callback cadence.
- Conclusion: resolved — for this harness's always-visible bar, draws are
  delivered whenever anything damages the bar, and the topology's 1s clock
  canary damages the shared bar window every second, guaranteeing ≥1Hz
  frame-clock service (and thus draw-signal delivery) independent of the
  plugin. Design consequence, as the question anticipated: the SUT-side
  Always is per-draw and is vacuous only in sub-second static windows,
  which is acceptable because the canary carries permanent-wedge detection
  (a wedged loop freezes the canary's own updates — observable both by
  screenshot and by the canary module simply not redrawing). Keep the
  canary in the same bar window as the tiles so it shares their GTK main
  loop AND their surface's frame-callback stream.

Second pass, 2026-07-22 (independent, GTK widget-level semantics — corroborates
the above from the toolkit side):

- Examined: GTK 3.24 source `gtk/gtkwidget.c` (gtk-3-24 branch,
  `gtk_widget_queue_draw_region` / `_area` / `_draw`); GTK3 docs
  `gtk_widget_is_drawable`, `gtk_widget_queue_draw_region`.
- Found: `gtk_widget_queue_draw_region` is a hard no-op for a non-mapped
  widget — verbatim: `if (!_gtk_widget_get_realized (widget)) return;` then
  `/* Just return if the widget or one of its ancestors isn't mapped */
  for (w = widget; w != NULL; w = w->priv->parent) if
  (!_gtk_widget_get_mapped (w)) return;`. All queue_draw variants funnel
  here, so for an unmapped bar neither the dirty-poll's `queue_draw` nor
  anything else can even *schedule* a draw; and per
  `gtk_widget_is_drawable` docs, "A widget can be drawn to if it is mapped
  and visible" — the draw signal is never emitted for unmapped widgets.
- Not found: any queue-draw path that bypasses the mapped check.
- Conclusion: confirms the first pass with primary GTK source — the unmapped
  case is a toolkit-level guarantee of zero draws (not merely
  compositor-dependent frame-callback starvation), so the SUT-side
  stall-budget Always is structurally vacuous while the bar is unmapped and
  the canary must carry the property in those windows. No change to the
  first pass's harness conclusion (bar always mapped, ≥1Hz canary damage).
