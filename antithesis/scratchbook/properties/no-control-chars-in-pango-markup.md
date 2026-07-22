# no-control-chars-in-pango-markup

## What this is

Claim S3/S4 (README, src/content.rs:8-9, src/markup.rs:142): "command output
can't break the markup" and "malformed markup falls back to escaped text, never
crashes Pango." The stated defense: `process()` wraps content in `<r>…</r>` and
parses with roxmltree; on parse failure it returns `escape(content)` as the
markup (src/markup.rs:145-154).

## The trust-boundary defect

C0 control characters (`U+0000`–`U+001F` except `\t \n \r`) are:

1. **not** escaped by minijinja HTML autoescape (src/markup.rs:109-117) or by
   the SUT's `escape()` (src/markup.rs:93-99 — only `& < > " '`);
2. **forbidden** in XML 1.0, so roxmltree rejects `<r>…C0…</r>` → the
   whole-tile fallback fires (src/markup.rs:147-153);
3. **still present in the fallback string**, because `escape(content)` copies
   the C0 byte through untouched.

Point 3 was expected to make the fallback itself invalid Pango markup — but an
empirical probe (2026-07-22, see Investigation Log) showed Pango's markup
parser is more tolerant than roxmltree: `set_markup` (src/text.rs:44-45, 81-82,
161-162) **accepts** the C0-carrying fallback string (probed `\x0B`, `\x1b`,
and `\x00` in a title; Pango 1.57.0). The tile renders the escaped tag soup as
literal text — no blank layout, no `g_warning`, no abort risk under
`G_DEBUG=fatal-warnings` on this path (no warning is emitted at all).

The confirmed consequence of one control byte in a data string:

- **Whole-tile collapse:** a single C0 char anywhere in the rendered markup
  fails the `<r>…</r>` parse, discarding *all* structured content — every
  `<status>`, `<icon>`, `<pulse>`, `<active/>`, `<bg>` — because they live in
  the same document. The whole markup renders as literal escaped tag soup and
  the attention-critical `prompt` status is lost (S2). Empirically confirmed
  end-to-end at f87ec19 via `pwetty render` (offscreen, real compose path).

Window titles are the ingress (arbitrary apps via niri). C0 bytes in titles are
plausible (terminal escape sequences, `\x1b`, `\x00` from buggy apps).

## Code paths

- roxmltree parse + whole-tile fallback: src/markup.rs:145-154
- `escape()` leaves C0 untouched: src/markup.rs:93-99
- data → autoescaped template output → `process()`: src/content.rs:128-130,
  src/markup.rs:109-117, src/lib.rs:531
- Pango `set_markup` entry points (no error surfaced to caller):
  src/text.rs:44-45, 81-82, 161-162
- SUT analysis flag: §11 F2 (C0 collapses tile to escaped tag soup), §4 S4

## Suggested assertion (net-new)

`Always`, at the Pango boundary (inside the `text::layout*` helpers, or on
`out.markup` at the end of `process()`): the markup string contains no
XML-forbidden control characters (i.e. no C0 except `\t \n \r`). Recommended fix
this documents: strip/replace C0 in the template-output path (or in `escape()`)
so a hostile title degrades to just its own span instead of collapsing the
whole tile to tag soup.

## Open questions

- Can C0 bytes survive niri title plumbing + the backend to reach a data
  string? Mechanism inside the plugin is code-certain; upstream trigger rate is
  unknown (SUT §12 open question). What changes: live vs latent framing.

### Investigation Log

#### What does `markup::process` + render do with a C0 byte in a title — tag-soup fallback, and does the PNG still render (does `set_markup` swallow or blank)?

2026-07-22 (subsumes the former open question "what does gtk-rs
`Layout::set_markup` do on a parse error — blank, unchanged, or
g_warning→abort under fatal-warnings"):

- Examined/probed: real compose path via `pwetty render claude --data <file>`
  (offscreen surfaceless EGL; `render_png` → `draw_content` →
  `markup::process`, src/lib.rs:531 → `text::layout` → `set_markup`,
  src/text.rs:45). Payloads: idle session with title carrying `\x0B` (VT),
  `\x1b` (ESC), and `\x00` (NUL); plus a clean-title control. Host Pango
  1.57.0, gtk-rs via waybar_cffi.
- Found: all three C0 variants rendered a PNG, exit 0, **no stderr warning, no
  crash/abort** — including NUL. The PNG shows the entire tile as literal
  escaped tag soup (`<span size='x-large' …`), proving the roxmltree
  whole-tile fallback fired (src/markup.rs:147-153) AND that Pango's
  `set_markup` *accepted* the fallback string despite the raw C0 byte it still
  carries — Pango's GMarkup-based parser tolerates C0 where roxmltree
  (strict XML 1.0) rejects it. The control render shows the normal styled
  card. So: tag-soup collapse yes; blank/swallow no.
- Not found: an actual Pango markup *parse error* was never produced — the
  general "what does set_markup do on a parse error" question was not
  exercised, but for the C0 input class it is moot (no parse error occurs).
  Behavior verified only on Pango 1.57.0; older/newer Pango could differ.
- Conclusion: resolved for this property's input class. Severity settles at S2
  (silent whole-tile collapse to tag soup, prompt/pulse lost) — NOT S1; no
  host-abort path exists here (no warning is emitted for fatal-warnings to
  escalate), so a second host-abort-on-hostile-title property is not
  warranted. The `Always` assertion and its site stand unchanged; the body's
  earlier "blank tile / fallback is not Pango-safe" prediction was corrected.
