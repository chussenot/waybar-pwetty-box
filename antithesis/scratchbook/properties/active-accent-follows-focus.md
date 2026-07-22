# active-accent-follows-focus

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in either repo (see `antithesis/scratchbook/existing-assertions.md`).
Backend repo: `/home/chussenot/agentic-db`.

## Claim under test (L3) and the traced chain

L3: "Desktop switch → active-accent repaint ≈ 250-300ms." The chain,
validated end to end:

1. niri `WorkspaceActivated` (Focused=true) → model flips every
   workspace's `IsFocused` and recomputes the focused output
   (internal/niri/model.go:80-91).
2. The daemon writes the tile cache **immediately** for this event kind,
   bypassing the 250ms throttle (internal/daemon/daemon.go:222-231 — the
   explicit "latency the user feels most" special case).
3. `Payload.Active = ws.IsFocused` per desktop key
   (internal/tile/tile.go:147); byte-dedupe means an accent move rewrites
   the cache exactly once with **both** affected keys changed.
4. tile-watch (75ms poll, tile.go:490, 499-538) emits one line per
   affected tile.
5. Template: `{% if active %}<active/>{% endif %}`
   (tiles/claude/tile.json, first directive) → `markup::process` sets
   `processed.active` (src/markup.rs:86-87, 211) →
   `draw_active_panel` renders the accent card (src/lib.rs:546-549,
   1098-1100), triggered by the 150ms dirty poll.

Mechanism bound: immediate write + 75ms + 150ms ≈ 250-300ms. No property
covers any hop of this today.

## Property

At quiescence, **exactly one** desktop key in the cache carries
`active:true`, it is the workspace niri reports focused, and that tile —
and only that tile — renders the accent card. After a focus switch, the
accent converges (old tile drops it, new tile gains it).

The exactly-one shape matters: the failure modes are asymmetric. Zero
active = cosmetic-ish (no accent anywhere); **two** active = the user's
"which desktop am I on" glance lies; **stale** active (accent stuck on the
previous desktop) is the worst — plausible and wrong. Stale is reachable
in principle via event-ordering drift between `WorkspaceActivated` flag
flips and full `WorkspacesChanged` snapshots (which replace all flags
wholesale, model.go:52-59); the invariant catches any such drift without
needing to predict it.

## Workload driver requirement (shared infrastructure)

Needs a **workspace-churn driver**: `niri msg action focus-workspace <idx>`
against the nested niri (a new parallel driver script,
e.g. `parallel_driver_focus_churn.sh`). This same driver is the missing
vector for the injection properties' niri-title levers
(`no-control-chars-in-pango-markup`, `embed-placeholder-parity` want title
churn on focused/unfocused desktops) — build once, serve both. Note
`WorkspaceActivated` also fires the immediate cache write, so focus churn
is additionally the highest-frequency writeTiles trigger available to the
workload — useful pressure for `tile-cache-never-torn` and the dedupe
paths.

## Suggested assertions (net-new)

Workload-side:

1. `Always("at quiescence exactly one tile payload is active and it matches niri's focused workspace")`
   — quiesce-then-check (per the catalog-wide rule: no raw wall-clock
   Always under faults): after faults stop and ≥2 daemon ticks with no
   focus changes, parse tiles.json — exactly one `"active":true` across
   keys, and it equals `niri msg --json focused-workspace` mapped to
   `output:idx`. Vacuity: skip when the model has no workspaces (the
   no-outputs escape hatch from `daemon-restart-no-placeholder-clobber`).
2. `Sometimes("the active accent moved between desktops")` — coverage
   guard that the churn driver produced at least one observed A→B accent
   transition in the cache.
3. `Sometimes("accent followed a focus switch within two dirty-poll periods")`
   — the latency claim as a **hint**, not an Always: measured
   event-counted (cache-write observed → accent-rendered draw observed
   within 2×150ms poll periods during a fault-free window). Fires
   routinely when L3 holds; its absence across a whole run is the
   staleness smell worth triaging, without ever false-positiving under
   injected pauses.

SUT-side (Rust):

4. `Sometimes("a draw rendered the active accent card")` — in
   `draw_active_panel` (lib.rs:1098). End-to-end reachability witness for
   the whole chain (payload flag → template → markup route → draw); also
   the per-draw anchor assertion 3 correlates against.

## Failure scenario

The fault injector pauses the daemon between the model flip and the cache
write, or a `WorkspacesChanged` snapshot lands with flag state from before
a rapid double-switch. The accent settles on desktop 2 while the user
works on desktop 5 — or on both. Every glance at the bar mis-anchors which
tile's status is "here". With a prompt pulsing on the truly-focused
desktop and the accent elsewhere, the user's attention is actively steered
away from the alert.

## Antithesis angle

Focus churn racing the immediate write, the 13ms reconcile debounce, the
1s tick rebuild, and daemon kill/restart mid-switch. Rapid A→B→A switches
inside one tile-watch poll interval exercise the byte-dedupe path (net-zero
change must not strand an intermediate state). Restart interleavings check
that the accent re-converges from the re-adopted model rather than
freezing on the pre-restart focus.

## Open questions

- `KindWorkspaceActivated` with `Focused=false` (activation on a
  non-focused output) mutates nothing but still triggers the immediate
  writeTiles (daemon.go:227-230 doesn't check ev.Focused) — harmless
  today (dedupe elides it); worth confirming the harness topology (single
  output in nested niri) can even emit it, else that branch is untestable
  and should be noted as such.
- Multi-output semantics: `Active` tracks niri's globally-focused
  workspace (`is_focused`), not per-output active. With the harness's
  single nested-niri output the distinction can't surface; if the
  topology ever grows a second niri output, "exactly one" needs
  re-confirmation against niri's is_focused uniqueness guarantee.
  `(partial: model code treats is_focused as unique — flag flips are
  global, model.go:84-90; niri-side uniqueness not independently
  verified)`

### Investigation Log

#### Does the immediate-write special case really bypass the throttle, and is Active exactly ws.IsFocused?

- Examined: daemon.go:222-231 (event arm calling d.writeTiles() directly),
  daemon.go:377-389 (maybeWriteTiles throttle — note writeTiles itself
  never touches lastTileBuild), tile.go:147 (Active assignment),
  model.go:80-91 (flag flip), tile.json (template gate),
  markup.rs:86-87/211 and lib.rs:546-549 (render side).
- Found: the WorkspaceActivated arm writes unthrottled; all other events
  go through the 250ms throttle. Active is ws.IsFocused verbatim; the
  emptyPayload preserves Active for focused-but-empty desktops
  (tile.go:93-99), so the exactly-one invariant spans empty desktops too.
- Conclusion: chain and bound confirmed; assertion set designed
  event-counted/quiesce-then-check per the evaluation's wall-clock rule.
