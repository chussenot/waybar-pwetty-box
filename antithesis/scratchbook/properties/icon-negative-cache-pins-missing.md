# icon-negative-cache-pins-missing

Focus: idempotency and replay — re-processing the same payload line must
converge to the correct render. The permanent negative icon cache makes the
displayed pixels depend on **draw history** (filesystem state at the first
draw that referenced each icon key), breaking "displayed state is a pure
function of (last payload line, config, time)".

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

- `src/lib.rs:1195-1203` — `ICON_CACHE`: `thread_local!`
  `HashMap<(String, u32, Option<u32>), Option<Vec<u8>>>` keyed by
  (source, device px, tint). `None` is a **cached failure**. Never evicted; no
  mtime or existence in the key. Because it is thread-local on the single GTK
  main thread, one negative entry is shared by **all 10 module instances**.
- `src/lib.rs:1300-1315` — `raster_svg_cached`: `entry().or_insert_with(...)` —
  insert-once, including the failure case. No retry path exists.
- `src/lib.rs:1248-1252` — the loader for `<icon src>`: `std::fs::read(s).ok()`
  — *any* read error (ENOENT, EACCES, transient EIO/EMFILE) becomes the
  permanent `None`. No log line either (sut-analysis §2, confirmed).
- Backend shim (validated in /home/chussenot/agentic-db):
  - `internal/tile/tile.go:309-352` — `wrapPNGAsSVG` writes the SVG shim to
    `~/.cache/claude-status/icons/<app>.svg` **synchronously, atomically
    (tmp+rename), before** the path enters any payload. **Correction to
    sut-analysis §2**, which calls this "the backend's async PNG→SVG shim":
    the write is synchronous within payload construction, so the
    "payload references the file before it exists" race does **not** exist in
    the write direction. The real trigger is external file loss (below).
  - `internal/tile/tile.go:206-263` — `iconMemo` (`sync.Map`) memoizes
    appID→path **forever** in the daemon; if the shim file is deleted, the
    memo keeps returning the dead path and `wrapPNGAsSVG` is never re-invoked
    to regenerate it until the daemon restarts.

## Failure scenario

1. Shim exists; payloads reference `~/.cache/claude-status/icons/kitty.svg`;
   tile renders the icon.
2. A cache cleaner (or the user, or fault injection) wipes
   `~/.cache/claude-status/icons/`.
3. Next draw with a **new** cache key — e.g. after a waybar restart, a
   device-scale change, or a `cap_h` change minting a fresh
   `(path, px, tint)` — reads the missing file → `None` cached permanently.
4. Daemon restarts and regenerates the shim at the same path. The plugin
   **still renders no icon**: the negative entry short-circuits the read
   forever. Only a waybar restart (or, accidentally, another px-key change)
   heals it. Re-delivery of the identical payload — the pipeline's replay
   mechanism — cannot repair the render.

Three-layer non-convergence (sut-analysis F7, confirmed): daemon `iconMemo`
(never re-stats) → cache-dir shim file (regenerated only on daemon restart) →
plugin negative cache (never retried). Each layer is individually
"write-once"; composed, a single transient filesystem event outlives both
processes' recovery mechanisms.

## Suggested assertions (net-new)

- SUT `AlwaysOrUnreachable` in `raster_svg_cached` / `draw_icon_alpha`: on a
  negative-cache hit for a `src:`-keyed entry, `stat` the path; assert it does
  **not** exist: message
  **"cached icon-load failure never masks a readable icon file"**.
  `AlwaysOrUnreachable` because the path only runs when the workload's markup
  carries `<icon src>` and a load has previously failed; when it runs, a
  readable file behind a negative entry is exactly the bug. Currently
  violated whenever the pin scenario occurs → immediate finding under the
  fault below.
