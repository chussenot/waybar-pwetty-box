# respawn-backoff-floor-holds — evidence

Found independently by 2 discovery focuses (resource-boundaries + failure-recovery) — merged during synthesis; independent rediscovery is a confidence signal.

No Antithesis instrumentation exists anywhere in this codebase (see
`existing-assertions.md`); every assertion suggested here is net-new.

## Claim

For each stream-mode module instance, consecutive producer spawn attempts are
separated by at least `RESPAWN_BACKOFF` (1s) — the codebase's claimed
guarantee S12 ("an immediately-exiting command can't busy-loop",
`src/content.rs:250-255`, README) — and once the failure cause clears, the
producer recovers completely. This is the one deliberate backpressure
mechanism in the whole system; the property pins it under fault injection.

## Code paths (verified at f87ec19)

- `src/content.rs:256-285` `spawn_stream`: infinite loop —
  `Command::new("sh").arg("-c").arg(&cmd)` with stderr nulled → read lines →
  `child.wait()` → **unconditional** `thread::sleep(RESPAWN_BACKOFF)` →
  respawn. Two failure classes converge on the same loop tail:
  - **Spawn `Err`** (fork/exec of `sh` itself fails: EMFILE/ENOMEM/ENOENT for
    sh): logged to stderr (`content.rs:280`), then backoff.
  - **Instant child exit** (e.g. `claude-status` not on PATH → sh exits 127;
    child stderr is **nulled** at `content.rs:265`, so the 127 diagnostic is
    discarded): reader sees immediate EOF, `wait()` (content.rs:278), then
    backoff.
- `src/content.rs:283` — `thread::sleep(RESPAWN_BACKOFF)` is at the **loop
  tail, unconditional**, so both classes are rate-limited to ≤1 spawn/sec/tile.
  This placement is the entire storm defense — there is no exponential
  backoff, no failure cap, no jitter, no failure counter, no health surfacing,
  and no diagnostics beyond one stderr line per spawn failure (S12 claim).
- Persistent failure mode (sut-analysis §7): `claude-status` not on waybar's
  PATH → `sh` exits 127 immediately, stderr nulled → ~1 spawn/s per tile
  forever (~86k forks/day/tile), tile permanently blank, zero diagnostics.
  Live deployment multiplies by 10 instances: persistent failure = **10
  forks/sec, ~86k forks/day**, silently. That *rate* is by design; the
  property asserts the rate never exceeds the design floor.

## Failure scenarios the property guards against

1. **Persistent-failure storm**: break the producer (rename the binary,
   exhaust fds, fill the disk). Expected: spawn attempts stay ≤1/s per tile.
   Any future refactor or fault-window that skips the sleep — e.g. a refactor
   that moves it inside the `Ok` branch, an early `continue` on a new error
   branch, or a panic-restart path — converts every failure into a hard
   busy-loop fork bomb: thousands of `sh` forks per second from inside the GTK
   host's process tree, starving the host and the machine. That regression is
   invisible in unit tests (the only stream test is happy-path,
   `content.rs:377`). The current code holds by construction (8 lines,
   unconditional sleep) — the value is regression-pinning plus surfacing the
   *aggregate* violation that the reload leak causes (see coupling below).
2. **Recovery after the cause clears**: restore the binary / release fd
   pressure. Next 1s cycle spawns successfully; `tile-watch` emits its initial
   line immediately (`/home/chussenot/agentic-db/internal/tile/tile.go:533`);
   tile converges within ~1.2s of the cause clearing. No cached failure state
   exists in this path (unlike the icon negative-cache), so recovery should be
   complete — that completeness is the claim to verify under fault timing
   (e.g. the cause clears *between* `spawn()` and the child's first exec).

## Suggested assertions (net-new) — competing designs kept side by side

**SUT `Always` (authoritative — both focuses converged on the same design):**
timestamp the top of the spawn loop; assert `now - last_attempt >=
RESPAWN_BACKOFF` on every iteration after the first (allow small scheduler
slack, e.g. ≥0.9s). Message formulations: **"stream respawn attempts are
spaced at least RESPAWN_BACKOFF apart per reader thread"** / **"consecutive
stream spawn attempts spaced at least RESPAWN_BACKOFF apart"**. This is the
storm-boundedness invariant, per-tile (10 instances each assert independently
— the message stays unique because it is one callsite in shared code).

**SUT `Sometimes`** — two designs asserting different transitions; both are
meaningful, evaluation picks (or keeps both):

- Design A (resource-boundaries): `Sometimes(respawned && new content
  published)`: message **"stream producer respawned after exit and the tile
  reconverged"** — the recovery half of claim L2; a meaningful semantic state
  (producer death is survivable) that Antithesis should reach via process-kill
  fault injection on tile-watch.
- Design B (failure-recovery): `Sometimes` at the transition failed-cycle →
  successful publish: message **"stream respawn succeeded after at least one
  consecutive failure"** — requires a small consecutive-failure counter (also
  the natural retry-outcome instrumentation point this codebase entirely
  lacks: today failure count, failure age, and last error are all
  unobservable).

