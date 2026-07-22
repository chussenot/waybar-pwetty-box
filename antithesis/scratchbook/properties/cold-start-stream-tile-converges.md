# cold-start-stream-tile-converges

Focus: lifecycle transitions — startup ordering: bar up before the daemon's
first cache write (stream mode, the shipped claude configuration).

All suggested assertions are **net-new**; no Antithesis instrumentation exists
anywhere in this codebase (see `existing-assertions.md`).

## Claim

In stream mode (`stream: true`, the claude preset's documented wiring), a tile
that comes up **before** the daemon has ever written `tiles.json` first shows
the empty placeholder, then converges to real session content once the daemon
writes — regardless of startup order between waybar, tile-watch, and the
daemon. This is the honest form of the README's L9 claim ("first real content
within spawn + emit + 150ms of bar start") extended across adversarial startup
orderings.

## Code paths (verified)

Plugin side:

- `src/content.rs:193` — the store starts as `TileContent::default()` (empty
  markup) with `dirty = true`; the first draws render nothing.
- `src/content.rs:256-285` — `spawn_stream`: spawn `sh -c <exec>` once, apply
  each stdout line; on EOF/spawn failure, keep last content and respawn after
  a fixed 1s backoff, forever. So a tile-watch that starts before the daemon
  never strands the plugin — the producer chain is self-healing.
- `src/lib.rs:366-374` — 150ms dirty poll turns a published line into a
  repaint.

Backend side (`/home/chussenot/agentic-db/internal/tile/tile.go`):

- `RunWatch` (tile.go:499-538): emits an initial line **immediately**
  (tile.go:533) — `emptyPayload` if the cache is missing/unreadable or the key
  absent (tile.go:516-521) — then polls the cache every 75ms (tile.go:490) and
  emits on any byte change (tile.go:526-530). So "daemon not up yet" produces
  a placeholder line at t≈0, and the daemon's first cache write produces a
  change → a fresh line → convergence.

Convergence bound when everything is healthy: daemon write + ≤75ms watch poll
+ ≤150ms dirty poll + 1 frame ≈ **250ms** after the first cache write.

## Failure scenario

Start order flipped (bar before daemon — normal at login, guaranteed
explorable under Antithesis scheduling):

1. waybar up → 10 tile-watch children spawn → each emits `emptyPayload` (a
   plausible-looking "long idle" tile — sut-analysis S2: the failure mode
   renders as calm normality).
2. Daemon starts seconds later, writes `tiles.json`.
3. Expected: every tile converges within ~250ms of the write. A live `prompt`
   session that existed before the bar started **must** surface now — this is
   the product's entire purpose.

Known ways the convergence can silently fail (each a finding this property
would catch):

- `claude-status` not on waybar's PATH → `sh` exits 127, stderr nulled,
  respawn every 1s forever, tile permanently blank, zero diagnostics
  (sut-analysis §7). The Sometimes below never fires.
- Reader thread stalled on an invalid-UTF-8 line before the daemon's write
  (covered in depth by `utf8-line-error-respawn-unblocks`).
- Daemon writes an empty cache first (covered by
  `daemon-restart-no-placeholder-clobber` for the restart case; the cold-start
  equivalent is the same `writeTiles`-before-niri-snapshot race on first
  start).

## Suggested assertions (net-new)

Workload-side:

- `Sometimes`: message **"bar started before the daemon's first cache
  write"** — startup-ordering coverage anchor (workload controls/observes
  start order; without this the property can pass without ever testing the
  interesting ordering).
