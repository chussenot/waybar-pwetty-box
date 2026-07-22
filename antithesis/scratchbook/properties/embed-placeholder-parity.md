# embed-placeholder-parity — placeholder count in processed markup equals extracted embed count

Found independently by 2 discovery focuses (data-integrity + security) — merged during synthesis; independent rediscovery is a confidence signal.

All suggested assertions here are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Claim under test

sut-analysis F2 (§11): "U+FFFC in any data string injects a phantom embed and
shifts embed indices … 'N placeholders ↔ N embeds' is load-bearing and
unenforced." Spot-checked end to end — confirmed, with one correction: the
shifted indices do **not** panic (all consumers use `.get()`), they silently
misrender.

The markup pipeline reserves inline embeds (`<status>`, `<icon>`, `<tickerbox>`,
`<sep>`, `<wrap>`, `<gutter>`) by pushing exactly one `EMBED_PLACEHOLDER`
(`U+FFFC`, OBJECT REPLACEMENT CHARACTER) into the laid-out markup and one entry
into `Processed::embeds`, in the same order. The renderer later re-pairs them
positionally: `flow_layout` splits each markup line on `U+FFFC` and hands each
gap a monotonically-increasing `embed_idx`, which indexes `Processed::embeds`.
The invariant that makes this positional re-pairing correct is
**"N placeholders in the markup ⇔ N embeds, in matching order."**

## The trust-boundary defect / code paths

`U+FFFC` is not an HTML metacharacter and not an XML-forbidden character, so it
survives *both* escaping layers:

- Placeholder constant: `EMBED_PLACEHOLDER = '\u{FFFC}'`
  (`/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/antithesis-research/src/markup.rs:40`).
- minijinja HTML autoescape (`render_template`, src/markup.rs:109-117) escapes
  only `& < > " '` — not `U+FFFC`.
- The SUT's own `escape()` (src/markup.rs:93-99), applied to every text node in
  `walk_children` (src/markup.rs:204-206), replaces the same five chars — not
  `U+FFFC`.
- roxmltree happily parses `U+FFFC` as ordinary text (it is a legal XML 1.0
  character), so a `U+FFFC` embedded in a data string lands verbatim in a text
  node and is pushed into `out.markup` at src/markup.rs:205 **with no matching
  `out.embeds` entry**.
- The fallback branch (parse-failure path, src/markup.rs:145-154) also does not
  strip it.
- Legitimate placeholders: one pushed per embed tag alongside one `embeds`
  entry (markup.rs:224-234) — parity by construction, *only* for tag-created
  placeholders.
- Consumer: `flow_layout` (src/lib.rs:651-671) splits `processed.markup` on the
  placeholder char and assigns embed indices by a running counter over
  *occurrences* — it cannot distinguish tag-created from data-borne
  placeholders.
- Index consumers all `.get(idx)` and skip on miss: lib.rs:765-766 (hero),
  lib.rs:804 (wrap), lib.rs:901-905 (width pass), lib.rs:932-935 (`let Some …
  else continue` in the draw pass). So the failure is silent, never a crash.

Ingress: window titles / app labels originate from arbitrary apps and web pages
via niri, flow through the backend into the `title` / `app` / `folder` fields,
and are interpolated into the template — `tiles/claude/tile.json:7` renders
`{{ title }}`, `{{ s.folder }}`, `{{ app }}` inside the same flow that carries
the `<status>` / `<tickerbox>` embeds.

## Failure scenario (stock claude preset)

Template (`tiles/claude/tile.json:7`) puts `{{ s.folder }}` and `{{ app }}` in
the **flow text** (titles live inside `<tickerbox>`/`<wrap>` inners, which are
pulled out of the flow and are immune — verified: `draw_ticker` lays inner out
directly via Pango, lib.rs:1543-1567).

`s.folder` is `filepath.Base(session.cwd)` (agentic-db tile.go:122-124) — a
directory name, which may contain any byte except `/` and NUL. A session
started in a directory whose name contains U+FFFC:

1. The folder string lands in flow text with a literal U+FFFC.
2. `flow_layout` sees an extra placeholder → emits a phantom
   `FlowItem::Embed(k)` consuming index k.
3. Every subsequent real embed renders with the *previous* embed's data: in the
   dual-session tile, session B's row can show session A's status mascot; the
   final real placeholder gets an out-of-range index and its embed (a status or
   tickerbox) **silently vanishes**.
4. Pixels now show attributions the producer never emitted — wrong-data-
   displayed, calm and plausible, the S2 shape.

Concrete index arithmetic (worked example): template renders (document order)
`<status …/>` `<tickerbox>…</tickerbox>` and a title field containing one
injected `U+FFFC` *before* the tickerbox. After `process()`:

- `out.markup` placeholder sequence L→R: `P_status`, `P_injected`, `P_ticker`
- `out.embeds` = `[status, tickerbox]` (len 2)

`flow_layout` assigns: status→idx 0, injected→idx 1, ticker→idx 2. Then:

- the injected placeholder renders `embeds[1]` = the **tickerbox** at the wrong
  screen position (src/lib.rs:932-935 / 901-905);
