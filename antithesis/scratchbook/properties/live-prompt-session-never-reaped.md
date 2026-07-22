# live-prompt-session-never-reaped

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in either repo (see `antithesis/scratchbook/existing-assertions.md`).
Backend repo: `/home/chussenot/agentic-db`.

## Historical bug validated (per validating-claims)

Commit `133ae8d` ("fix(daemon): reaper uses window/pid liveness only, not a
heartbeat timeout") — mechanism confirmed from the fix diff, not the report:
the old `deadPredicate` had a **10-minute `last_seen` staleness arm**. An
idle Claude fires no hooks, so `last_seen` froze at its Stop and GC deleted
the row at 10 minutes — mid-decay. The fix deletes the staleness arm
entirely; the commit adds `TestIdleDecayRendersFullFade` which pins the
liveness-only behavior through the real GC→aggregate pipeline.

The same mechanism applies with higher stakes to **prompt**: a session
blocked on a permission prompt also fires no further hooks (Notification is
one-shot; nothing else fires until the user acts). Under the old bug, a
10-minute-old unanswered prompt would have been silently reaped — the alert
deleted precisely because the user hadn't seen it yet. Any reintroduced
staleness/heartbeat reap re-creates this. The property is the regression
guard, plus guards on the two reap arms the fix's replacement design added.

## Current reap mechanism (internal/daemon/gc.go)

`deadPredicate` (gc.go:58-71) reaps a session when ANY of:

1. `window_id` valid but absent from the niri model (gc.go:60-62) —
   **no debounce**: one GC tick with the window missing = immediate reap.
2. `terminal_pid` valid but `/proc/<pid>` gone (gc.go:63-65) — also
   undebounced (fixture uses NULL terminal_pid, so this arm is inert).
3. first-party absence: `fpMiss` counter reaches
   `firstPartyMissThreshold = 3` consecutive 1s ticks (gc.go:19,
   94-129), gated on `fpAvailable` (len(fp) > 0 after `ReadLive`).

GC runs on the 1s tick (internal/daemon/daemon.go:236-243 → gc at 351-374).

## Two race surfaces worth attacking

**(a) Window-model staleness racing niri events — including the startup
mass-reap window.** The model starts empty (`niri.NewModel()`,
daemon.go:91) and is populated only when the event stream's initial
`WindowsChanged` snapshot arrives (internal/niri/model.go:61-67). The DB
poller primes immediately (daemon.go:283), so `d.sessions` can be populated
while the model still knows zero windows. If the first 1s tick fires before
the initial snapshot (daemon paused by the fault injector; niri slow),
`HasWindow` is false for **every** session → every session with a window_id
is reaped in one tick. Unlike a false first-party reap, this is **not
self-healing for prompt**: gc.go:16-18 claims "A false reap is self-healing
regardless — the live session's next hook recreates the row", but a Claude
blocked on a permission prompt fires no next hook. The row — and the alert —
is gone until the user interacts for some other reason. Note the sibling
property `daemon-restart-no-placeholder-clobber` found the same
model-not-yet-adopted shape for `writeTiles` and gained a no-outputs escape
hatch; **gc has no such hatch**.

The same arm also has a steady-state variant: niri emits full `WindowsChanged`
snapshots; any transient snapshot that omits a live window (compositor
restart inside the harness, event reordering) reaps instantly, no debounce.

**(b) First-party miss-counter racing the 3-tick threshold.** With
`-sessions-dir` pointed at a workload-controlled dir, `fpAvailable` is true
whenever ≥1 file parses (daemon.go:357-365). If the prompt session's own
file is transiently unreadable/mid-rewrite/unparseable for 3+ consecutive
ticks while some *other* session's file stays valid, the prompt session is
reaped despite a live window. The threshold comment (gc.go:11-19) explicitly
budgets for "a transient unreadable/mid-rewrite first-party file" — 3 ticks
is the entire defense, and the self-healing justification is (per above)
false for prompt.

## Failure scenario

A permission prompt is up; the tile pulses. The fault injector pauses the
daemon around a restart so the first GC tick beats the initial niri window
snapshot — or holds the session's first-party file unreadable for 3s. The
row is deleted; the next cache write renders the desktop as the idle
placeholder. Silent, plausible, and permanent until the user touches that
session (F9: the alert state is exactly the one that cannot heal itself).

## Suggested assertions (net-new)

Workload-side:

1. `Always("a prompt session with live window and live first-party file survives GC")`
   — fixture: session row in `prompt` with a real window_id whose window
   the workload keeps open, first-party file (when the overlay dir is in
   play) kept present and valid. Checked each checkpoint: the row still
   exists. Event-counted: checked after ≥2 daemon ticks of stable fixture,
   not on a wall-clock deadline.
2. `Sometimes("daemon restarted while a prompt session was live")` — shared
   coverage guard with `daemon-restart-no-placeholder-clobber`.

SUT-side (Go):

3. `Unreachable("gc reaped via window-absence before the first niri window snapshot arrived")`
   — in gc/deadPredicate, gated on a `seenWindowSnapshot` flag set when the
   first `KindWindowsChanged` is applied. This is the startup mass-reap
   tripwire; **expected to fire at HEAD** under scheduling faults.
4. `Sometimes("gc reaped a session via window absence")` and
   `Sometimes("gc reaped a session via first-party absence")` — distinct
   messages, one per predicate arm (details carry session state). Replay
   anchors that attribute every reap to its arm; a reap of a prompt-state
   session in the details is the triage breadcrumb.
5. `Sometimes("first-party miss counter recovered before the reap threshold")`
   — in `firstPartyDead` where the counter resets on reappearance
   (gc.go:110-112). Confirms the debounce does its job at least once.

## Antithesis angle

Thread/process pause on the daemon stretches the tick-vs-initial-snapshot
window from microseconds to seconds; kill/restart cycles re-enter it every
time. The fp arm is driven by workload file mutation (rewrite the session
file slowly; make it a dir; chmod 000) timed against the 1s tick — 3 ticks
is well inside a single fault window. No clock fault needed anywhere.

## Open questions

- Should gc gate on model adoption (mirror of writeTiles' no-outputs escape
  hatch)? If the answer is "yes, obvious fix", assertion 3 flips from
  expected-failure to regression guard. If "no, startup reaps are
  acceptable because hooks recreate rows", the prompt case needs an explicit
  design answer since prompt has no next hook. `(needs human input)`
- Is 3 ticks the right first-party debounce given the file is re-read at
  two independent call sites (overlay at 13ms cadence, gc at 1s)? A slow
  workload rewrite can straddle several ticks; the threshold was tuned for
  brief races, not adversarial IO. Decides whether a found violation is
  "bug" or "tune the constant".
- Does niri under the harness stack ever emit a `WindowsChanged` snapshot
  that transiently omits a live window? `(partial: model replacement
  semantics confirmed from model.go:61-67 — any omission reaps instantly;
  whether real niri emits such snapshots is unverified)`

### Investigation Log

#### Was the original reaper bug really a heartbeat-timeout mechanism?

- Examined: `git show 133ae8d` in /home/chussenot/agentic-db (full diff:
  gc.go, daemon.go, decay_timeline_test.go, gc_test.go).
- Found: pre-fix `deadPredicate(model, now)` took a clock and included a
  `last_seen` staleness arm with a 10-minute threshold; the fix removes the
  clock parameter and the arm, leaving window/pid liveness only. The commit
  message and the added `TestIdleDecayRendersFullFade` both pin the
  mechanism: idle sessions emit no hooks → last_seen frozen → reaped at
  10min mid-decay.
- Conclusion: mechanism confirmed from the fix itself. The prompt-state
  extrapolation (no hooks while blocked) is grounded in hook.go/state.go:
  the only prompt-setting event is a one-shot Notification
  (internal/hook/hook.go:185-201); no event fires while a prompt sits
  unanswered.

#### Is the "false reap is self-healing" claim true for prompt?

- Examined: gc.go:16-18 (the claim), hook.go event handling, state.go
  MapEvent table.
- Found: recreation requires a subsequent hook; a blocked prompt produces
  none (PostToolUse fires only after the user approves). The claim holds
  for working (PostToolUse cadence) and idle-being-touched, not for prompt.
- Conclusion: the property's severity argument stands.
