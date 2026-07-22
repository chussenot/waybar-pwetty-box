# tile-cache-never-torn — tiles.json publish atomicity holds only under exactly one writer

Found independently by 2 discovery focuses (coordination + data-integrity) — merged during synthesis; independent rediscovery is a confidence signal.

Focus: the single-daemon assumption behind the tile cache's atomic-rename
discipline.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Claim under test

sut-analysis §2 states "tiles.json … written atomically (tmp+rename,
byte-dedupe, tile.go:431-437) — torn reads ruled out." Spot-checked: the
rename is atomic, but the *tmp name is fixed*, so the "torn reads ruled out"
conclusion is conditional on there being exactly one writer process — and
nothing enforces that.

## Code paths (all in /home/chussenot/agentic-db unless noted)

- `internal/tile/tile.go:431-437` — `WriteCacheBytes`: **fixed tmp name**
  `tmp := path + ".tmp"`, then `os.WriteFile(tmp, data, 0o644)` (open O_TRUNC
  + write + close, no fsync), then `os.Rename(tmp, path)`. Comment at
  tile.go:418-419 claims "a concurrent reader never sees a half-written file"
  — true for readers vs ONE writer, false for two writers.
- `internal/daemon/daemon.go:70-112` — daemon `Run`: **no singleton
  enforcement whatsoever** — no lockfile, no pidfile, no socket bind, no flock
  on the DB or cache. Grep for lock/pid/flock across `internal/daemon` (and
  flock/lockfile/pidfile across agentic-db) finds nothing. Two daemons started
  against the same `--db` share `CachePath` (`internal/tile/tile.go:85-87` —
  `tiles.json` next to the DB); two daemon processes can run concurrently
  (manual double-start, restart overlap).
- `internal/daemon/daemon.go:397-419` — `writeTiles`, the only production
  caller of `WriteCacheBytes` (daemon.go:414), runs on the single actor
  goroutine — so *within one daemon* there is exactly one writer. But its
  dedupe (`lastTiles`) is **per-process** memory; two daemons each believe
  they are the sole writer.
- Contrast inside the same file: `internal/tile/tile.go:339-355` — the icon
  SVG shim writer uses `os.CreateTemp(dir, name+".*.tmp")` (unique tmp)
  **because** "the daemon and a cacheless CLI run can resolve the same app
  concurrently" (comment at tile.go:339-340) — the author applied unique-tmp
  discipline where multi-writer was anticipated. `WriteCacheBytes` did not get
  the same treatment. Discriminating evidence that the fixed tmp is an
  oversight resting on an unenforced single-daemon assumption, not a
  deliberate choice.
- Readers: `internal/tile/tile.go:440-450` `ReadCache` (`os.ReadFile` +
  `json.Unmarshal`), polled every 75ms by every `tile-watch` / `RunWatch`
  (tile.go:490, 499-538, 517) and once per `tile-data` run (tile.go:471).

## Failure scenario

Two daemons alive (restart overlap, manual second start, a future systemd unit
coexisting with niri `spawn-at-startup` — nothing prevents any of these). Both
independently-found interleavings preserved:

Variant 1 (shared-inode overwrite):

1. Daemon A opens `tiles.json.tmp` (O_TRUNC), gets inode X.
2. Daemon B opens the same path — **same inode X, truncated again**.
3. B writes dataB, closes, renames tmp → `tiles.json`. The live cache is now
   inode X.
4. A — still holding an fd to inode X — writes dataA at offset 0: **A is now
   writing directly into the live `tiles.json`**, in place, with readers
   polling it every 75ms. If `len(dataA) < len(dataB)` the stable result is
   dataA followed by the tail of dataB — torn JSON published under the
   "atomic" discipline. A reader can also catch A mid-write.
5. A's own rename then fails ENOENT (tmp already renamed away) — logged and
   swallowed (`daemon.go:414-416`).

Variant 2 (publish-during-write):

1. A: `os.WriteFile(tmp, dataA)` completes.
2. B: `os.WriteFile(tmp, dataB)` opens the same fixed tmp path with O_TRUNC
   and begins writing.
3. A: `os.Rename(tmp, path)` — publishes the inode B is *still writing into*.
4. B's remaining writes now mutate the **published** `tiles.json` in place.
   Readers can observe: an empty file (right after B's truncate+rename
   ordering variants), a partial JSON prefix, or a file whose bytes change
   under a non-atomic `os.ReadFile`.

POSIX guarantees rename atomicity of the directory entry, not quiescence of
open file descriptors — the fd B holds follows the inode across the rename. So
the "reader never sees a half-written file" comment (tile.go:418-419) is false
under two writers.

Simpler interleave (no rename involved): A open, B open, A write, B write —
file is dataB overlaid by whichever tail is longer. Either daemon's rename
publishes the mix.

Downstream: `ReadCache` unmarshal fails → `tile-watch` / `RunWatch` `emit()`
takes the error branch (tile.go:515-521 / 516-521) → **emptyPayload (idle
level-6) pushed over whatever state was live**, including `prompt` — the F9
silent-staleness mask; a live `prompt` renders as idle placeholder. See
`cache-error-demotes-live-tile` for that consumer-side amplification step and
`daemon-restart-no-placeholder-clobber` for the adjacent {}-clobber race.

Secondary consequence even when no write is torn: two healthy daemons with
momentarily divergent models (different DB-poll/tick phases) alternate
last-writer-wins cache contents → watchers emit on every flap (75ms cadence) →
waybar repaint storm. Also both daemons act as "sole mutator of niri workspace
names" (daemon.go package comment) — dueling renames; out of scope here but
same missing-singleton root cause.

