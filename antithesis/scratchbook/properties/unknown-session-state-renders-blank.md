# unknown-session-state-renders-blank

Focus: protocol contracts — the `sessions[].state` enum
(`working|prompt|idle|shell`, closed enum in `tiles/claude/schema.json:54-56`)
is enforced at NO layer of the four-hop chain (DB → daemon → tile-watch →
template → renderer), and the renderer's behavior for a value outside the enum
is to draw **nothing at all**: a live session silently loses its status
indicator, its animation, and its cap priority.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

The pass-through chain (no validation at any hop):

1. `/home/chussenot/agentic-db/internal/db/db.go:429-444` — `LoadLive` scans
   `sessions.state` verbatim. `state.go:29-31` says the column is "validated on
   read", and `state.Status.Valid()` exists (`state.go:51-58`) — but this read
   path never calls it, and the schema has no CHECK constraint (`db.go:35`:
   `state TEXT NOT NULL,`). Investigated (see Investigation Log): CURRENT code
   cannot write a non-enum value — every writer of `Session.State` (hook +
   reconcile overlay) emits only the four enum constants, and the wider
   `clauded` alphabet (`busy`, `waiting` — `clauded.go:40-50`) is mapped down
   safely by `firstPartyState` (unrecognized/deferred values return
   `("", false)` and are never written, `reconcile.go:61-63`). The non-enum
   vector is therefore version skew (a hook binary with a different alphabet
   writing the shared DB), external SQL writes, or corruption. The
   sut-analysis bug history (stale-`busy` inversion, waiting→"?" false
   positives) is motivation only — the mechanism below is independently
   code-confirmed.
2. `/home/chussenot/agentic-db/internal/tile/tile.go:120-121` — `sessionTile`
   copies `s.State` verbatim into the payload.
3. `tile.go:104-117` — `statePriority` gives unknown states priority 4 (LAST):
   under the 2-session cap an unknown-state session is dropped before an idle
   one — whatever its true urgency was.
4. `tiles/claude/tile.json:7` — template interpolates verbatim:
   `<status state='{{ s.state }}' .../>`; the pulse check compares literally
   against `'prompt'`, so a dirty prompt-like state never pulses.
5. `src/lib.rs:1023-1073` — `draw_status`: known arms for
   `working/shell/prompt/idle/empty`, then `_ => {}` (lib.rs:1072) — **no
   indicator drawn**. Note the asymmetry: a MISSING `state` attribute defaults
   to `"idle"` (`attr(attrs, "state").unwrap_or("idle")`, lib.rs:1023 — renders
   a bright idle bar), while an unknown value renders nothing.
6. `src/content.rs:53-69` — `content_animates` matches only the known state
   strings → an unknown-state session never animates; combined with (5) the
   session shows folder/title with a blank gap where the indicator belongs.

Contract drift in the other direction: the renderer supports `state='empty'`
(lib.rs:1071) which the schema enum does not list and the producer never emits
(`emptyPayload` uses `idle`, tile.go:93-99) — the plugin's de-facto alphabet is
a superset of the schema's.

## Failure scenario

Any layer produces a state outside the enum — hook/daemon version skew writing
an internal status, a schema evolution adding a state (e.g. `waiting`) that
ships in the backend before the plugin updates, or a corrupted DB row:

- The session renders with NO status indicator: to the user this reads as a
  layout quirk, not as "state unknown". If the true state was prompt-like, the
  alert never fires (S2) — and the cap may have silently dropped the session
  entirely on a busy desktop (ties to `prompt-priority-survives-session-cap`).
- Nothing is logged anywhere along the chain. The failure is invisible in every
  observable except the missing pixels.

## Suggested assertions (net-new)

- SUT-side Go `Always` in `sessionTile` (`tile.go:120`): `state.Status(s.State).Valid()`;
  message **"session state entering the tile payload is a schema enum value"**.
  Catches DB pollution at the earliest hop, where the session ID and raw value
  are still available for the details payload.
