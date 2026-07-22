# content-snapshot-torn-read — a rendered frame reads markup and uniforms from the same content generation

Found independently by 2 discovery focuses (data-integrity + concurrency) — merged during synthesis; independent rediscovery is a confidence signal.

All suggested assertions here are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Claim under test

sut-analysis §3 identifies "the draw callback reads `uniforms()` and `markup()`
under two separate lock acquisitions … a `set()` in between renders one frame
with uniforms from generation N and markup from N+1." Spot-checked in code by
both focuses independently — confirmed exactly as described.

## Code paths

- `/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/antithesis-research/src/lib.rs:234-237`
  — draw callback, first lock acquisition:
  `content_draw.as_ref().map(|s| s.uniforms())` clones the uniform vec under
  the content mutex.
- `/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/antithesis-research/src/lib.rs:247`
  — ~10 lines later (after borrowing the engine and computing sizes) the draw
  callback acquires the lock a second time:
  `content_draw.as_ref().map(|s| s.markup())` clones the markup string under a
  *separate* acquisition of the same mutex.
- `/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/antithesis-research/src/content.rs:100-115`
  — `markup()` and `uniforms()` are independent `lock()` calls; there is no
  combined snapshot accessor.
- Writer: `ContentStore::set` (content.rs:84-92), called from the stream reader
  thread on every producer line (content.rs:275). Markup and uniforms are
  written together atomically under the mutex (`*guard = content`,
  content.rs:88-90), so a single content generation is internally consistent;
  the tear can only happen *between* the draw callback's two acquisitions.
  Both `markup` and `uniforms` in a `TileContent` derive from the **same**
  payload (content.rs:242-247), so a mixed pair corresponds to no payload the
  producer ever emitted — a displayed-state fidelity violation, one frame
  wide, self-healing at the next dirty poll (≤150ms, lib.rs:368).
- Uniform consumer: `src/lib.rs:268` — store uniforms feed only the tile-level
  background shader (`sh.render(..., &shader_uniforms)`). Markup-borne
  `<bg>`/`<glow>` embeds build their own uniforms from tag attributes
  (lib.rs:1142-1181), not from the store.

Adjacent, same class, noted here rather than as a separate property: `set()`
stores the `animating` flag **before** taking the content lock
(content.rs:85-90), so a tick callback (lib.rs:352) can observe the *new*
animation decision while the draw still renders the *old* markup for one frame.
Same one-frame self-healing shape; the generation assertion below would not
catch it (it is flag-vs-content, not content-vs-content), but the fix (single
locked snapshot) removes both windows.

## Failure scenario

1. Draw callback runs (dirty poll or tick), reads `uniforms()` → generation N.
2. Stream thread's `set()` lands: content becomes generation N+1.
3. Draw reads `markup()` → generation N+1.
4. The frame composes a background shader driven by N's uniforms under text
   describing N+1 — e.g. for a tile whose shader uniform encodes state urgency,
   background says `working` while text says `prompt` for one frame (a
   "prompt" text row over an "idle"-intensity background).

The window is microseconds wide and requires a writer publish inside one draw —
essentially impossible to hit in manual testing, routine for Antithesis
scheduling exploration.

The Antithesis-interesting part is also the heal, not just the tear: if the
writer is paused *after* the content write but *before* the dirty store
(content.rs:90→91), the torn frame is the last frame rendered and no redraw is
pending — the torn composite persists on screen for as long as the writer is
descheduled (see `publish-visible-within-poll-bound` for the general form of
that window). Otherwise it self-heals at the next dirty poll.

## Suggested instrumentation (net-new, SUT-side, Rust plugin) — competing designs kept side by side

Shared substrate (both focuses converged): add a monotonically increasing
`generation: u64` to `TileContent`, incremented by the writer inside `set()`
(it lives under the mutex, so it travels atomically with markup and uniforms);
expose it from both accessors — `uniforms()` and `markup()` return
`(value, gen)` in instrumented builds — or better, add a combined
`snapshot() -> (String, Vec<(String,f32)>, u64)` and assert the old two-call
path is gone.

- **Design A (data-integrity): strict same-generation `Always`.** In the draw
  callback, after both reads:
  - Type: **Always** — every draw must be generation-consistent; a single
    mixed frame is a violation.
  - Message: `"draw snapshot: markup and uniforms read from the same TileContent generation"`

  This assertion is *expected to fail* at f87ec19 — it demonstrates a real (if
  low-severity) torn read whose fix is one combined lock acquisition. Catching
  it is also a good calibration signal that the harness's thread-timing
  exploration is working.

- **Design B (concurrency): reachability `Sometimes` + generation-monotonicity
  `Always` pair.** In the draw callback:
  1. `Sometimes(ugen != mgen, "draw observed uniforms and markup from different content generations")`
     — proves Antithesis can force the interleaving; replay anchor for the
     compound scenarios (persistence via writer pause).
  2. `Always(mgen >= ugen, "markup generation never lags uniforms generation within a single draw")`
     — the single-writer monotonicity direction. A violation means a second
     writer, generation misuse, or memory-ordering breakage.
  3. `Always(mgen >= last_rendered_mgen, "rendered content generation is monotonic across draws")`
     — cross-frame monotonicity; guards the same topology assumption.

