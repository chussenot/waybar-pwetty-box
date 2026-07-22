# orphaned-tile-watch-bounded

Found independently by 2 discovery focuses (failure-recovery + resource-boundaries) — merged during synthesis; independent rediscovery is a confidence signal.

Focus: waybar restart recovery — orphaned tile-watch processes from the
previous incarnation must not accumulate.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`). The producer side lives in the
external backend repo (~/agentic-db); any backend-side instrumentation is
likewise net-new there.

## Claim

`tile-watch` producer processes do not accumulate across waybar death/restart
cycles: after waybar dies and the tile data next changes, every orphaned
tile-watch chain exits, so the live tile-watch process count stays bounded by
the configured module count (exact bound depends on the assertion design
below).

## Code paths (verified)

- `src/content.rs:260-284` — one detached reader thread + one `sh -c` child
  (`tile-watch`) per stream tile. Nothing registers cleanup: no kill-on-drop,
  no process group, and `wbcffi_deinit` is a no-op for these (SUT analysis:
  reload leak, "waybar modules are process-lifetime" false premise at
  `content.rs:259`).
- Backend `~/agentic-db/internal/tile/tile.go:499-538` `RunWatch`: infinite
  loop — sleep 75ms (`watchPoll`, tile.go:490) → read cache → marshal →
  **write to stdout only if the payload changed** (`if s := string(b); s !=
  last`, tile.go:526-530, error ignored). The loop has **no exit condition of
  its own**; it never checks whether its parent or its pipe is alive except
  implicitly through the write. Waybar death (crash or kill) closes the pipe
  read ends; the orphaned `tile-watch` is reparented to init and keeps polling
  the cache every 75ms (tile.go:490, 534-537).
- **Orphan death is write-gated**: only a write can hit EPIPE and kill the
  process — tile-watch is a Go program writing to fd 1; when the pipe's read
  end closes, the *next* write gets EPIPE, and the Go runtime's default
  disposition for SIGPIPE on fds 1/2 kills the process. The ignored error
  return (`_, _ =`) never matters because the runtime signal fires first.
  **Verified by repro 2026-07-22** (see Investigation Log): after the read end
  closed, the real tile-watch survived while its payload was unchanged and
  died with wait status 141 (128+SIGPIPE) **26ms** after a forced cache
  change. Grep of ~/agentic-db found no `signal.Notify`/`signal.Ignore` for
  SIGPIPE, so the default applies.
- **Chain topology (verified 2026-07-22)**: `sh -c` on this system (dash
  0.5.12) does **not** exec-replace — each chain is **two processes** (`sh`
  parent + `claude-status` child; strace shows vfork+execve). The `sh` holds a
  second copy of the pipe write end but never writes, so it cannot get EPIPE;
  it dies by exiting normally the moment it reaps its dead child (observed in
  the same probe). Orphan process counts are therefore 2× the chain counts
  below, and workload counters must match either both names or `claude-status`
  only.
- Consequence: a desktop whose payload never changes (`emptyPayload` — no
  `idle_ago` field, `tile.go:93-99`) produces an **immortal orphan**: it lives
  indefinitely, polling tiles.json every 75ms. Orphan lifetime is
  data-dependent, not bounded.

## Failure scenario

Repeated waybar restarts — crash/restart churn is exactly what Antithesis
induces, and the p9c crash at f87ec19 makes involuntary restarts realistic
(every output-removal SIGABRT is one; waybar aborts on output removal or
reload):

1. Waybar dies; its 10 tile instances leave up to 10 orphaned `tile-watch`
   processes (chains) per death.
2. Orphans on desktops with live idle sessions die within ~a minute
   (`idle_ago` tick → write → EPIPE). Orphans on empty desktops **never die**;
   each burns a 75ms-poll loop against tiles.json until the next payload
   change finally reaps the whole cohort at once.
3. New waybar spawns 10 fresh producers. After k restarts with q quiet
   desktops: **10 + k·q live tile-watch processes**, each doing ~13
   cache-file reads/sec. Unbounded fd/CPU/process growth tied directly to
   crash frequency — a recovery procedure (restart) that leaks state
   (processes) every time it runs. A supervisor restarting waybar in a crash
   loop accumulates 10 orphans per iteration.
4. If the SIGPIPE assumption is wrong (or a future backend change ignores
   SIGPIPE), orphans are immortal and accumulate without bound.

This is distinct from the in-process reload leak (deinit no-op, same threads
accumulating *inside* one waybar): here the host process is gone and the leak
is at the OS level, surviving even a "clean" recovery.

## Suggested assertions (net-new, workload-side — the plugin cannot observe its own death) — competing designs kept side by side

**Bounded-count `Always`** — two designs for the same invariant; the
difference is a periodic 2× bound (tolerates a draining generation) vs exact
equality at forced-reap checkpoints. The evaluation phase picks.

- Design A (failure-recovery): Workload `Always` (periodic check, 10s grace
  after each restart): message **"live tile-watch process count stays within
  one generation of configured tiles"** — condition: `count(tile-watch procs)
  <= 2 * N_tiles`. **Expected to fail at f87ec19** on quiet desktops after ≥2
  restarts; the property documents the leak and becomes the regression guard
  after a fix (process group + kill, or poll-the-parent in tile-watch).
- Design B (resource-boundaries): Workload `Always` at post-restart
  checkpoints: message **"tile-watch process count equals live module count at
  post-restart checkpoints"** — workload sequence: kill waybar (SIGKILL and
  SIGABRT variants) → restart waybar → force a tiles.json payload change (the
  workload can touch session state or write the cache directly) → settle ~1s
  (75ms poll + write + SIGPIPE delivery) → count all processes matching
  `claude-status tile-watch` system-wide; assert == module count.

**`Sometimes` non-vacuity guards** — both kept; they confirm different
aspects:

- Design A: Workload `Sometimes`: message **"orphaned tile-watch exited via
  EPIPE after host restart"** — confirms the write-gated reaping path actually
  functions when payloads do change (guards the Go-SIGPIPE assumption
  empirically inside the Antithesis environment).
- Design B: `Sometimes(orphan_count > 0 before the payload change)`: message
  **"waybar death left temporarily-orphaned tile-watch producers"** — confirms
  the interesting window (orphans alive, waiting for their reaping write) was
  actually explored rather than orphans dying instantly for an unanticipated
  reason.

## Fault requirements

Requires killing/restarting the **waybar process**. If waybar is the container
entrypoint, this becomes container termination — **node-termination faults
(disabled by default in Antithesis) would be required; flag this**. Recommended
harness shape instead: run waybar under an in-container supervisor (cage/niri
already wrap it in `test/shot.sh`'s topology) so the workload can
`pkill waybar` and let the supervisor respawn it — keeps everything a
process-level fault, no node termination needed.

## Key observations

- The orphan population is workload-visible with a one-liner
  (`pgrep -fc 'tile-watch'`), making this one of the cheapest properties to
  check.
- The same missing-cleanup root cause (no kill-on-drop, detached everything)
  produces both this OS-level leak and the in-process reload leak; a single
  fix (owning the child in a process group, killed from a real
  `deinit`/`Drop`) addresses both. Cross-reference whichever property another
  agent files for reload accumulation.
- The reaping write is *collective*: one tiles.json change reaps every orphan
  whose payload changed — but per-tile payloads differ, so a change touching
  only desktop 3's payload reaps only desktop 3's orphans. The workload's
  forced change must touch every tile key (or the checkpoint assertion must
  count per-tile).
- The `sh -c` intermediary (settled 2026-07-22): `sh` does NOT exec-replace on
  this system (dash vforks and waits), so every chain is `sh` + `claude-status`
  — and the `sh` parent dies immediately when its child dies (observed). Count
  both process names in the workload, or count `claude-status` only and use
  N_chains bounds.
- Interlock with `reload-conserves-producer-chains`: reload (waybar survives)
  leaks chains that are NOT orphans — their pipe read end is still held by the
  leaked reader thread, so EPIPE never comes and this property's reaping
  mechanism *cannot* clean them up. The two properties cover the two disjoint
  lifecycle exits (host death vs module teardown), and their assertion
  messages must stay distinct.

## Open questions

- Does the harness supervisor (cage/session manager) deliver SIGHUP or kill
  the process group when waybar dies? Matters: if children die with the group,
  the orphan window never opens, the Sometimes is unreachable, and the
  property only documents harness behavior, not SUT behavior. Check the
  Antithesis compose topology when it exists.
- Should orphan reaping latency itself be bounded (liveness: "orphan dies
  within X of the next cache write")? `(partial: the SIGPIPE timing input is
  now measured — death 26ms after a cache change, bound ≈ watchPoll 75ms +
  scheduling, so X = 1s is comfortably safe — but whether to file the liveness
  assertion at all is still a design choice)` Matters: a stuck-in-write orphan
  (blocked on a full pipe with no reader — cannot happen after EPIPE, but can
  during the reload-leak scenario) would pass the count check late.
- What is the acceptable steady-state bound after a fix — exactly N_tiles, or
  N_tiles + a draining generation? `(needs human input)` — depends on whether
  the chosen fix kills synchronously at deinit or lets orphans drain via
  EPIPE; the assertion's constant follows the fix design.

### Investigation Log

#### Does the Go SIGPIPE default actually kill tile-watch on first post-death write?

2026-07-22.

- Examined: live probe against the installed `claude-status tile-watch` with a
  scratch `--db` path (RunWatch reads only `dir(--db)/tiles.json` — no niri, no
  DB — so the probe never touched real state). A FIFO reader consumed the
  initial emit and closed the read end (simulating waybar death); the scratch
  cache was then atomically renamed to a changed payload.
- Found: the producer stayed alive while the payload was unchanged (orphan
  window confirmed), then died with wait status **141 = 128+SIGPIPE** — **26ms**
  after the cache change (within one 75ms `watchPoll`). Reaping is exactly
  write-gated as claimed.
- Not found: any SIGPIPE handler/ignore in ~/agentic-db.
- Conclusion: resolved. Orphans are NOT immortal on changing payloads; the
  finding stays "bounded-but-data-dependent orphan lifetime" (immortal only on
  a never-changing payload), and the Sometimes guard "orphaned tile-watch
  exited via EPIPE" is empirically reachable.

#### Does `sh -c 'claude-status tile-watch N'` exec-replace or leave an intermediate `sh`?

2026-07-22.

- Examined: `/bin/sh` → dash 0.5.12-12ubuntu3; ps of the probe's spawned chain;
  `strace -f /bin/dash -c 'sleep 0.1'`.
- Found: dash vforks + execve's the child and waits — it does NOT exec-replace
  even a single simple command on this build. Chain = 2 processes. The `sh`
  parent exited the moment its Go child died (its 141 exit status is how the
  probe observed the SIGPIPE); it does not linger.
- Not found: any scenario where `sh` outlives the dead Go child here.
- Conclusion: resolved. Orphan *process* counts double (2 per chain) but the
  chain dies atomically-enough; the Always bound should be expressed in chains
  (count `claude-status` processes only) or double its constant if counting
  both names. No 3× constant is needed — `sh` does not survive its child.