- SUT-side Rust `Always` at the top of `draw_status` (`src/lib.rs:1023`),
  checking the resolved state is one of the renderer's known arms (incl.
  `empty`): message **"status tag reached the renderer with a known state"**.
  Catches template-level and non-tile-watch producers the Go assertion can't
  see; hot path but a trivial string compare.
- Workload `Sometimes`: inject a payload with `state: "waiting"` through the
  stream and confirm the Rust assertion fires (i.e. the injection actually
  reaches the renderer); message **"unknown-state payload traversed the chain
  to the renderer"** — validates the test plumbing itself.

## Key observations

- "Renders unknown states as nothing" is the quiet twin of the emptyPayload
  masking: both convert an exceptional condition into plausible-looking
  normality. An `Unreachable`-style guard was considered instead of `Always`,
  but the `_ => {}` arm is a legitimate forward-compat catch-all today;
  `Always` at the two ends of the chain expresses "the alphabet must match"
  without forbidding the renderer's defensive arm.
- The missing-attribute → `"idle"` default (lib.rs:1023) is arguably a second,
  separate masking (absent state shown as bright idle); kept as an observation
  rather than a property because only a hand-written template can omit the
  attribute — the bundled template always emits it.

## Open questions

- Should the renderer's `empty` state be added to the schema enum, or is it a
  plugin-internal extension by design? `(needs human input — one-line contract
  decision; affects what alphabet both Always assertions check against.)`
- What is the desired rendering for an unknown state — blank (current), the
  `?` prompt treatment, or an explicit "unknown" glyph? `(needs human input;
  determines whether the Rust assertion's failure is "bug found" or "contract
  clarified".)`

### Investigation Log

#### Can the CURRENT hook/daemon code write a non-enum state, or is the vector only version skew / external writes?

Investigated 2026-07-22.

- Examined: every writer of `Session.State` in /home/chussenot/agentic-db
  (`grep -rn '\.State ='` over non-test Go: hook.go:146,199,203 and
  reconcile.go:85 are the only hits); `state.MapEvent` (state.go:203-222);
  `firstPartyState` + `overlayFirstParty` (reconcile.go:43-88);
  `db.Upsert`/`LoadLive`/schema (db.go:35, 382-444); all `database.Upsert`
  callers (hook.go only — resume/doctor/recap never write sessions).
- Found: the DB-write alphabet from current code is exactly
  `{idle, prompt, working}` — hook.go:146 `s.State = string(state.Idle)` (new
  row), hook.go:199 `s.State = string(state.Prompt)` (matched Notification),
  hook.go:203 `s.State = string(t.NewStatus)` where `MapEvent` only returns
  Idle/Working/Prompt with `ChangeStatus: true`. `shell` is never stored
  (state.go:47: "our hooks never set it") and reaches the payload only via the
  in-memory overlay: `firstPartyState` maps `clauded.Shell -> state.Shell`,
  and its non-enum arms all defer — `case clauded.Waiting: ... return "",
  false` and `default: return "", false` (reconcile.go:56-63) — with the write
  gated on ok (reconcile.go:84-85 `if st, ok := ...; ok {
  sessions[i].State = string(st) }`). So the full alphabet reaching
  `sessionTile`'s state field from current code is `{working, prompt, idle,
  shell}` on the daemon path (LoadLive + overlay), `{working, prompt, idle}`
  on the `BuildLive` fallback (no overlay), plus `emptyPayload`'s literal
  `idle` — all inside the schema enum.
- Not found: any CHECK constraint on `sessions.state` (db.go:35 is bare
  `TEXT NOT NULL`), any `Valid()` call on the read path, any other process
  writing the sessions table.
- Conclusion: RESOLVED — current code provably cannot emit a non-enum state;
  the vector is version skew (a differently-versioned hook/daemon binary
  sharing the DB), external SQL writes, or corruption. The Go `Always` in
  `sessionTile` is a skew/corruption guard; the workload must inject the dirty
  state via a direct DB write (`UPDATE sessions SET state='waiting'`), which
  Antithesis can do. Property invariant and assertions stand as written.