- the real tickerbox gets idx 2 → `processed.embeds.get(2)` = `None` → silently
  skipped (`else { continue }`, src/lib.rs:933-935).

For the dual-session claude tile, the per-session `<status>` embeds carry the
attention-critical `state='prompt'` marker. A `U+FFFC` in an earlier session's
folder/title shifts every subsequent status embed by one and drops the last —
so the tile can render a `prompt` session's status slot with a *different*
session's badge, or drop it entirely. Attention masked by untrusted title data
(product severity S2).

## Suggested assertion (net-new, SUT-side, Rust plugin)

Both discovery focuses converged on the same invariant and type — **Always**,
parity of placeholder count vs `embeds.len()` — with two equivalent placement
variants (pick one site only, do not duplicate):

- Placement A: immediately after `markup::process` in `draw_content`
  (lib.rs:531): `placeholder_count(processed.markup) == processed.embeds.len()`.
  Message: `"markup process: embed placeholder count in processed markup
  equals extracted embed count"`.
- Placement B: inside `markup::process` immediately after `walk_children`
  (count of `U+FFFC` in `out.markup` equals `out.embeds.len()`) — keeps the
  check GTK-free/unit-testable.

This is a true per-evaluation invariant of the render contract; it is the
precondition the entire flow-composition path silently assumes. The point of
the assertion is that hostile data breaks it, which Antithesis surfaces as a
finding with the exact input.

## Antithesis angle

This is workload-input-shaped rather than fault-timing-shaped: the workload
must create sessions whose cwd basename (or an app name) contains U+FFFC —
trivially scriptable (`mkdir $'￼-dir'` + start a session there). Fault
injection then multiplies coverage by racing payload churn against draws, but
the trigger itself is data. Value for Antithesis: the assertion turns a silent
misrender into a first-class finding, and the parity check guards every future
template/preset, not just the claude one.

## Key observations

- A worse variant exists outside the claude preset: U+FFFC inside a *standard
  tag attribute value* (data bound into e.g. `<span foreground='{{ v }}'>`)
  would put the placeholder inside a tag in `processed.markup`; `flow_layout`'s
  split would then cut mid-tag, producing segments that are invalid Pango
  markup in addition to the index shift. No bundled template binds data into
  standard-tag attributes today, so this is latent — the same single assertion
  catches it.
- `\n` in flow data adds flow lines and shifts geometry (F2) — a layout
  distortion, not an integrity violation; no assertion proposed.
- C0 control chars in data collapse the whole tile to escaped tag soup via the
  roxmltree parse failure fallback (markup.rs:145-154) — visible and honest
  (S4-shaped), covered by the existing fallback behavior; left uncataloged
  under this focus.

## Open questions

- Can U+FFFC survive niri's title/app_id plumbing in practice (for the `app`
  flow-text vector, as opposed to the folder vector which is confirmed
  trivially reachable via `mkdir`)? Mechanism is code-certain end-to-end in the
  plugin; the only unknown is whether niri or the backend happens to filter it.
  Why it matters: if the backend strips `U+FFFC`, the property is a latent
  defect (still worth an assertion, since any other exec producer can inject
  it) rather than a live one — the priority framing changes (live vs latent,
  and "local misconfiguration" vs "external input corrupts displayed
  attribution"), not the invariant. If niri does not sanitize, web-page-
  controlled titles become a second, remote-influenced vector.
  `(partial: mechanism code-certain in the plugin; niri-side sanitization unchecked)`
- Does `wrap`/`gutter`/`sep` interact with the shift differently? They are also
  embed tags and share the front cursor, so the same shift applies; the
  attention-relevant case is `<status>`, already covered.

### Investigation Log

#### Can U+FFFC survive niri's title/app_id plumbing in practice?

Investigated 2026-07-22.

- Examined: the full plugin-side chain — placeholder constant
  (src/markup.rs:40), minijinja autoescape (markup.rs:109-117), the SUT's own
  `escape()` (markup.rs:93-99) applied in `walk_children` (markup.rs:204-206),
  the parse-failure fallback (markup.rs:145-154), tag-created parity
  (markup.rs:224-234), and the consumer `flow_layout` plus all index consumers
  (src/lib.rs:651-671, 765-766, 804, 901-905, 932-935); template ingress
  (tiles/claude/tile.json:7); backend derivation of the folder vector
  (agentic-db tile.go:122-124, `filepath.Base(session.cwd)`).
- Found: the mechanism is code-certain end-to-end inside the plugin — U+FFFC
  survives both escaping layers and roxmltree, lands in `out.markup` with no
  matching `embeds` entry, and shifts indices silently. The folder vector is
  confirmed trivially reachable (`mkdir` with U+FFFC in the basename; a
  directory name may contain any byte except `/` and NUL).
- Not found: whether niri's title/app_id plumbing (or the backend) filters
  U+FFFC on the `app`/title path — niri's source was not examined during
  discovery.
- Conclusion: tagged `(partial: ...)` — plugin mechanism confirmed; the
  niri-side sanitization check remains open. Its answer changes only the
  live-vs-latent priority framing (and whether web-page-controlled titles are
  a remote vector), not the invariant or the assertion.
