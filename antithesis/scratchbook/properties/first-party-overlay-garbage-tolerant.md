# first-party-overlay-garbage-tolerant

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in either repo (see `antithesis/scratchbook/existing-assertions.md`).
Backend repo: `/home/chussenot/agentic-db`.

## Why this seam

The first-party overlay reads Claude Code's undocumented
`~/.claude/sessions/<pid>.json` files. The package doc says it plainly
(internal/clauded/clauded.go:15-18): "The format is UNDOCUMENTED and may
change between Claude versions… Every function here is therefore tolerant."
That tolerance claim is exactly a claimed guarantee to test, and the area
has real bug history: the stale-busy inversion **e60a874** (validated below)
plus a38f714 (blanket waiting→"?" false positives) and 7fac1c6
(crash-zombie files). The overlay is re-read at two independent call sites:
every DB poll (**13ms cadence** — sendSnapshot, internal/daemon/daemon.go:
300-307) and every 1s GC tick (daemon.go:357-365).

With `-sessions-dir` pointed at a workload-controlled directory, the entire
format surface is drivable without Claude Code.

## Tolerance mechanics (traced)

- `Read` (clauded.go:113-136): missing dir → empty map, no error; per-file
  `readFile` failures are skipped; duplicate SessionIDs keep the fresher
  `StatusUpdatedAt`.
- `readFile` (clauded.go:179-204): read error / unmarshal error / missing
  sessionId → skipped. **No size cap**: `os.ReadFile` slurps whatever is
  there — a multi-GB `.json` is read whole, at up to ~77×/s from the
  overlay call site. This is the one unbounded resource on the seam.
- `ReadLive` (clauded.go:164-175): filters dead-pid files (crash zombies).
- `firstPartyState` (internal/daemon/reconcile.go:43-64): maps only known
  values; unknown statuses and non-permission `waiting` defer to hook
  state → **the overlay can only ever write enum states** (Working, Idle,
  Shell, Prompt) — non-enum leakage via this path is structurally
  impossible at HEAD, so the state-alphabet assertion is a skew guard, not
  an expected failure (complements `unknown-session-state-renders-blank`,
  which injects at the DB/cache layer).
- Stale-busy gate (reconcile.go:46-51): `busy` with
  `StatusUpdatedAt < hookLastSeen` defers to the newer hook state; missing
  timestamp keeps legacy busy→working.
- Error handling at the call sites: overlay read error → logf + keep hook
  state (daemon.go:303-306); gc read error or empty set → fpAvailable=false
  → **reap nothing** (gc.go:94-100). Both fail-safe on paper.

## Historical bug validated (per validating-claims)

**e60a874 "fix(daemon): first-party overlay correctness —
permission-prompt waiting + stale-busy gate"** — mechanism confirmed from
the diff and its tests: (1) a first-party file frozen at `busy` (observed
with --resume + long sessions) masked a finished turn as "working" — the
fix gates `busy` on freshness vs the session's last hook time
(`TestOverlayStaleBusyDefersToHookIdle`); (2) a real permission prompt
was rendered "working" because the one-shot Notification hook got
clobbered by a later PostToolUse — the fix maps `waiting` to Prompt only
when `waitingFor` names a permission prompt (allow-list,
`IsUserPrompt`, clauded.go:78-81), and deliberately does NOT freshness-gate
`waiting`/`idle`. The asymmetry is load-bearing (reconcile.go:40-42:
"Do not 'simplify' the gate onto the other states") — prime refactor-
regression territory.

## Failure scenarios

- A format drift (field renamed, status enum extended) that makes files
  unparseable degrades to hook-only state — fine — but a drift that keeps
  files *parseable with wrong semantics* (e.g. statusUpdatedAt changes
  units to seconds) silently disables the stale-busy gate: every busy
  looks ancient (or eternally fresh) and the inversion returns.
- Partial-garbage dirs interact with GC: one valid alien file keeps
  `fpAvailable` true while the real session's file is corrupt → the live
  session counts as first-party-absent → reaped at 3 ticks (this leg is
  owned by `live-prompt-session-never-reaped`; cross-link).