## Suggested assertions (net-new) — designs kept side by side

**Reader-side `Always` (both focuses converged on the same design):** in
`ReadCache` (tile.go:440-450), or at the `RunWatch` call site: when
`os.ReadFile` succeeds, `json.Unmarshal` must succeed — evaluated on every
successful file read (every 75ms per tile-watch, ~13×/s per watcher, so
coverage is free). ENOENT is explicitly out of scope (missing cache is a
legitimate pre-daemon state); only successful-read-then-parse-failure
violates. Message formulations: **"tile cache reads always parse as valid
JSON"** / **"tile cache read: successful read of tiles.json parsed as a
complete payload map"**.

- SUT `Sometimes` on the rename-error branch of `WriteCacheBytes`
  (tile.go:436 error path): message **"tile cache tmp rename lost a concurrent
  race"** — cheap detector that a second writer existed; also an exploration
  hint toward the interleaving.
- Optional writer-side companion (data-integrity design): take an `flock`
  (non-blocking) around `WriteCacheBytes` and `Unreachable`-assert the
  contended branch — but the reader-side parse check is the minimal,
  sufficient detector.
- Workload `Sometimes`: message **"two daemons wrote the tile cache
  concurrently"** — coverage guard; the workload must actually create writer
  overlap (start a second daemon, or restart-before-stop) or the Always is
  near-vacuous for this hazard.

## Antithesis angle / fault requirements

Process-level only; no node-termination faults needed:

- Workload starts two daemon instances (or restarts the daemon with overlap)
  and lets the scheduler interleave the two `WriteFile` calls with the
  readers' 75ms polls; Antithesis scheduler pauses between
  `os.WriteFile(tmp)` and `os.Rename` (and between open and write inside
  WriteFile) widen the race windows arbitrarily.
- Process-pause (SIGSTOP) one daemon mid-`WriteFile`, let the other publish,
  resume — widens the publish-during-write steps from microseconds to
  arbitrary durations.
- Kill a daemon between `WriteFile` and `Rename`: leaves a stale fixed-name
  `.tmp`; the next daemon's write reuses the path (harmless alone, but part of
  the same discipline).

## Key observations

- A torn write can also produce **parseable-but-mixed** JSON in principle
  (tail alignment); the parse assertion cannot catch that. The
  flapping/divergence consequence is partially observable via the watcher emit
  storm; a full "cache content matches exactly one daemon's model" check is
  not practical. The parse invariant is the sharp, checkable core.
- Interleaved-but-valid JSON (B fully overwriting before A's rename) publishes
  B's coherent data under A's rename — content-correct, no violation; the
  parse check correctly does not fire.
- Single-writer operation genuinely satisfies the invariant: the daemon actor
  goroutine is the only caller of `writeTiles` (daemon.go:414 is the sole
  `WriteCacheBytes` caller), and kill-between-write-and-rename merely orphans
  a tmp file that the next `WriteFile` truncates. The property only bites when
  the unenforced singleton assumption breaks — which is exactly what it is
  for.
- `os.WriteFile` does not fsync; crash-before-rename loses the tmp only (old
  cache intact) — not a defect. Crash-after-rename-before-data-flush could
  publish empty content on power loss, but the daemon rewrites on restart
  because `d.lastTiles` (daemon.go:411, 418) starts nil, so it self-heals.
- This SUT has no consensus protocol, no election, no quorum: "split-brain"
  here is precisely two writers both believing they own one file. Fix shapes:
  unique tmp via `os.CreateTemp` (mirrors tile.go:341, removes torn writes but
  keeps flapping) or a real singleton lock (removes both).

## Open questions

- Is dual-daemon operation considered operator error, or should the daemon
  enforce a singleton (flock on the DB/cache dir)? `(needs human input)` — if
  operator error, the assertions still hold value as detectors during restart
  overlap; if enforcement is intended, the property gains a companion ("second
  daemon refuses to start") and the workload changes from "make them collide"
  to "verify the lock".
- Can two daemon instances realistically coexist in the deployed setup? Does
  any supervisor in the eventual deployment (niri spawn-at-startup vs a future
  systemd unit) allow restart overlap (start-new-before-stop-old)? No lock
  exists and no systemd unit ships (sut-analysis §7 "no restart unit ships"),
  so supervision is manual — overlap is plausible but unconfirmed. If the
  answer is "impossible by deployment convention," the property still stands
  as the guard for that convention, but its priority drops; if daemons are
  ever supervised with restart-on-crash, overlap windows become routine and
  this is a live corruption path. Matters: decides whether the workload's
  dual-writer scenario models a real deployment path or only a manual misuse.

### Investigation Log

#### Is dual-daemon operation considered operator error?

- Examined: `internal/daemon/daemon.go` Run/run (full startup path),
  `internal/tile/tile.go` cache read/write, grep for lock/pid/flock/singleton
  across `internal/`, `share/` (systemd units: only recap timers ship),
  package comment in daemon.go.
- Found: no enforcement, no documentation of a singleton requirement, and a
  contrasting unique-tmp pattern at tile.go:341 showing multi-writer was
  handled elsewhere.
- Not found: any statement of intent about concurrent daemons.
- Conclusion: tagged `(needs human input)` — code and docs are silent on
  intent; only the owner can say whether enforcement belongs in the daemon.
