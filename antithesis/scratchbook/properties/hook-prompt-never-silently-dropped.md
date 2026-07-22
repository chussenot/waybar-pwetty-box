# hook-prompt-never-silently-dropped

**All suggested assertions are net-new.** No Antithesis SDK instrumentation
exists in either repo (see `antithesis/scratchbook/existing-assertions.md`).
Backend repo: `/home/chussenot/agentic-db`.

## The by-design silent-failure ingress

`claude-status hook` is the sole real ingress for session state. Its package
doc (internal/hook/hook.go:1-9) declares the design posture: the hook "must
NEVER block or fail Claude. Run swallows every error (logging it to a
ring-buffered file next to the DB) and always returns nil, so the process
exits 0 in all cases." Claude Code sees exit 0 no matter what happened, and
never retries — a dropped event is dropped forever. The daemon then reads
only what landed in the DB, so a lost `Notification` (prompt) means the bar
never alerts (F9 severity, the product's core purpose).

The property states the design's own promise as a two-leg ledger: **every
hook invocation carrying a prompt transition is either recorded in the DB or
observably rejected (a line lands in the hook error log)** — never neither.

## Swallow-point inventory (hook.go, traced)

Errors that reach the caller are logged via `logError` (Run,
internal/hook/hook.go:69-72 → internal/hook/log.go:27-48):

- `io.ReadAll(stdin)` error — hook.go:80-82 → logged.
- `json.Unmarshal` error — hook.go:87-89 → logged. **Stricter than the
  comment claims**: the comment says "tolerant parse", but any unmarshal
  error returns — a malformed *sibling* field (e.g. `message` as an object,
  `session_id` as a number) on an otherwise-valid prompt event drops the
  whole transition. Logged, so the ledger holds, but this is the easiest
  workload lever for the "rejected" leg.
- `db.Open` error — hook.go:95-98 → logged.
- `database.Get` error — hook.go:115-118 → logged.
- `database.Upsert(s)` error (the state write itself) — hook.go:221 →
  logged. DSN sets `busy_timeout(2000)` + `_txlock=immediate`
  (internal/db/db.go:341-345), so a write-lock held >2s by another process
  turns the state write into a logged error — a lost transition with a
  healthy-looking exit 0.

Silent by design (no log, no record):

- Empty `session_id` — hook.go:90-93: return nil, nothing logged. Benign
  (nothing to key on), but it is a true zero-trace drop; the property's
  precondition excludes it (a prompt transition carries a session_id).
- Every `database.InsertEvent(evt)` audit write — hook.go:111, 129, 133,
  219: `_ =` discarded, **not logged**. State can be recorded while its
  audit row silently vanishes. The property's primary leg is the sessions
  row, so this only weakens the audit trail — worth a separate `Sometimes`
  to make audit loss visible.
- **`logError` itself is best-effort** — log.go:40-42: if the log file
  cannot be opened/written (disk full, read-only fs, dir missing), the
  rejection evidence is silently discarded. This is the hole that makes the
  property falsifiable at HEAD: disk pressure that fails *both* the DB write
  and the log write produces a prompt transition with **zero trace**.

## The prompt-transition path specifically

A Notification becomes Prompt only when `isPromptNotification(message)`
matches ("permission"/"approve"/"confirm", hook.go:37-41, 235-243) AND the
prior state is not idle (hook.go:185-201). Workload fixture: create the
session mid-work first (UserPromptSubmit → working), then deliver the
Notification. `UserPromptSubmit`/`PostToolUse`/`Stop` transitions are the
same ledger with easier preconditions; the prompt one is the alert-critical
instance.

## Failure scenario

A permission prompt fires while the workload holds the SQLite write lock
past the 2s busy timeout (or the state dir is at disk-full). The hook exits
0; Claude Code carries on; the DB row still says `working`; if the log write
also failed there is no evidence anywhere. The daemon renders a calm working
dot; the user never sees the prompt. Everything is "healthy".

## Suggested assertions (net-new)

Workload-side (primary — the hook is a short-lived process, so the check is
event-counted: run the hook synchronously, then inspect):

1. `Always("hook prompt transition was recorded in the DB or rejected in the hook log")`
   — after each workload-driven prompt-transition hook invocation: the
   sessions row shows `state='prompt'` (or the event audit row exists), OR
   `hook.log` grew by ≥1 line since the pre-invocation snapshot. **Expected
   to fail under disk-full** (the logError hole); whether that is accepted
   ("never block Claude" deliberately trades observability) is an owner
   question, not a reason to soften the assertion.
2. `Sometimes("a prompt-transition hook ran under DB lock contention")` —
   coverage guard: the workload actually held `BEGIN IMMEDIATE` (sqlite3
   CLI) across an invocation.

SUT-side (Go, in agentic-db):

3. `Reachable("hook logged a swallowed ingress error")` — in `logError`
   after a successful write (log.go:47). Confirms the rejected leg is
   exercised, and is the replay anchor for "which error class".
4. `Unreachable("hook error log write itself failed")` — in the two silent
   return arms of `logError` (log.go:40-42 openErr; optionally the Fprintf
   result). This is the zero-trace tripwire; firing = the ledger broke.
5. `Sometimes("hook audit event insert failed while state write succeeded")`
   — at the `_ = database.InsertEvent` sites, condition on err != nil.
   Makes the silent audit-loss class visible without changing behavior.

## Antithesis angle

Fault injection owns every lever: SQLite lock contention (a workload
transaction held across the 2s busy timeout), disk pressure on the state
dir (fails Upsert and then the log append), malformed sibling fields in the
hook JSON (workload pipes crafted stdin), kill-mid-invocation. The hook is
spawned per event, so timing diversity is free — every invocation is a
fresh interleaving against the daemon's 13ms poll and 1s tick writes
(`captureTitleChanges` InsertEvent takes the same write lock,
internal/daemon/daemon.go:426-433).

## Open questions

- Is zero-trace loss under disk-full accepted by design? The hook's charter
  is "never block or fail Claude"; making `logError` failure observable
  (e.g. one stderr line) would not violate that, but it is an intent call.
  If accepted, assertion 1 becomes `AlwaysOrUnreachable` conditioned on the
  log dir being writable, and assertion 4 is demoted to documentation.
  `(needs human input)`
- Should the strict `json.Unmarshal` be tolerant (decode what parses, act
  on session_id + event name)? Today a malformed sibling field drops a
  valid prompt transition — logged, so the ledger holds, but the alert is
  still lost. If tolerance is intended, a second property ("well-keyed
  events survive sibling-field garbage") becomes worth writing.
- Does `InsertEvent` audit loss matter enough to log? Decides whether
  assertion 5 stays a `Sometimes` or the sites get real logging.

### Investigation Log

#### Is the swallow-everything behavior really total (no error can escape)?

- Examined: internal/hook/hook.go Run/run (all return paths),
  internal/hook/log.go logError, internal/db/db.go Open DSN (line 341-345).
- Found: Run unconditionally returns nil (hook.go:69-72); every run() error
  routes to logError; logError silently ignores its own failures
  (log.go:40-42); busy_timeout is 2000ms so lock contention beyond that
  becomes a logged Upsert error.
- Not found: any retry, any stderr output, any exit-code signal. Confirmed
  total swallow.
