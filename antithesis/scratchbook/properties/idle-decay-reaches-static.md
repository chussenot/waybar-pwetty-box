# idle-decay-reaches-static

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in either repo (see `antithesis/scratchbook/existing-assertions.md`).
Backend repo: `/home/chussenot/agentic-db`.

## Claim under test (L5) and validated thresholds

L5: "Idle decays through 7 levels over 60min; tile eventually goes static."
Backend-driven end to end. Thresholds validated at
internal/state/state.go:148-155 (`DecayThresholds` = 1, 4, 10, 20, 35,
60 min; `DecayLevel` at 160-167 is inclusive at the lower edge — exactly
60m is level 6; `DecayLevels = 7`, state.go:26). The tile path computes the
level in `sessionTile` (internal/tile/tile.go:120-131):
`IdleLevel = DecayLevel(now - last_talk_ts)` — recomputed on every cache
build (1s tick / 250ms throttle). Plugin side: `content_animates` returns
false for `level='6'` idle markup (src/content.rs:58-61), so level 6 stops
frame-clock redraws — "goes static".

**No clock fault needed**: `last_talk_ts` is a unix-ms column the workload
writes directly. Backdating it moves the session through the entire
60-minute design space in one tick. (GC caveat per the fixture context:
the window must stay live — `last_seen_ts` is NOT a reap trigger since
133ae8d, so old timestamps are safe.)

## Relationship to siblings

`static-idle-redraw-budget` and `idle-level-gate-clamp-divergence` own the
**out-of-range** payloads (idle_level 7/100/-1/6.0 injected at the cache
layer) and the plugin's clamp/gate agreement. This property owns the
**in-range, real-backend-arithmetic** progression: correct level per age,
monotonic advance, and the terminal static state — the honest reading of
L5. Its violation in the never-goes-static direction is the heat bug
arriving via backend data rather than injected garbage.

## The NULL-last_talk divergence (found during tracing)

`sessionTile` computes a level **only when** `LastTalkTS.Valid`
(tile.go:125). An idle session with NULL `last_talk_ts` keeps
`IdleLevel = 0`, which `omitempty` then drops from the JSON — the template
defaults it to 0 (`s.idle_level | default(0)`, tiles/claude/tile.json) →
rendered as **freshly-idle bright**, and `content_animates` says true
(recent-idle glow) → **animating at 30fps forever**. The workspace-name
path made the opposite call: `aggregate` maps no-talk-timestamp idle to
the **dimmest** level (`DecayLevels - 1`, internal/daemon/reconcile.go:
177-179). Same daemon, two answers. NULL-last_talk idle rows are reachable:
a Notification (or PostToolUse) arriving for a session the DB doesn't know
creates the row with the construction-default idle state and no BumpTalk
(internal/hook/hook.go:140-153, 185-201) — the same row shape the
`cache-error-demotes-live-tile` evidence flagged for its unbounded
repair-suppression window. So the never-goes-static leg has a concrete,
reachable trigger at HEAD, not just a hypothetical.

## Suggested assertions (net-new)

Workload-side (fixture: idle session, real window, workload-authored
`last_talk_ts`; check ≥2 daemon ticks after each write, away from bucket
boundaries — e.g. ages 30s, 2m, 6m, 14m, 27m, 45m, 90m):

1. `Always("emitted idle_level matches the decay bucket of the backdated last_talk age")`
   — read tiles.json (or the tile-watch line); compare to
   DecayLevel(age) computed by the workload. Boundary-safe by fixture
   choice, event-counted by tick.
2. `Always("idle_level never decreases while last_talk_ts is unchanged")`
   — per desktop key, across successive cache reads between backdate
   writes: elapsed only grows, so the level is monotone non-decreasing.
   Catches arithmetic/unit regressions (ms vs s is one typo away).
3. `Sometimes("an idle tile reached level 6 and went static")` — the L5
   landing witness: payload shows idle_level 6 AND the plugin's draw
   counter for that tile stops advancing between content changes (reuses
   the per-tile draw-counter infrastructure from
   `static-idle-redraw-budget`; that property owns the redraw-budget
   Always — this one only needs the arrival witness).
4. `Always("an idle session with NULL last_talk is not rendered as freshly idle")`
   — **expected to fail at HEAD** (renders level 0, animating). Fixture:
   insert the Notification-shaped row (state idle, last_talk_ts NULL)
   directly. The assertion encodes the reconcile.go precedent (dimmest);
   if the owner rules the intended level is something else, the check
   retargets — the invariant "not level 0 / not animating forever" is the
   floor either way.

SUT-side (Go):

5. `Sometimes("tile payload carried an idle session with NULL last_talk")`
   — in `sessionTile`'s idle branch when `!LastTalkTS.Valid`. Coverage
   witness for assertion 4's fixture actually flowing through, and a
   production-side canary (it fires on real Notification-created rows
   too).

Deliberately NOT suggested: a Go `Always` re-asserting
`IdleLevel == DecayLevel(elapsed)` inside sessionTile — it would re-call
the same function on the same inputs (tautology). The workload-side check
computes the expectation independently.

## Failure scenario

Never-goes-static direction: a NULL-last_talk idle row (or a future unit
regression in the elapsed math) renders bright, glowing, 30fps — per tile,
across 10 tiles the space-heater regression returns with all-healthy
processes. Wrong-level direction: an hour-idle session shows `██ 0m` —
plausible-looking, silently wrong recency information (S2-lite), and the
user's glance-ranking of desktops ("which Claude did I abandon longest
ago?") inverts.

## Antithesis angle

Faults add interleaving between the backdate write, the 13ms DB poll, the
1s tick, the 250ms build throttle, and tile-watch's 75ms poll — the level
must converge to the new bucket within ticks regardless of which snapshot
the write lands in. Kill/restart of the daemon mid-progression exercises
re-derivation from the DB (levels must not regress after restart — same
oracle as assertion 2 keyed to unchanged last_talk_ts).

## Open questions

- Intended rendering for NULL-last_talk idle: dimmest (match aggregate),
  level 0, or omit the session? The two backend paths disagree today;
  assertion 4 needs the ruling only for its target value, not its
  existence. `(needs human input)`
- `idle_ago` accompanies the level (`fmtAgo`, tile.go:590-599) — worth
  folding a coarse check into assertion 1 ("6m age renders '6m'"), or is
  string formatting below the property bar? Cheap to add while the
  fixture exists.

### Investigation Log

#### Are the decay thresholds and the static-at-6 chain real as claimed?

- Examined: state.go:139-167 (thresholds + DecayLevel), tile.go:120-131
  (sessionTile), reconcile.go:170-183 (aggregate's idle branch),
  content.rs:53-69 (content_animates level-6 gate),
  tiles/claude/tile.json (idle_level default(0)), 133ae8d's
  TestDecayLevelTimeline (pins the bucket math).
- Found: thresholds as documented; inclusive lower edges; sessionTile
  computes per-build from last_talk_ts; level 6 excluded from animation by
  the quote-agnostic gate. NULL-last_talk: sessionTile leaves 0 (omitted
  by omitempty) while aggregate uses DecayLevels-1 — divergence confirmed
  by inspection at HEAD.
- Not found: any code path writing idle_level > 6 from the backend
  (DecayLevel clamps to DecayLevels-1) — consistent with the catalog's
  existing note that out-of-range is skew/injection territory.
