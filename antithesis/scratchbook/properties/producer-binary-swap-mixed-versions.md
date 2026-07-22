# producer-binary-swap-mixed-versions

Focus: version compatibility — upgrading the `claude-status` binary while the
system runs creates a **mixed-version producer fleet with no handshake**: the
long-lived daemon keeps running the old build, while each of the ten
`tile-watch` children is silently replaced by the new build at its next
respawn. The seam between them, `tiles.json`, carries **no format-version
marker**, and the new tile-watch re-emits whatever it tolerantly unmarshals
from the old daemon's cache, unvalidated.

All suggested assertions are **net-new** (neither this repo nor ~/agentic-db
has any Antithesis instrumentation; see `existing-assertions.md`).

## Code paths / validated mechanism

- **Respawn re-execs from PATH every time.** src/content.rs:256-285
  (`spawn_stream`): the respawn loop runs `Command::new("sh").arg("-c")
  .arg(&cmd).spawn()` on *every* iteration — fresh PATH resolution per
  respawn. Replace the `claude-status` binary on disk and the next respawn
  (after EOF/kill + 1s `RESPAWN_BACKOFF`) runs the new version. Tile-watches
  that haven't died keep running the old build from their unlinked inode — a
  mixed fleet across the ten tiles is a reachable steady state, not a blip.
- **The daemon does not turn over.** The daemon is a separate long-lived
  process with no restart unit (SUT analysis §7: it even exits permanently
  when the niri event stream closes). A binary upgrade changes nothing about
  the running daemon; old-daemon/new-watch coexistence lasts until someone
  manually restarts the daemon.
- **No version marker in the cache.** ~/agentic-db internal/tile/tile.go:
  `WriteCache`/`WriteCacheBytes` (≈420-437) marshal a bare
  `map[string]Payload` (tmp+rename, atomic); `ReadCache` (≈439-450)
  unmarshals the same bare map. No envelope, no schema/format version, no
  daemon-build stamp anywhere in the file.
- **tile-watch re-emits the cache verbatim, unvalidated.** `RunWatch.emit()`
  (tile.go ≈502-539): read error → `emptyPayload`; success → `p = cached`,
  re-marshaled and printed. `state.Status.Valid()` exists (state.go:51-58)
  but has zero non-test callers — nothing between the cache bytes and the
  plugin's stdin checks anything.
- **Go's tolerant unmarshal converts shape skew into zero values.** Unknown
  fields are dropped; missing fields zero-value. A cache written by a
  different-shaped Payload version yields e.g. `Sessions: nil` (emitted as no
  `sessions` key → template `sessions[0]` → minijinja error card) or
  `SessionTile.State: ""` (emitted as `"state":""` — no omitempty on State,
  tile.go:66 — → template `state=''` → the renderer's silent `_ => {}`
  fallthrough, src/lib.rs:1072).
- **The shape HAS changed in history — this is not hypothetical.** agentic-db
  commit e34138c "feat(tile): adopt pwetty's data-driven sessions[] contract"
  replaced the pre-sessions payload shape; the struct-tag history also shows
  `state` flipping between `json:"state,omitempty"` and `json:"state"`. Two
  real, buildable versions of `claude-status` with incompatible tiles.json
  shapes exist in the repo's own history (cf65020..e34138c boundary) — a
  genuine two-version Antithesis topology, not a synthesized future.

## Failure scenario

User runs `go install` (or the package manager upgrades claude-status) while
the desktop is up. Over the next minutes, tile-watches die and respawn one by
one (any EOF, any kill, any reload) into the new build; the daemon stays old.
If the payload shape differs across the boundary: some tiles render from
old-shape emissions (old watches), some from new-shape re-encodings of
old-shape cache bytes (new watches) — zero-valued states rendering as
indicator-less rows, or nil sessions rendering error cards — while the bar as
a whole looks "mostly fine". The reverse skew (daemon upgraded and restarted,
stale old-format tiles.json still on disk, old watches lingering) replays an
old-format cache through new readers with the same zero-value outcome. In
every branch the user-visible symptom is per-tile, plausible-looking, and
silent — S2 territory, striking exactly the alert the product exists for.

## Relationship to adjacent properties

`tile-watch-output-schema-valid` owns "every emitted line conforms to the
vendored schema" in the single-version topology; `unknown-session-state-
renders-blank` and `idle-level-gate-clamp-divergence` own the plugin-side
detectors for specific bad payload values. This property owns the **topology
and transition**: that the mixed-version state is reachable at all, persists
indefinitely, and converges (or fails to) — their conformance assertions
become the *oracles* inside this property's harness.

## Suggested assertions (net-new)

- Workload `Sometimes` (topology reached): a tile-watch respawn executed a
  different claude-status build than the currently-running daemon (workload
  stamps each build; compares `/proc/<pid>` paths or version output). Message:
  **"mixed-version daemon and tile-watch fleet was reached"**. Without this,
  every other check silently tests the matched-version world only.
- Workload `Always` (conformance across the seam, distinct message from the
  single-version property): every line emitted by any tile-watch during a
  mixed-version window validates against the *older* of the two builds'
  vendored schema contract. Message: **"tile lines remain contract-valid
  under mixed claude-status versions"**. `Always` fits: it must hold on every
  emitted line; with today's two historical builds it is expected to fail —
  the finding is that no compatibility layer exists.
- Workload `Sometimes` (convergence liveness): within a bounded time after
  the daemon is also restarted onto the new build (fleet re-matched), every
  tile's rendered content derives from a new-build emission. Message:
  **"tile fleet reconverged after producer upgrade completed"**.

## Antithesis topology notes

Testable by building agentic-db at two commits into the image (e.g. HEAD and
pre-e34138c) behind a symlink the workload flips, combined with Antithesis
process kills (tile-watch, daemon) to drive respawns through the swap in
varied orders. This is the one axis in the version focus where *actually
running two versions* pays for itself; payload-value skew (new enum values,
wider idle ranges) is cheaper to explore via synthesized payloads and is
already owned by the protocol-contract properties.

## Key observations

- The swap trigger is any tile-watch death — which Antithesis injects
  natively — so the transition interleaves with everything else (reloads,
  daemon restarts, cache rewrites) for free.
- Because the plugin keeps last content on EOF and the respawn is silent,
  there is no observable marker of "this tile is now fed by a different
  program version" anywhere in the system.
- A version/format marker in tiles.json (one field) plus a `Valid()` call in
  `RunWatch.emit` would collapse most of this property's failure space; both
  are backend-side, one-line-ish changes — worth noting for remediation
  priority.

## Open questions

- What does the pre-e34138c payload actually contain at top level (exact old
  shape)? Why it matters: determines which oracle fires under skew (nil
  sessions → error card, vs zero-valued state → silent blank row) and
  therefore which severity class the mixed window lands in; the harness
  should pick the older commit to maximize shape distance.
- Does `go install` replace the binary atomically (rename) or in place? Why
  it matters: in-place truncation of a running daemon's/watch's binary can
  crash the *old* processes mid-window (SIGBUS on cold page), which would
  masquerade as the respawn-driven swap but is a different mechanism; the
  workload's symlink-flip design sidesteps this, but a realism variant should
  mirror the real installer's mode.
- Is old-daemon/new-watch the only realistic direction, or does the reverse
  (new daemon, old lingering watches) occur in practice? Why it matters: the
  reverse direction exercises old *readers* of new cache bytes — a different
  tolerant-unmarshal surface; if reachable (it is, whenever the daemon is
  restarted first), the workload should drive both orders explicitly.