- Workload `Sometimes`: delete the icons dir mid-run, then restore/regenerate
  it (restart the daemon), and assert the icon eventually reappears in the
  rendered output: message
  **"icon render recovered after icons cache dir was wiped and regenerated"**.
  At f87ec19 this never becomes true (documents the wedge); after a fix
  (mtime/existence-keyed retry or TTL) it becomes the regression guard.

## Fault requirements

Filesystem manipulation from the workload (delete/restore files in the icons
cache dir) plus daemon restarts. Antithesis fault injection on file reads
(transient EIO/EMFILE) reaches the same pin **without** deletion — the file
"exists" the whole time, making the SUT-side assertion fire cleanly.

## Key observations

- The negative cache converts a *transient* read failure into a *permanent*
  render divergence — strictly worse than no cache for the failure case, while
  the success case (avoid re-rasterizing per frame on animated tiles) is the
  legitimate need. A fix only has to re-check existence/mtime on negative
  entries; positive entries can stay permanent.
- Severity is S5-cosmetic for app logos, but the same `raster_svg_cached` path
  serves `<icon name=...>` bundled icons and the status mascot sizing chain —
  a transient failure at first draw of any key pins it for the process
  lifetime of waybar (days/weeks on a desktop).

## Open questions

- What is the realistic non-injected trigger rate for cache-dir loss (does the
  deployment run systemd-tmpfiles or a cache cleaner over `~/.cache`)? Decides
  real-world priority, not validity — under Antithesis the trigger is trivial
  either way.
- Does the live deployment ever actually change device scale at runtime
  (monitor swap / mixed-DPI dock)? `(partial: the MECHANISM is confirmed — a
  scale change mints new (path, px, tint) keys that silently heal or re-pin,
  see Investigation Log; whether this deployment's hardware ever triggers it
  is a deployment fact, not a code fact.)` If yes, symptoms are
  intermittent/confusing in production, which raises the value of the
  SUT-side assertion as the only reliable observer.

### Investigation Log

#### Does a device-scale change mint new (path, px, tint) keys in ICON_CACHE (silently healing or re-pinning)?

Investigated 2026-07-22 at f87ec19.

- Examined: waybar-pwetty-box `src/lib.rs` — the cache key type
  (lib.rs:1195-1203), `draw_icon_alpha`'s key/px construction
  (lib.rs:1236-1252), `raster_svg_cached`'s insert-once entry
  (lib.rs:1300-1315), the mascot path (lib.rs:1327-1343), and where `scale`
  originates (lib.rs:232, `let scale = area.scale_factor().max(1);` — GTK's
  integer device scale factor per widget, re-read on every draw).
- Found: YES, mechanically certain. The cache key is `(key, px, tint_key)`
  and `px = ((cap_h * scale).round() as u32).max(1)` (lib.rs:1246; mascot:
  `px = ((box_side * scale).round() as u32).max(1)`, lib.rs:1339). `scale`
  is the live `scale_factor()` of the widget, so moving the bar to a
  different-scale output (or the output's scale changing) changes `px` for
  every icon (integer scale 1→2 at least doubles it) → `entry((key, px,
  tint)).or_insert_with(load)` misses → `load()` re-runs `std::fs::read`.
  If the file has reappeared since the old failure, the new key gets a
  positive entry — the render silently heals while the old negative entry
  sits dead in the map; if the file is still missing, a fresh negative entry
  is minted at the new px — a re-pin. Nothing is ever evicted (the map only
  grows), and `cap_h` changes have the same key-minting effect.
- Not found: any subscription to scale-factor changes that clears or
  revalidates entries; any eviction at all.
- Conclusion: RESOLVED (mechanism) — scale changes are an accidental,
  partial healing channel, which makes production symptoms intermittent and
  hard to attribute, exactly the failure-shape that motivates the SUT-side
  `AlwaysOrUnreachable` (stat-on-negative-hit) as the only deterministic
  observer. Property invariant/assertions unchanged. Residual question
  (whether this deployment's hardware ever changes scale) re-tagged
  `(partial: ...)` above — it affects real-world priority only.
