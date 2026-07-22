# prompt-priority-survives-session-cap

Focus: protocol contracts — the producer's documented "a prompt is never dropped"
guarantee (tile.go) and the template's hardcoded two-entry pulse check, which
together implement the product's core alert ("Claude is waiting for you").

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

Producer (`/home/chussenot/agentic-db/internal/tile/tile.go`):

- `tile.go:74-78` — `maxSessionsPerTile = 2` with the explicit claim: "statePriority
  ordering keeps the most salient two (**a prompt is never dropped**) so the tile's
  'any session prompt -> whole tile pulses' alert still fires."
- `tile.go:101-117` — `statePriority`: prompt=0, working=1, shell=2, idle=3,
  **unknown states=4 (sort last)**.
- `tile.go:159-173` — the sort (priority, then WindowID, then SessionID tie-break)
  followed by the truncation `ss = ss[:maxSessionsPerTile]`. This sort is the ONLY
  mechanism backing the claim.

Plugin (`/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/antithesis-research`):

- `tiles/claude/tile.json:7` — the template computes
  `prompting = sessions[0].state == 'prompt' or ((sessions | length) > 1 and sessions[1].state == 'prompt')`
  — it **only inspects indices 0 and 1**. For schema-valid payloads (maxItems 2)
  that covers everything; for a >2-session payload the pulse depends entirely on
  the producer's ordering. The ordering requirement is documented nowhere in
  `tiles/claude/schema.json` (which specifies maxItems but says nothing about
  element order).
- `src/content.rs:53-69` — `content_animates` matches `state='prompt'` and
  `<pulse` → the tile gets frame-clock ticks; `src/lib.rs:1042-1046` renders the
  `?` + bloom.

## Failure scenario

A desktop accumulates 3+ tracked sessions (e.g. kitty tabs, each a Claude
session sharing one niri window — the grouping is by workspace, `tile.go:401-410`,
so this is the designed path to >2). One of them transitions to `prompt` while
the daemon rebuilds the cache:

1. If the sort mis-orders (or a future edit changes the tie-break), the cap drops
   the prompt session → the payload carries only working/idle entries → no
   `<pulse` in the markup → **the user is never alerted**. Severity S2/S3 in the
   product table — the exact failure the product exists to prevent.
2. If the session's state string is dirty (`"Prompt"`, `"prompt "`, version-skew
   value), `statePriority` returns 4 (unknown, sorts LAST) → droppable by the cap
   even though the ground truth is a waiting prompt. The plugin side offers no
   mitigation: minijinja `==` is exact string equality (`"prompt " == 'prompt'`
   → false, empirically confirmed at f87ec19 — see Investigation Log), so a
   dirty state also fails the template's pulse check even when it survives the
   cap. Ties this property to `unknown-session-state-renders-blank`.
3. Even with ≤2 sessions, the end-to-end chain (DB → daemon tick 1s → 250ms
   throttle → cache write → tile-watch 75ms poll → plugin 150ms dirty poll →
   markup) has multiple debounce/dedupe stages; a prompt that flaps during
   cache-rebuild races could be persistently swallowed by the byte-dedupe.

## Suggested assertions (net-new)

- SUT-side Go `Always` in `PayloadFor` immediately after the truncation
  (`tile.go:173`), guarded on the pre-cap input containing a prompt session:
  message **"session cap kept a prompt session in the emitted payload"**.
  Details payload: input states, output states.
- SUT-side Go `Sometimes` at the same site, fired when `len(ss) > maxSessionsPerTile
  && input contains prompt`: message **"session cap engaged while a prompt was
  present"** — proves the interesting branch was actually exercised (otherwise the
  Always is vacuous).
- Workload `Always` (end-to-end): whenever the workload has placed a prompt
  session on desktop i (via DB row or injected cache payload), the rendered
  markup for tile i contains `<pulse` within 3s (generous vs the ~1.5s L4
  mechanism bound): message **"prompt session renders as pulsing tile within 3s"**.

## Key observations

- The claim is real and load-bearing (bead cek per sut-analysis §9), but enforced
  only by a sort comparator with zero tests on the >2-session path.
- The schema is silent on ordering; the template's index-0/1 check makes ordering
  a de-facto contract requirement only for schema-INVALID (>2) payloads — a
  contract hole rather than a bug today.

## Open questions

- Is >2 sessions per desktop actually reached in production (kitty-tab
  workflows)? If it never occurs, the SUT-side cap assertion should be
  `AlwaysOrUnreachable` instead of `Always`+`Sometimes`, and the workload must
  synthesize the state. If it is common, the current zero-test coverage is more
  alarming.
- Is 3s the right end-to-end bound under fault load (daemon paused, cache write
  throttled)? If Antithesis pauses the daemon process, the bound needs to be
  conditioned on the daemon being scheduled, or the workload assertion will
  false-positive. Determines how the workload check must be gated.

### Investigation Log

#### Does minijinja's `==` compare exact strings (does `"prompt "` with trailing space fail the template check)?

2026-07-22:

- Examined/probed: throwaway crate pinning `minijinja = "=2.21.0"` (exact
  Cargo.lock version) replicating `render_template` (`src/markup.rs:109-117`),
  run against the verbatim `tiles/claude/tile.json` template.
- Found: `{{ x == 'prompt' }}` with `x: "prompt "` → `false`. Full template with
  `{"sessions":[{"state":"prompt ",...}]}` renders with NO `<pulse>` wrapper —
  the markup carries `state='prompt '` verbatim. Note the same trailing space
  also defeats the plugin's animation gate (`content_animates` looks for the
  literal substring `state='prompt'`, which `state='prompt '` does not contain),
  so a dirty prompt state produces a non-pulsing, non-animating tile.
- Not found: nothing missing — no coercion or trimming exists anywhere in the
  comparison path.
- Conclusion: resolved — exact string equality, no lenient coercion; scenario 2
  is NOT mitigated on the plugin side. Invariant and assertion types unchanged.