- `Sometimes`: message **"stream tile converged from placeholder to live
  session content after a late daemon start"** — the core liveness condition.
  `Sometimes` is the right type: this is a progress property ("a good thing
  eventually happens"), and the meaningful condition is the
  placeholder→content transition under the flipped ordering, not every
  evaluation.

SUT-side (plugin, cheap):

- `Sometimes` in `ContentStore::set` (`src/content.rs:84-92`) when the
  previous markup was empty/default and the new one is non-empty: message
  **"tile content transitioned from empty to populated"** — makes convergence
  observable without screenshots and per-instance attributable via details
  (tile index).

**Vacuity warning:** a single shared `Sometimes` across 10 instances passes if
*any* instance converges. For a strict per-tile check, the workload should
compare each tile's expected key in `tiles.json` against the per-instance
details payloads, or run a reduced-instance config.

## Fault / harness requirements

- Workload must control process start order (start waybar first, delay the
  daemon) — plain orchestration, no special faults.
- Antithesis scheduling faults naturally widen the window between tile-watch's
  initial placeholder emit and the daemon's first write.

## Key observations

- Stream mode is self-healing by construction (respawn loop + tile-watch's
  75ms cache poll + change-triggered emit). The property's value is guarding
  the three silent-failure holes above, which all present as "placeholder
  forever" — indistinguishable, without this check, from a genuinely idle
  desktop.
- Contrast with `poll-mode-cold-start-converges`: the same startup ordering
  under the *default* config (`stream: false`, `interval: 0`) has no healing
  mechanism at all — split into its own property because the invariant
  outcome differs.

## Open questions

- What should the workload treat as "live session content" vs placeholder?
  The `emptyPayload` renders via the empty-preset markup (single idle session,
  max decay); matching on rendered markup vs matching on the JSON line
  consumed decides where the assertion hooks. Cheapest: assert on the JSON
  payload at the `ContentStore::set` hook (is it `emptyPayload`-shaped?)
  rather than on markup.

### Investigation Log

#### Does the daemon's first-ever writeTiles (fresh install, no cache file) have the same empty-model race as the restart case?

Investigated 2026-07-22.

- Examined: daemon startup ordering in
  /home/chussenot/agentic-db/internal/daemon/daemon.go — `run()`
  (daemon.go:180-249), `pollDB`'s immediate prime (daemon.go:283
  `d.sendSnapshot(ctx, out)` before the ticker loop), `maybeWriteTiles`
  (daemon.go:382-389, `lastTileBuild` zero on a fresh daemon), `writeTiles`
  (daemon.go:397-419), `applyEvent`/`adoptExistingNames` (daemon.go:254-275),
  `niri.StreamEvents` child spawn (eventstream.go:148-182); consumer side
  `RunWatch.emit` (tile.go:514-531).
- Found: YES — the code path is identical; `writeTiles` has no
  populated-model guard and no prior-cache check, so on a fresh install the
  sequence "DB prime → 13ms debounce → reconcile → writeTiles over an empty
  model" writes `{}` as the FIRST-EVER cache whenever the niri event-stream
  child (async `niri msg -j event-stream` spawn) hasn't delivered its initial
  `WorkspacesChanged` yet. The dedupe does not stop it: `lastTiles` starts
  nil, `bytes.Equal("{}", nil)` is false (daemon.go:411-413). BUT the
  consumer-visible consequence differs from the restart case: with no prior
  cache, tile-watch was already emitting `emptyPayload` (missing file →
  `ReadCache` error → placeholder, tile.go:516-521); after `{}` lands,
  `ReadCache` succeeds but the key misses → the same placeholder → the
  string dedupe (`s != last`, tile.go:526) suppresses any new line. The `{}`
  write is placeholder-to-placeholder, invisible on the stream.
- Not found: any guard ordering the first write after model adoption; any
  path where the `{}` first write produces a non-placeholder line. One
  narrow caveat: if a reconcile lands in the gap between the initial
  `WorkspacesChanged` and `WindowsChanged` events, the intermediate cache
  holds real keys mapping to per-workspace `emptyPayload`s (with `active:
  true` on the focused one) — an extra placeholder-shaped line with a
  different `active`/`shortcut` byte pattern, still not a content transition.
- Conclusion: RESOLVED — same race, but for the no-prior-cache path it is
  masked (nothing to clobber; placeholder either way). The workload's
  convergence check can expect a **single placeholder→content transition** at
  the consumed-line level, phrased as "only placeholder-shaped lines until
  the first populated line" rather than "exactly one line" (the
  workspaces-before-windows gap can emit an extra placeholder variant under
  adversarial scheduling). The restart property
  (`daemon-restart-no-placeholder-clobber`) remains the only case where the
  race clobbers real state.