- A huge session file turns the 13ms overlay read into an allocation storm
  — daemon OOM kills the sole state reconciler; the bar freezes stale
  everywhere (S2 via backend death).

## Suggested assertions (net-new)

Workload-side (with `-sessions-dir` at a workload dir; fixture per the
catalog: real window, terminal_pid NULL):

1. `Always("daemon survives adversarial first-party session files")` —
   daemon pid stable (or supervisor restart count unchanged) across a
   garbage barrage: truncated JSON, binary junk, wrong-typed fields, dirs
   named `x.json`, symlink loops, 0-byte files, duplicate sessionIds.
2. `Always("daemon RSS stays below ceiling while an oversized session file exists")`
   — the no-size-cap leg; ceiling calibrated like the plugin RSS
   properties.
3. `Always("tile session states remain in the schema enum under first-party garbage")`
   — read tiles.json after each barrage round (≥2 ticks settle);
   states ∈ {working, prompt, idle, shell}.
4. `Always("a stale busy file never masks a fresher hook idle")` — the
   e60a874 regression: write fp file `status=busy, statusUpdatedAt=T`;
   deliver hook Stop at T+Δ; within 2 ticks the tile shows idle (not
   working). Event-counted, no clock fault (both timestamps are
   workload-authored).
5. `Sometimes("a first-party permission-prompt promoted a session to prompt")`
   — coverage of the allow-list leg: file with
   `status=waiting, waitingFor="permission prompt"` over a hook-working row
   yields a prompt tile.

SUT-side (Go):

6. `Sometimes("first-party file skipped as unparseable")` — readFile's
   ok=false arm (clauded.go:180-190). Confirms garbage actually reached the
   parser rather than being fixture-eaten upstream.
7. `Sometimes("stale-busy gate deferred to the newer hook state")` —
   reconcile.go:48-50. The gate's live-fire witness; if this never fires
   the regression assertion 4 is running vacuously.

## Antithesis angle

The workload owns the directory, so fault diversity is file-shape diversity
plus IO faults (EIO via fs fault injection on the dir, permission flips,
rename-vs-read races at the 13ms cadence). The two independent read sites
(13ms overlay, 1s gc) race each other over the same mutating files —
mid-rewrite reads are guaranteed, which is precisely the tolerance being
claimed.

## Open questions

- Should `readFile` cap file size (it's the only unbounded read on the
  daemon's hot loop)? If "yes", assertion 2 becomes a regression guard; if
  "accepted — local dir, trusted producer", record the acceptance since the
  dir is now also the documented workload injection point.
  `(needs human input)`
- Are there parseable-but-semantically-drifted shapes worth pinning beyond
  statusUpdatedAt units (e.g. `waitingFor` prose changing so "permission"
  no longer substring-matches — which would silently kill the prompt
  overlay, the exact ezmm shape the fix addressed)? The allow-list's
  comment says broaden only against observed values; a canary assertion on
  "waiting file with nonempty waitingFor deferred to hook" frequency could
  make drift visible. `(partial: mechanism understood; whether drift
  detection belongs in this property or a doctor-side check is a design
  call)`

### Investigation Log

#### Was the stale-busy inversion a real system defect, and what exactly fixed it?

- Examined: `git show e60a874` (clauded.go + reconcile.go/overlay_test.go
  diff), current reconcile.go:43-64, clauded.go:78-81.
- Found: pre-fix, `busy` mapped to Working unconditionally; the fix adds
  the freshness gate (StatusUpdatedAt < hookLastSeen → defer) with tests
  naming the observed trigger (file frozen at busy under --resume). The
  waiting→prompt allow-list was added in the same commit with the /btw
  false-positive documented.
- Not found: any freshness gate on waiting/idle — confirmed deliberate
  (comment + tests pin the asymmetry).
- Conclusion: both mechanisms confirmed from fix + tests; property legs 4/5
  encode them end-to-end (the unit tests stop at overlayFirstParty; the
  property drives file→daemon→cache→tile).
