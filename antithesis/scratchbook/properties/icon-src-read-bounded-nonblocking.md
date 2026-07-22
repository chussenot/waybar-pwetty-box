# icon-src-read-bounded-nonblocking

## What this is

`<icon src="…"/>` performs a **synchronous, unbounded `std::fs::read`** of a
data-controlled path on the **GTK main thread**, inside the `connect_draw`
callback.

## The trust-boundary defect

The path is data-controlled at the plugin's trust boundary (the NDJSON pipe).
The bundled claude template emits it directly from the `app_icon` data field
(tiles/claude/tile.json:7):

```
{% if '/' in app_icon %}<icon watermark='1' src='{{ app_icon }}'/>{% else %}…
```

`app_icon` is a schema field documented as "an absolute path to an .svg"
(tiles/claude/schema.json:95-96). Autoescape protects the *markup* (quotes/
angle brackets), but a filesystem path needs none of those characters, so the
raw path flows into `src`. The plugin then:

1. detects the leading watermark/hero icon in `draw_flow` (src/lib.rs:765-793,
   837-845),
2. calls `draw_icon_alpha` → `raster_svg_cached` (src/lib.rs:1300-1315),
3. whose load closure runs `std::fs::read(s).ok()` (src/lib.rs:1248-1252)
   — **on the main thread, no timeout, no size cap, no file-type check.**

`connect_draw` is registered on the GTK main thread (src/lib.rs:231, 260); all
FFI/draw work is main-thread (SUT §3). So this read blocks the one thread that
paints **every** tile on the bar.

### Failure scenarios

- **FIFO / device / slow path → permanent bar wedge.** `fs::read` on a FIFO
  blocks until a writer sends EOF; on a slow/networked path it blocks for the
  I/O duration. The draw callback never returns → all 10 tiles freeze → the bar
  is dead until waybar restarts. The negative cache (below) never even records
  it, because the *first* read never completes.
- **Huge regular file → per-frame stall + memory spike.** No size cap: a
  multi-GB path is fully read into a `Vec<u8>` on the main thread. Caches on
  success per `(key, px, tint)` (src/lib.rs:1309-1314), but `px` is
  `cap_h * scale` so a scale/resize change re-triggers the read.
- **Arbitrary-file read into the raster path.** Any path the process can open
  is read; only SVG-parseable bytes render, but the read itself is the primitive.

### Backend mitigation is out of the SUT's control

The shipped backend `resolveAppIcon` (agentic-db internal/tile/tile.go:245-277)
only emits paths to `.svg` files it found by `os.Stat` under known icon-theme
dirs, or a cache-shim path it wrote — so *in normal operation* the value is
constrained. But this is a mitigation in a **separate, independently-buggy
process** (6 state-derivation fixes in its history, SUT §5) that the plugin
neither controls nor validates. `iconCandidates` (tile.go:364) interpolates the
raw niri `app_id` into `filepath.Join(dir, c+".svg")` with no `..`/`/`
scrubbing, so a crafted app_id could in principle traverse to any existing
`.svg`. And any other configured exec producer bypasses the backend entirely.
The property is about the **plugin's** unconditional trust of the path, which is
real regardless of what the current backend happens to send.

## Code paths

- data-path fs::read (uncapped, main thread): src/lib.rs:1248-1252
- cache (caches `None` failures forever; never populated on a blocking read):
  src/lib.rs:1300-1315
- watermark/hero icon draw in the draw callback: src/lib.rs:765-793, 837-851
- draw callback is main-thread: src/lib.rs:231, 260
- template emits data-controlled src: tiles/claude/tile.json:7
- schema documents app_icon as a path: tiles/claude/schema.json:95-96
- backend derivation (mitigation, other repo): agentic-db tile.go:245-277, 364
- SUT analysis flag: §11 F8, §12 ranked surface

## Suggested assertions (net-new)

- `AlwaysOrUnreachable`, placed **before** the `fs::read` (so a blocking FIFO
  can't deadlock past it): `fs::metadata(s)` shows a regular file whose length
  ≤ a fixed cap (e.g. a few MB). The path is optional, so a run may never hit
  it — but any hit must satisfy the guard. This documents the missing
  regular-file + size guard.
- Companion `Reachable`: "draw-path `<icon src>` filesystem read executed" — so
  Antithesis confirms it reached (and is exploring) the primitive.

## Open questions

- Should the read move off the main thread entirely (async load + repaint on
  completion), or is a bounded synchronous read acceptable for a personal tool?
  Why it matters: decides whether the property is "bounded+regular-file only"
  (Always) or "never synchronous on the main thread" (a stronger structural
  property). `(needs human input)` — a design call.
- Can a niri `app_id` drive `resolveAppIcon` to emit a traversing path to a
  non-icon `.svg`? Mechanism (unsanitized `filepath.Join`) is present; requires
  a `.svg` to exist at the target. What changes: whether the backend is a
  reliable mitigation or itself a vector. Lives in the other repo; noted, not
  fully traced here.

### Investigation Log

#### Should the read move off the main thread, or is a bounded synchronous read acceptable?

Investigated 2026-07-22.

- Examined: the read itself (`std::fs::read(s).ok()`, src/lib.rs:1248-1252),
  the raster cache (src/lib.rs:1300-1315), the watermark/hero draw sites
  (src/lib.rs:765-793, 837-851), main-thread registration of the draw callback
  (src/lib.rs:231, 260), template ingress (tiles/claude/tile.json:7), schema
  documentation of `app_icon` (tiles/claude/schema.json:95-96), backend
  derivation (agentic-db tile.go:245-277, 364), sut-analysis §11 F8/§12;
  README and AGENTS.md for any stated threading or latency contract for icon
  loads.
- Found: the mechanism as described in this file — a synchronous, uncapped,
  main-thread read of a data-controlled path, with no timeout, size cap, or
  file-type check, and a cache that is never populated when the first read
  blocks.
- Not found: any statement of intended behavior — no comment, doc, or bead
  declares the synchronous read a deliberate simplification, names an
  acceptable size/latency bound, or requires async loading.
- Conclusion: tagged `(needs human input)` — design call for the owner. The
  answer decides the property's shape: bounded+regular-file-only
  (`AlwaysOrUnreachable` as suggested) vs the stronger structural property
  "never synchronous on the main thread".
