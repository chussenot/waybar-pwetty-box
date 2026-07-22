# cache-error-demotes-live-tile — a transient cache read error must not replace a live published state with the idle placeholder

Found independently by 2 discovery focuses (data-integrity + coordination) — merged during synthesis; independent rediscovery is a confidence signal.

The writer/reader protocol on `tiles.json` has no "unknown" state: a failed
read is translated into a *fabricated* state change at the reader. Distinct
from `daemon-restart-no-placeholder-clobber` (that property is about the
daemon *writing* `{}` over a good cache during startup, i.e. missing-key after
a successful read). This property is about the **read-error branch** of the
watcher itself: cache file missing, unreadable, or unparseable (e.g. torn by
the dual-writer race in `tile-cache-never-torn`).

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Claim under test

sut-analysis §7: "Transient cache-miss during daemon restart actively *pushes*
placeholder over good state" and "Cache read error → `emptyPayload` (idle
level-6) — a live `prompt` visually replaced by a plausible 'long idle'."
Spot-checked in code by both focuses — confirmed, and the window is worse than
"transient": the daemon's in-memory dedupe can make it unbounded.

## Code paths (all in /home/chussenot/agentic-db unless noted)

- `internal/tile/tile.go:499-538` — `RunWatch` (`tile-watch`, the streaming
  producer). The `emit` closure (tile.go:514-531): starts from
  `p := emptyPayload(idx, false)`; only on `ReadCache` **success** (and key
  present) is `p` replaced by the cached payload. Any read error (file
  missing, unparseable, transient EIO) falls through to the placeholder; the
  string-dedupe (`s != last`, tile.go:526) then treats fabricated-empty /
  placeholder-vs-live as a genuine *change* and **publishes it**, replacing
  whatever real state was last emitted.
- `internal/tile/tile.go:498` — doc comment: "on any read error it emits the
  empty placeholder" — the behavior is **documented as intended**, at least
  for the daemon-never-ran case.
- Contrast: the one-shot poll path `Run` / `tile-data` (tile.go:471-481)
  degrades to a live niri+DB query (`BuildLive`) on cache read error — only
  the streaming path invents placeholder state; the streaming path got the
  strictly worse fallback.
- `internal/daemon/daemon.go:397-419` — the repair side. `writeTiles` dedupes
  against the **in-memory** `d.lastTiles` (bytes.Equal, daemon.go:411-413),
  not against the file. If `tiles.json` is deleted or corrupted externally
  while the marshaled payload bytes are unchanged, the daemon **never rewrites
  it** until the payload actually changes.
- `internal/tile/tile.go:120-131` — `sessionTile`: `IdleAgo`/`IdleLevel` are
  set only for `idle` sessions **with a valid `last_talk_ts`**. A `prompt`
  session's payload carries **no time-varying field**, so its marshaled bytes
  stay stable indefinitely → the dedupe holds → the masking window for exactly
  the attention-critical state is unbounded. Same for `working`/`shell`, and
  for an idle session whose `last_talk_ts` is NULL (reachable: a row created
  by a non-qualifying Notification stays at the construction-default `idle`
  with no `BumpTalk`, or a `working` row overlaid to idle by first-party
  status — neither ever set `last_talk_ts`).
- `internal/tile/tile.go:590-599` — `fmtAgo`: `m := int(d.Minutes()); if
  m < 60 { "%dm" } else { "%dh" m/60 }`. So an idle session's `idle_ago`
  string changes **once per minute below 60m and once per hour at/after
  60m**; `idle_level` changes only at the six decay thresholds
  (1/4/10/20/35/60m, `state.go:148-167`) and freezes at level 6 past 60m. The
  daemon rebuilds every 1s tick (throttled to 250ms), so the byte-change
  cadence — and therefore the max repair-suppression window of the
  `d.lastTiles` dedupe for all-idle content — is **≤ ~60s while any idle
  session is under an hour old, up to ~1h once all are past an hour, and
  unbounded when no session carries `idle_ago`** (idle with NULL
  `last_talk_ts`, empty-desktop placeholders — `emptyPayload` has level 6 and
  no `idle_ago`).
- `internal/tile/tile.go:93-99` — `emptyPayload` is not visibly an error: one
  idle session at max decay (`idle_level: DecayLevels-1`) — renders as a calm
  long-idle desktop. A live `prompt` (pulsing) tile downgrades to still,
  plausible normality.
- Plugin side: the placeholder line is a perfectly valid payload, rendered
  normally (waybar-pwetty-box `src/content.rs:275`). The attention pulse stops
  (`content_animates` goes false on idle level-6 — waybar-pwetty-box
  `src/content.rs:53-69`). No layer downstream can tell "demoted by read
  error" from "genuinely idle" — the failure is invisible end to end.
