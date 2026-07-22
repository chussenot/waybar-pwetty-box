# publish-visible-within-poll-bound

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists anywhere in this codebase at f87ec19 (see
`antithesis/scratchbook/existing-assertions.md`).

## Code paths

- Writer, the three-step non-atomic publish: `ContentStore::set`,
  `src/content.rs:84-92`:
  1. `animating.store(...)` (content.rs:85-87, Release)
  2. content written under the mutex (content.rs:88-90)
  3. `dirty.store(true)` (content.rs:91, Release)
- Consumer heartbeat: the 150ms dirty-poll timeout, `src/lib.rs:366-374` —
  `take_dirty()` (`swap(false, AcqRel)`, content.rs:118-120) → `queue_draw()`.
- Rescue path for animating content: the tick callback (`src/lib.rs:351-361`)
  queues draws regardless of the dirty flag while `animating()` is true, and
  the draw always reads the *latest* store content — so animating content gets
  rendered even if the dirty store is delayed.

## Failure scenario

The reader thread is paused (Antithesis thread-pause / CPU modulation — a
stand-in for real descheduling under load or cgroup throttling) between step 2
and step 3 of `set()`. Store state: readable content is generation N+1, dirty
flag is false, no redraw queued.

- If the new content is **animating** (e.g. idle → prompt), the tick callback
  rescues it: `animating` was stored *first* (step 1), so the gate is already
  true and per-frame draws render the new content. The animating-before-content
  ordering is load-bearing here — worth stating because it is undocumented.
- If the new content is **static** (e.g. working → idle level-6, or any
  update on a tile whose template lacks the `could_anim` literals so no tick
  callback exists at all, lib.rs:332-340), *nothing* redraws. The tile shows
  stale pixels for exactly as long as the writer is descheduled, with new
  content sitting readable in the store. This is the SUT's S2 severity class
  (silent staleness) manufactured purely by scheduling, with zero process or
  I/O failure.

The window is ~2 instructions wide, which is why it has never been observed;
Antithesis's scheduler exploration is precisely the tool that widens it.

## Key observations

- No update is ever permanently *lost*: once the writer resumes, step 3 runs,
  and the next poll (≤150ms) renders. The property is a bounded-visibility
  liveness claim, and the bound is conditional on writer-thread progress —
  which is the finding. The fix (if the team wants one) is to set dirty before
  releasing the mutex, or store dirty before content; either collapses the
  window.
- The reverse race (main thread's `take_dirty` consuming the flag while a
  set() is mid-flight) causes at most one redundant redraw, never a lost one —
  the draw reads latest content, and step 3 re-arms the flag. Verified by
  walking the interleavings; no assertion needed for that direction.
- L1 ("sub-150ms repaint", README) is the marketing form of this property; the
  SUT analysis already re-derives the honest bound as 2 poll periods + 1 frame
  (~300ms + frame). The assertion below uses that bound with hysteresis.

## Suggested instrumentation (net-new)

Reuses the `TileContent.generation` counter from
`torn-uniforms-markup-frame`, plus a main-thread cell `last_rendered_gen`
updated in the draw callback. In the 150ms dirty-poll callback (it exists for
every content tile and runs regardless of animation state — the right
heartbeat site):

1. `Always(store_gen == last_rendered_gen || redraw_pending || polls_behind < 2, "published content is rendered or redraw-pending within two dirty-poll periods")`
   — where `polls_behind` counts consecutive polls that observed
   `store_gen > last_rendered_gen` with `dirty == false` and no draw queued.
   The two-poll hysteresis absorbs in-flight draws and normal latency; only a
   stalled publish (dirty never stored) or a broken wakeup path trips it.
2. `Sometimes(store_gen > last_rendered_gen && !dirty, "content generation advanced ahead of the dirty flag")`
   — direct observation of the mid-set window; exploration anchor that tells
   Antithesis this interleaving is interesting.

Reading `store_gen` requires taking the content mutex in the poll callback
(6.7Hz, trivial cost) or mirroring the generation into an `AtomicU64` stored
as step 2.5 — the mirror must be written under the lock to stay coherent.

## Open questions

- Is a writer-pause-induced stale frame a *finding* the owner cares about, or
  accepted-by-design for a single-user tool? The same question the SUT
  analysis poses for daemon-death staleness (§7) applies one level down. The
  answer decides whether assertion 1 is a tripwire worth triage time or noise
  to be bounded more loosely. `(needs human input)`
- Exact bound tuning: is 2 poll periods right under a heavily-loaded main
  loop (10 instances share it)? A 3-poll bound trades sensitivity for triage
  signal-to-noise. Decide after a first run's false-positive rate is known.

### Investigation Log

#### Is a writer-pause-induced stale frame a finding the owner cares about, or accepted-by-design?

Investigated 2026-07-22.

- Examined: the three-step publish (`ContentStore::set`, src/content.rs:84-92),
  `take_dirty` (content.rs:118-120), the tick rescue path (src/lib.rs:351-361),
  the dirty-poll heartbeat (src/lib.rs:366-374), the README L1 claim
  ("sub-150ms repaint") and sut-analysis §7 (which poses the same
  accepted-by-design question for daemon-death staleness); AGENTS.md and beads
  for any stated staleness tolerance.
- Found: the mechanism as described in this file — a writer descheduled
  between steps 2 and 3 leaves static content readable but unrendered for the
  duration of the pause; no update is permanently lost; the README states only
  the marketing bound and sut-analysis re-derives the honest ~300ms+frame
  bound, neither addressing scheduling-induced overshoot.
- Not found: any statement of whether staleness beyond the poll bound under
  writer descheduling is accepted for a single-user tool — sut-analysis §7
  leaves the analogous question explicitly open.
- Conclusion: tagged `(needs human input)` — intent question for the owner.
  The answer decides whether assertion 1 is a tripwire worth triage time or
  should be bounded more loosely.
