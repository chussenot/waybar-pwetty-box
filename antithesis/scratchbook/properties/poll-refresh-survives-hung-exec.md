# poll-refresh-survives-hung-exec

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## The gap

`run_command` (src/content.rs:288-296) is `Command::output()` with **no
timeout**. The poll-mode refresh thread (content.rs:208-215) is a single
loop: run → publish → sleep(interval). One invocation that never
completes wedges that tile's refresh thread **forever**, silently — the
tile keeps rendering its last content with zero staleness indication.
`sut-analysis.md` §7 names the mode ("hung command freezes that tile
forever, silently"); the memory-side risk (a never-exiting producer
buffering unbounded output) is owned by `stream-ingest-memory-bounded` —
**the staleness is owned by no property**. This one closes it.

Two distinct hang shapes, both real:

1. Child never exits (e.g. `sleep infinity` typo'd into an exec) —
   `output()` waits on exit.
2. Child exits but a forked grandchild inherited stdout — `output()`
   reads to EOF, which never comes (the classic daemonizing-command
   footgun; the likeliest real-world config accident).

Priority note: poll mode is NOT the production wiring (the claude preset
ships `stream: true`); it is reachable by one config line
(`stream` defaults to false) and by the `interval` variants tile #3
already uses for `poll-mode-cold-start-converges`. Small property,
low priority accordingly.

## What holds today vs what doesn't

- Holds: once the hung child (and anything holding the stdout write end)
  dies, `output()` returns, the loop publishes (empty stdout → template
  error card — honest, visible) and resumes. Recovery-on-death is real
  and worth pinning.
- Doesn't hold: any bound on a single invocation. There is no invariant
  to assert about the hang itself at f87ec19 other than making the stall
  observable — the property is partly a gap demonstrator, like
  `poll-mode-cold-start-converges`.

## Suggested assertions (net-new)

SUT-side (Rust):

1. `Sometimes("poll refresh cycle completed")` — end of each loop
   iteration (after `publish.set`, content.rs:210), details carrying the
   tile id. The heartbeat: emission-counted liveness; per-tile silence
   while the workload's hang fixture is active is the stall fingerprint,
   and its resumption timestamps recovery in the replay.

Workload-side:

2. `Always("poll tile publishes within 2 intervals after its hung child dies")`
   — fixture: tile #3 in poll mode (`interval: 2`), exec swapped to a
   wrapper that hangs (variant A: never exit; variant B: exit after
   forking a stdout-holding grandchild); workload later SIGKILLs the
   holdout, then asserts the tile's content advances within 2 poll
   intervals (event-counted, no wall-clock Always under faults — check
   after the kill during a quiet window). Pins the recovery leg that
   passes today.
3. `Sometimes("a poll exec invocation outlived 10 intervals")` — the
   staleness witness: fired by the workload's own bookkeeping when the
   heartbeat (assertion 1) stays silent across 10 nominal intervals while
   the fixture hang is active. Documents that the unbounded stall was
   actually reached in-run; this is the accept-or-fix artifact to put in
   front of the owner (add a timeout / kill-on-next-tick, or accept for a
   personal tool).

## Failure scenario

A user copies a stream-style exec into a poll-mode module (or drops
`"stream": true` in an edit). The command wraps a tool that forks a
helper. The tile shows plausible data from login and never updates again;
no error card, no log line, CPU flat. Indistinguishable from a quiet day.

## Antithesis angle

Faults add little beyond the fixture (the hang is workload-constructed),
but scheduling faults diversify where the kill lands relative to
`output()`'s internal read/wait, and process-kill faults against the
wrapper produce the recovery leg organically. The property mostly buys a
pinned recovery contract + an explicit, replayable demonstration of the
gap.

## Open questions

- Intended contract: should `run_command` get a timeout (and what
  happens on expiry — error card vs keep-last)? Interacts with the
  `interval: 0` one-shot question already open on
  `poll-mode-cold-start-converges`; a timeout on a one-shot changes
  cold-start semantics too. `(needs human input)`
- Variant B (grandchild holds stdout): confirm `sh -c` under the
  container's /bin/sh doesn't insert its own reaping that alters EOF
  timing (same /bin/sh identity question as the catalog-wide one; only
  affects fixture construction, not the invariant).

### Investigation Log

#### Is the stall really unbounded and silent at f87ec19?

- Examined: src/content.rs:288-296 (run_command), 208-215 (poll loop),
  Command::output semantics (waits for exit AND drains piped stdout to
  EOF).
- Found: no timeout, no select, no watchdog anywhere in the plugin; the
  loop has no second thread to observe the stall; failure maps to
  `unwrap_or_default()` → empty string only when `output()` itself
  returns.
- Conclusion: both hang shapes stall indefinitely; recovery occurs
  exactly when the child exits and all stdout write ends close —
  matching the two assertion legs.