- Recovery: once a read succeeds again, `s != last` re-emits the real payload
  → wrong-state window is one poll (75ms) if the failure was transient, or
  **forever** if it persists (cache deleted while daemon dead — no restart
  unit ships, sut-analysis §7).

## Failure scenario

A session sits in `prompt` (the one state the product exists to surface):

1. Session enters `prompt`; daemon writes tiles.json; tile-watch publishes it;
   tile pulses (the product's core alert).
2. tiles.json becomes momentarily unreadable — one 75ms tick's `ReadCache`
   fails: daemon restart mid-write, the two-writer torn publish
   (`tile-cache-never-torn`), external `rm` (e.g.
   `rm -rf ~/.local/share/...` while the daemon is down), tmpfs cleanup, or a
   transient FS fault.
3. tile-watch emits `emptyPayload`: the plugin repaints the tile as a calm dim
   idle placeholder; the attention pulse stops.
4. Repair depends on the daemon *re-writing* the file. If the failure was a
   one-tick blip, the tile flashes idle→prompt within ~300ms (75ms poll +
   150ms dirty poll) and the mask is a flap. Because of the in-memory dedupe
   and the byte-stable prompt payload, if the daemon did not itself restart
   (external deletion / torn publish by a second instance), the file is never
   rewritten — the user's waiting prompt is masked **indefinitely** with zero
   diagnostics, until some unrelated state change (desktop switch flips
   `Active`, session state change). If the daemon did restart, repair lands
   within ~1s.

The coordination defect: the protocol conflates "I could not read the shared
state" with "the shared state says empty". Keep-last (the discipline the
plugin itself uses on producer EOF, waybar-pwetty-box `src/content.rs:250-255`)
is the obvious alternative for the had-prior-state case.

Severity: S2 in the sut-analysis product table — the failure mode the product
exists to prevent, rendered as plausible normality.

## Suggested assertions (net-new, Go SUT side + workload) — the two focuses propose DIFFERENT assertion types for the same tile.go:515-531 branch; both kept

- **Design A (data-integrity): `Unreachable`.** In `RunWatch.emit`, before
  publishing: if `rerr != nil` (cache read failed) and `last` is a non-empty,
  non-placeholder payload, the demote path has been entered.
  - Type: **Unreachable** — this is a critical corruption path that must never
    be observed; Antithesis hunts for it under fault injection.
  - Message: `"tile-watch: cache read error demoted a live published payload to the empty placeholder"`
- **Design B (coordination): conditioned `Always`.** SUT `Always` in
  `emit()`'s error branch: message **"cache read failure never replaces
  previously read tile state"** — condition: read failed ⇒ (`last == ""` —
  nothing was ever emitted, startup placeholder is legitimate — OR the emit is
  skipped/unchanged). **Expected to fail at current code** whenever a read
  failure follows good state; the property documents the fabrication and
  becomes the regression guard after a keep-last fix.
- **Difference to resolve at evaluation:** Design A frames the branch as a
  must-never-happen violation (a finding on first hit, no startup carve-out
  needed because it conditions on `last` being non-empty non-placeholder);
  Design B frames it as an invariant with an explicit startup-placeholder
  carve-out, expected to fail at f87ec19 and to flip to a regression guard
  after the fix. Same code branch, different assertion type and framing.

Companions (both cheap, worth having):

- **Sometimes** on the `rerr == nil && ok` branch after a previous
  `rerr != nil` poll — `"tile-watch: cache recovered after a failed read"` —
  confirms runs actually exercise the error/recovery cycle rather than never
  seeing a failed read.
- SUT or workload **Sometimes**: message **"cache read failed while watcher
  held non-empty state"** — non-vacuity guard; the Always/Unreachable above is
  only exercised when this condition actually occurs (workload must
  delete/corrupt the cache or drive the dual-writer race).

## Antithesis angle / fault requirements

Filesystem faults on the cache path, workload-driven cache
deletion/corruption, or the dual-daemon race from `tile-cache-never-torn`. All
process/file level; no node termination.

- Kill/restart the daemon at random points around its 250ms-throttled writes
  (daemon.go:379-389) while tile-watch polls at 75ms — races the read against
  create/rename windows.
- Delete tiles.json from the workload while a session sits in `prompt` and no
  other state changes: with the dedupe defect, the placeholder mask persists
  and the Unreachable fires on the first 75ms poll.
- Combine with `tile-cache-never-torn`: torn bytes → parse error → same demote
  path, without any file deletion.

## Key observations