**Workload `Always` (coarser)** — two designs with different granularity:

- Design A (per-tile): message **"per-tile producer fork rate stays at or
  below one per second"** — count fork/exec events of the tile exec
  (process-table polling at ~100ms, or `/proc` loop) per tile identity over a
  sliding window.
- Design B (aggregate, environment-level): total fork rate of `sh`/`tile-watch`
  across the container stays ≤ `1.2 × N_tiles`/s over any 30s window: message
  **"stream respawn fork rate stays within backoff budget"** — catches storms
  even if SUT instrumentation is bypassed (e.g. panicking reader thread
  respawned by some future supervisor logic).

## Fault requirements

Resource faults only (fd exhaustion, ENOSPC, memory pressure) plus
workload-driven PATH/binary breakage mid-run. **No node-termination faults
required.**

## Key observations

- **Coupling with `reload-conserves-producer-chains`**: after N reloads, N+1
  leaked reader threads per tile each legally spawn at 1/s — the per-thread
  floor holds while the per-tile aggregate rate is (N+1)/s. The workload-level
  per-tile assertion therefore fails whenever the reload leak is present; the
  SUT-side per-thread assertion isolates S12 itself. Keep both messages
  distinct to avoid conflating the two defects in triage.
- The spawn-failure path (`Err` at content.rs:280) and the fast-exit path (EOF
  at content.rs:278) are different branches that happen to share the sleep;
  fault injection should exercise both (unlink the exec binary vs make it
  `exit 0` immediately).
- The 1s fixed backoff is simultaneously the storm defense and the recovery
  latency — any future "make recovery faster" change is exactly the change
  most likely to break the storm bound; these assertions make that trade
  explicit.
- Spawn failure and instant-exit are visually identical to an empty desktop
  (blank tile) — there is no `Sometimes`-worthy user-visible signal to hook,
  which is why the instrumentation must live in the loop itself.
- Fork counting (settled 2026-07-22): `sh -c` on this system (dash 0.5.12)
  does NOT exec-replace a single simple command — it vforks and waits — so
  every spawn attempt creates **two** processes (`sh` + the exec'd command).
  Workload fork counters must expect 2 forks per attempt (or count one name);
  the invariant and the 1/s floor are unchanged.

## Open questions

- Is process-table polling fast enough to falsify the floor from the workload
  side, or is the SUT-side timestamp assertion the only reliable
  implementation? Matters: decides whether this property costs any SUT change
  at all; a busy-loop violation (100s of forks/s) is easily caught by polling,
  a marginal one (e.g. 900ms spacing) is not.
- Under what real conditions does `Command::new("sh").spawn()` itself fail vs
  succeed-then-127? Why it matters: only the `Err` branch logs anything today;
  if Antithesis resource faults mostly manifest as the silent 127 path, triage
  of a failed run has zero SUT diagnostics to correlate — which argues for
  adding a (rate-limited) log or counter on the instant-exit path as part of
  instrumentation, not just assertions.

### Investigation Log

#### Does `sh -c '<single command>'` exec-replace itself or fork a child?

2026-07-22.

- Examined: `/bin/sh` and `/usr/bin/sh` (both → dash 0.5.12-12ubuntu3);
  `strace -f -e trace=fork,vfork,clone,execve /bin/dash -c 'sleep 0.1'`; ps of
  a live `sh -c 'claude-status tile-watch --db <scratch> N'` chain.
- Found: dash **forks** (vfork + child execve) and stays as the waiting parent
  — no exec-replace, contradicting the common "dash execs simple -c commands"
  assumption for this build. Observed chain: `sh` (parent) + `claude-status`
  (child). The parent exits immediately when the child dies.
- Not found: any exec-replacement under any probed variant (with/without
  output redirection, absolute or PATH-resolved command).
- Conclusion: resolved. Two observable processes (and 2 forks) per spawn
  attempt; workload counter expectations double, invariant unchanged. Note
  the harness container's `/bin/sh` may differ (busybox ash, other dash
  builds) — re-check the count constant there if counting forks exactly.
