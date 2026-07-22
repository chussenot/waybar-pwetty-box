# session-desktop-attribution-tracks-window

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in either repo (see `antithesis/scratchbook/existing-assertions.md`).
Backend repo: `/home/chussenot/agentic-db`.

## Two historical attribution fixes, both validated

**7d42f65 "fix(tile): deterministic window pick (was blinking between
windows)"** — mechanism confirmed from the diff: the daemon groups windows
from `model.Windows()` (a Go map, randomized iteration), so for an
app-layout desktop (windows, no session) `winsOnWs[0]` — the window the
tile displays — flipped on every ~250ms cache rebuild. The tile visibly
blinked between apps. Fix: sort by stable window id. At HEAD the sort was
replaced by an explicit lowest-id scan (internal/tile/tile.go:190-197) —
same invariant, different code; exactly the kind of rewrite a regression
guard should outlive.

**1c26d14 "fix(hook): resolve windows via client cwd when Claude is
daemonized"** — mechanism confirmed from the diff: Claude Code v2.1.x
reparents session processes under a per-user daemon, so the hook's /proc
ancestry walk dead-ends at pid 1 and every session stayed `window_id` NULL —
and `BuildAll` drops NULL-window sessions (tile.go:404-406), so they
rendered **nowhere**. Fix: fall back to scanning /proc for `claude`-comm
client processes whose cwd matches the session's cwd and walking *their*
ancestry (hook.go:273-349); binds **only on an unambiguous single-window
match** (`resolveClientWindow`, hook.go:298-318 — "a wrong binding is worse
than a NULL one").

## The attribution chain at runtime

session.window_id (bound once by the hook) → `BuildAll` maps
window_id → `winByID[...].WorkspaceID` → `sessByWs` → payload emitted under
`Key(ws.Output, ws.Idx)` (tile.go:393-416). The desktop a session renders on
is therefore a function of **where the niri model currently places its
window**. Window moves arrive as `KindWindowOpenedOrChanged` (carries the
new WorkspaceID, internal/niri/model.go:69-71).

Timing: a window move marks the actor dirty but does **not** trigger the
immediate `writeTiles` that `WorkspaceActivated` gets (daemon.go:222-231);
it flows through reconcile → `maybeWriteTiles`, throttled to 250ms
(daemon.go:377-389). So attribution lags a move by up to ~250ms + 75ms
tile-watch poll + 150ms dirty poll — and any daemon pause stretches it.

## Property

At quiescence, every tracked session with a live window renders on exactly
the desktop key whose workspace contains that window — exactly once across
the whole cache (no loss, no duplication, no stale-desktop ghost) — and an
app-layout desktop's displayed window is a deterministic function of its
window set.

## Failure scenarios

- Regression of the 7d42f65 class: any future map-iteration-order
  dependence (the fix already migrated code shape once) → tiles flip
  content at 4Hz — S5 heat/cosmetic, but historically real.
- Move race gone wrong: a session's window moves from workspace A to B;
  a stale model or a lost `WindowOpenedOrChanged` leaves the session
  rendering on A while the user stares at B showing the idle placeholder —
  S2, alert on the wrong desktop.
- Ambiguity regression of the 1c26d14 class: two same-cwd claude clients in
  different windows get bound to one of them arbitrarily → a prompt pulses
  on the wrong desktop (strictly worse than NULL, which the fix's guard
  encodes).

## Suggested assertions (net-new)

Workload-side (ground truth is workload-owned: it created the windows and
moves them via `niri msg action move-window-to-workspace` in the nested
niri):

1. `Always("each fixture session renders on exactly the desktop key holding its window at quiescence")`
   — quiesce-then-check: after ≥3 daemon ticks with no topology changes and
   faults stopped, parse tiles.json; the session's folder/title appears
   under exactly the key `output:idx` of its window's workspace and no
   other key.
2. `Sometimes("a fixture window moved workspaces while its session was live")`
   — coverage guard for the move driver.
3. `Always("app-layout desktop content is stable across cache rebuilds with unchanged topology")`
   — with ≥2 windows and no session on a desktop and topology frozen, two
   reads of tiles.json ≥2 ticks apart show identical app/title for that key
   (the 7d42f65 regression guard; byte-dedupe in writeTiles means any flip
   is a real rewrite).
4. `Always("hook-bound window_id is the window hosting the client, or NULL")`
   — for hook invocations where the workload staged the /proc topology
   (fake `claude`-comm processes with controlled cwds): after the hook, the
   row's window_id is either the true window or NULL — never a third value.
   The ambiguous-two-clients fixture must yield NULL (1c26d14's guard).

SUT-side (Go):

5. `Sometimes("cache rebuild regrouped a session onto a different desktop key")`
   — in BuildAll (or a small diff against the previous build), replay
   anchor for move transitions actually flowing through.

## Antithesis angle

Scheduling faults interleave the move event, the 250ms build throttle, the
1s tick, and tile-watch's 75ms poll; pausing the daemon between the niri
event and the cache write stretches the stale-attribution window
arbitrarily — the quiescent check stays sound while the transient window is
explored for stuck states (e.g. an event dropped while paused). The
ambiguity fixture (assertion 4) is pure workload staging: rename a sleep
binary to `claude`, set its cwd, run the hook with a crafted stdin — no
compositor cooperation needed beyond real windows.

## Open questions

- Can a `WindowOpenedOrChanged` be lost (channel close/reconnect) leaving
  the model permanently stale on a window's workspace? The event stream has
  no resync-on-gap; the daemon exits when the stream closes
  (daemon.go:217-221), which converts "lost event" into "daemon restart" —
  probably the saving grace. `(partial: exit-on-close confirmed; whether
  niri can drop events on a live stream unverified)`
- Two sessions sharing one window (kitty tabs) both surface by design
  (tile.go:402-410); the exactly-once check must treat them as two entries
  under one key, not duplication. Fixture should include this shape once so
  the assertion's predicate is validated against the intended sharing.
- Is the `claude`-comm /proc staging stable inside the Antithesis container
  (procfs visibility, comm truncation at 15 bytes — "claude" fits)? Affects
  only assertion 4's fixture.

### Investigation Log

#### Did the "blinking tile" bug really stem from map iteration order?

- Examined: `git show 7d42f65` (diff + added
  `TestPayloadForDeterministicWindowPick`), current tile.go:184-197.
- Found: pre-fix code took `winsOnWs[0]` where the slice was built by
  ranging a map (BuildAll); the fix sorted by window id; HEAD now min-scans
  for the lowest id. Deterministic-pick invariant unchanged.
- Conclusion: mechanism confirmed; property leg 3 guards the invariant, not
  the implementation.

#### Did daemonized Claude really strand sessions off every tile?

- Examined: `git show 1c26d14` (diff + commit message), hook.go:273-349 at
  HEAD, tile.go:404-406 (NULL-window drop in BuildAll).
- Found: ancestry walk dead-ends at pid 1 under the claude daemon; NULL
  window_id sessions are skipped by BuildAll (never rendered). The fix's
  ambiguity guard returns 0,0,false when two candidates reach different
  windows.
- Conclusion: mechanism confirmed; assertion 4 encodes the fix's own
  "wrong is worse than NULL" rule.