- The likely fix shapes differ by side, which is why the daemon-repair defect
  is recorded here rather than as its own property: (a) tile-watch keeps
  `last` on read error instead of fabricating placeholder (one/two-line fix,
  kills the whole class); (b) daemon verifies file existence before
  dedupe-skipping (self-repair). Assertion Design A's Unreachable covers both
  — if either fix lands, the path stays unreachable.
- The dedupe makes the fabrication durable in the stream: after emitting
  empty, `last` is the empty JSON, so the watcher will re-emit real state as
  soon as a read succeeds — good — but every failure/success alternation
  produces a visible idle/prompt flap and restarts the pulse animation.
- The plugin cannot compensate: it applies each line as truth (S6 only guards
  non-JSON; a *valid* fabricated payload is indistinguishable from data). The
  fix has to be at the watcher.
- `emptyPayload` is indistinguishable-by-design from a real empty desktop
  (tile.go:89-99), so a workload-side check needs producer-side ground truth
  (what the daemon last built), not pixel/payload inspection alone.

## Open questions

- Is degrade-to-placeholder on read error *intended* ("no cache = show
  nothing") rather than a defect — i.e. is empty-on-read-error deliberate
  beyond the daemon-never-ran case, downgrading previously-good state on a
  transient error? The poll path's contrasting `BuildLive` fallback suggests
  the streaming behavior is an oversight, not policy. If intended/accepted,
  the property should be rewritten to allow the demotion but bound its
  duration (repair liveness) — e.g. scope the Always to persistent failures
  only (k consecutive failed reads) or drop it in favor of the Sometimes
  detector; if a defect, the Unreachable / Always stands as written (keep-last
  is a two-line fix). `(needs human input)` The sut-analysis file-level open
  question "daemon death ⇒ permanent silent staleness accepted by design?" is
  the same judgment call one level up.

### Investigation Log

#### Is empty-on-read-error deliberate?

- Examined: `RunWatch` doc comment (tile.go:493-498), `emit()` implementation
  (tile.go:514-531), `tile-data`'s contrasting `BuildLive` fallback
  (tile.go:478-481), sut-analysis §7 ("transient cache-miss during daemon
  restart actively pushes placeholder over good state") and its file-level
  open questions.
- Found: the doc comment states the behavior, so it is intended at least for
  "no cache yet"; the one-shot sibling command uses a better fallback,
  suggesting the streaming path's behavior was not weighed against the
  had-prior-state case.
- Not found: any comment, bead, or commit distinguishing transient from
  persistent read failure, or acknowledging the prompt-masking consequence.
- Conclusion: tagged `(needs human input)` — intent for the
  previously-had-state case is genuinely undocumented.

#### How long can an all-idle payload stay byte-identical (dedupe repair-suppression window)?

Investigated 2026-07-22.

- Examined: `fmtAgo` (tile.go:590-599), `sessionTile` (tile.go:120-131),
  `emptyPayload` (tile.go:93-99), `state.DecayThresholds`/`DecayLevel`
  (state.go:148-167), the daemon tick + `maybeWriteTiles` throttle + byte
  dedupe (daemon.go:236-242, 379-419), hook.go's `BumpTalk` handling
  (hook.go:152-154, 185-204) for whether idle-with-NULL-`last_talk_ts` is
  reachable.
- Found: `fmtAgo` is minute-granular under 60m (`"%dm"`) and hour-granular
  at/after (`"%dh", m/60`); `idle_level` freezes at 6 past 60m. The daemon
  rebuilds every 1s tick with fresh `db.Now()`, so idle payload bytes change
  once/minute (<1h old) and once/hour (≥1h old). Both fields are gated on
  `s.LastTalkTS.Valid` — an idle session with NULL `last_talk_ts` (e.g. a row
  created by a non-qualifying Notification, which sets the construction
  default `idle` without `BumpTalk`; or a `working` row overlaid to idle by
  first-party status) emits neither field and is byte-stable forever, exactly
  like `prompt`. `emptyPayload` (level 6, no `idle_ago`) is also static.
- Not found: any daemon-side check of the cache file's existence before the
  dedupe skip (the repair gap is real, confirmed at daemon.go:411-413).
- Conclusion: RESOLVED — the repair-suppression window for all-idle content is
  bounded at ~60s while any idle session is under an hour old, grows to up to
  ~1h once every idle session is past an hour, and is unbounded for
  idle-with-NULL-talk / empty-desktop-only caches (in addition to the already
  confirmed unbounded prompt/working/shell cases). The workload's
  cache-deletion fault therefore masks state for a bounded-but-long window
  even in the "benign" all-idle case; body updated with the exact granularity.
