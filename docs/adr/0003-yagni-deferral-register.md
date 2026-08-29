---
title: "ADR-0003: The YAGNI deferral register"
status: active
date: 2026-08-30
decision-makers:
  - chussenot
---

# ADR-0003: The YAGNI deferral register

## Context

This repository has said "not yet" a lot, and mostly said it well: the seams for
the deferred things are already cut (`EFFECT_TAGS`, `markup::Embed`,
`ShaderCache`), so the deferrals cost design work, not just typing. But a
deferral recorded only as a parenthesis in a README —
*"(v1: a tile uses either inline embeds or span effects, not both.)"* — decays
into folklore. Six months later nobody remembers whether that was a limitation,
a bug, or a decision, and the cheapest way to find out is to try it and see what
breaks.

A deferral without a **reversal trigger** is indistinguishable from an oversight.

## Decision

Every deliberate "not yet" lives in the register below, and every row names the
observation that would make it "now". When a trigger fires, the row is removed in
the same commit that implements the thing — not before, not later.

Rows are added by the change that defers, not retrospectively.

| # | Deferred | Seam that already exists | Reversal trigger |
|---|---|---|---|
| 1 | **User-supplied per-span shaders** — `<shader src='…'>` alongside `<glow>` | `EFFECT_TAGS` in `lib.rs`; `ShaderCache` keys by source, so a second span shader is a registration, not a subsystem | A tile needs a per-span effect `<glow>` cannot express, *and* the effect is wanted by a second tile. One tile's one-off is a built-in, not a plugin surface. |
| 2 | **`<bar>` / `<ring>` / `<sparkline>` inline embeds** | `markup::Embed` → measured placement → `draw_flow`; `<wrap>`/`<tickerbox>`/`<status>` all ride it | A producer emits a numeric series (or a bounded ratio) that a tile currently renders as digits, and the digits are demonstrably harder to read at a glance in a rendered PNG. |
| 3 | **Mixing inline embeds with `<box>`/`<glow>` span effects in one tile** | Both paths exist; only their composition is untested | A tile design needs both. Cost when it fires: `draw_flow` and `span_rect` currently answer different questions about position, and reconciling them is the actual work — budget it, don't discover it. |
| 4 | **Antithesis instrumentation in the code** | `antithesis/scratchbook/` catalogues ~60 properties and their assertions; `existing-assertions.md` records that the codebase has none | An Antithesis run is actually commissioned. Until then the catalogue is a design artifact — see [the study](../studies/antithesis-property-sweep.md). |
| 5 | **Vendoring or submoduling the producer repos whose goldens we copy** | `tests/tile_gate.rs` compares against a sibling checkout and skips loudly when absent | The loud skip stops being loud enough: a drift between a tile's `samples/` and a producer's goldens ships to a user. Cost when it fires: this repo's test suite starts depending on another repo's layout. |
| 6 | **Installing the `.so` to a versioned or prefixed path** | `mise run install`; `module_path` points straight at `target/release` | Someone needs two builds of the module resident at once (an A/B of a render change on one bar). See [ADR-0002](0002-install-restart-contract.md) for why the copy was not worth it for the single-build case. |
| 7 | **A `[tools]` toolchain pin in `mise.toml`** | `rust-toolchain.toml` pins 1.92 and rustup honours it on the first `cargo` call | Never, unless `rust-toolchain.toml` stops being honoured. Two files that can disagree about the toolchain is the exact failure the pin exists to prevent. Recorded here so the next person does not add it as an obvious improvement. |

## Alternatives, priced

**Leave deferrals as README parentheses.**
Cost: zero now. Later, every "not yet" is re-litigated from scratch by whoever
next touches that seam, usually by implementing it to find out why it was
deferred. That is how row 3's `draw_flow`/`span_rect` mismatch gets discovered
mid-feature instead of budgeted. Rejected.

**File each deferral as an issue in `bd`.**
Cost: nothing, and it is genuinely better for *scheduling*. It is worse for
*durability*: an issue tracker answers "what is queued", and a reader of the code
six months from now is asking "why is this seam half-cut", which is a question
for the tree. Not rejected — complementary. A row here may also be a bead; the
row is the one that survives.

**Just build the deferred things.**
Cost: rows 1 and 2 are each a day or two, and both would be built against no
user. The register exists precisely so
that "we could just do it" is a decision with a stated trigger rather than a
mood. Rejected.

## Consequences

- Removing a row is part of the commit that implements it. A row that outlives
  its feature is a documentation bug the `docs` gate cannot catch — reviewers can.
- **Cite a row by its name, never its number.** Removing a row renumbers every
  row below it, and the `docs` gate checks that a link resolves, not that it
  still points at what the prose claims. Two references rotted this way the
  first time a trigger fired.
- A trigger phrased as "when we need it" is not a trigger and should not be
  merged. Every row above names an observation someone could actually make.
- This ADR will be edited far more often than the other two. That is the point;
  its `date` moves with it.