- **Difference to resolve at evaluation:** Design A treats the tear itself as
  a violation (an expected-fail bug-finder that becomes a regression guard
  once a `snapshot()` fix lands); Design B treats the tear as
  reachable-by-design (`Sometimes`) and asserts only the monotonicity
  invariants around it. If the team fixes the tear, Design B's assertion 1
  flips to `Unreachable("draw observed a torn uniforms/markup pair")` and the
  property becomes a regression guard. The assertion type is a discrete
  commitment — pick one framing.

## Antithesis angle

- Pure thread-interleaving exploration: high-frequency producer churn (workload
  drives rapid payload changes) multiplies `set()` calls; the scheduler only
  needs to land one between the two lock acquisitions of a draw.
- No process faults needed — this is the one property in this set that tests
  Antithesis's scheduler exploration rather than its process/fs faults.

## Key observations

- **Severity is configuration-scoped.** The shipped claude preset
  (`tiles/claude/tile.json`, verified at f87ec19) declares neither
  `shader_uniforms` nor `background_shader`; `uniforms()` returns an empty vec
  every generation, so the tear is pixel-invisible for the flagship
  deployment. It is user-visible only for a tile configured with a data-driven
  background shader (the pattern the `u_load`/`u_hot` unit test at
  content.rs:365-375 documents as intended usage).
- The two reads are same-lock re-acquisitions, not two different locks — there
  is no lock-ordering/deadlock angle, only an atomicity gap.
- Single-writer topology (verified: only content.rs:210 and content.rs:275
  call `set()`, one thread per store) means store generations are strictly
  monotonic; the draw can only ever observe markup *newer* than uniforms,
  never older. That asymmetry is itself a checkable invariant that would catch
  a future refactor introducing a second writer or a lock-free fast path
  (Design B assertions 2-3 above).
- Mutex poisoning is swallowed (`unwrap_or_default`, content.rs:100-115): a
  poisoned lock returns empty markup/uniforms silently. Practically
  unreachable (no panic site holds this lock), and the sut-analysis already
  marks it as an ideal `Unreachable` instrumentation point — left to the
  concurrency-focus agent's mutex property; noted because the same accessors
  are involved.
- The dirty-flag protocol itself was examined and is **sound**: `set()` writes
  content before storing `dirty=true` (Release), the 150ms poll `take_dirty()`
  swaps (AcqRel) — no lost updates, no stale-render-after-dirty. No property
  needed there.

## Deployment shape (resolved 2026-07-22)

No shipped preset, documented pattern, or live deployment combines
`shader_uniforms` with `stream: true`:

- Bundled presets: `tiles/claude/tile.json` and `tiles/empty/tile.json` carry
  neither `shader_uniforms` nor `background_shader` (nor `exec`/`stream` —
  those live in the waybar module config).
- Docs: the only `shader_uniforms` example (README.md:247-254,
  `examples/shaders/reactive.glsl`) uses a poll-mode `exec` (no `stream`);
  the only `stream: true` doc (`tiles/claude/README.md:29`) has no shader.
- Live config (`~/.config/waybar/config.jsonc`, inspected 2026-07-22): all 20
  `cffi/pwetty#N` blocks are `tile: "claude"` + `stream: true` with no
  `shader_uniforms`/`background_shader` — `uniforms()` is an empty vec every
  generation, the tear is pixel-invisible in production.

Consequence: the user-visible severity of this property is **hygiene** for
every real deployment shape that exists today; the workload **must add a
synthetic tile** combining `stream: true` + `shader_uniforms` (the
reactive.glsl pattern) for the tear to be pixel-meaningful and for the
`Sometimes`/`Always` pair to be non-vacuous. Rank below the other concurrency
properties in workload priority accordingly.

## Open questions

- Should the tear be *fixed* (snapshot `(markup, uniforms)` under one lock — a
  ~5-line change adding a `snapshot()` method) rather than instrumented? See
  the Design A vs Design B difference above; if fixed, the reachability
  assertion flips to `Unreachable` and the persistence bound is covered by
  `publish-visible-within-poll-bound`. Matters because the assertion type is a
  discrete commitment.

### Investigation Log

#### Does any shipped preset/doc/deployment combine `shader_uniforms` with `stream: true`?

2026-07-22.

- Examined: `tiles/claude/tile.json`, `tiles/empty/tile.json`,
  `tiles/claude/README.md`, `README.md` (shader_uniforms section ~240-260),
  `examples/waybar-config.jsonc`, `examples/shaders/reactive.glsl`, and the
  live `~/.config/waybar/config.jsonc` (read-only).
- Found: presets carry no shader keys at all; README's shader_uniforms
  example is poll-mode; the stream:true doc pattern has no shader; the live
  config's 20 pwetty modules are all `tile: "claude"` + `stream: true`,
  no shader keys anywhere.
- Not found: any combination of the two keys in any shipped or documented or
  deployed config.
- Conclusion: resolved — none exists; workload must add a synthetic
  stream+shader_uniforms tile (noted in body, sharpened from the prior
  `(needs human input)` bullet, which is dropped).
