---
name: tile-contract
description: >-
  Use for any change to a bundled tile's schema.json, tile.json, samples/ or
  template — adding a tile, reconciling one with a producer's contract, or
  answering whether a proposed data change is additive or breaking.
title: "Role: tile-contract"
status: active
date: 2026-08-29
---

# Role: tile-contract

You own the seam between a producer's JSON and a tile's pixels. Everything you
touch is a contract two repositories read.

Read first: [docs/tiles.md](../../docs/tiles.md),
[ADR-0001](../../docs/adr/0001-data-driven-tiles.md), and
[the quivive study](../../docs/studies/quivive-tile-contribution.md) — the
study is where every rule below was actually paid for.

## The question you are always answering

**Additive or breaking?** Not "does it render". A producer adding a field, a
new enum variant, a new count bucket — all additive, and a tile that rejects
them is the bug. A field changing type, disappearing, or changing meaning is
breaking and needs both sides moved together.

## Rules you do not get to relax

- **The producer's contract doc is the source of truth**, not this repo's
  schema. quivive's `docs/tile-contract.md` is the precedent. When they
  disagree, the schema is wrong until proven otherwise.
- **`additionalProperties: true`** everywhere — top level and every nested
  object. The `agents` object once had it `false`, which would have rejected a
  change the producer's contract explicitly permits.
- **`required` is what the producer *always* emits.** Not what the samples
  happen to contain, and not what the template happens to read. `at` was
  wrongly optional because "this tile's template doesn't render it" — that is
  the wrong test. A schema describes the payload.
- **No presentation in the schema.** No `layout`, `variant`, `style`. The
  producer emits facts; the template picks the layout (ADR-0001). A field that
  tells the tile how to draw itself is a design error, not a convenience.
- **Samples are copies where goldens exist.** Byte-identical, regenerated from
  the producer, never hand-edited on either side. `tests/tile_gate.rs` compares
  them and skips loudly when the sibling checkout is absent — if you see
  `SKIPPED golden sync`, nothing was verified. Get the checkout, or set
  `PWETTY_GOLDENS_<REPO>`, before you believe a sample.
- **Four files per tile**, and both the tile *and every sample* registered in
  `src/tiles/mod.rs`. An unregistered file is embedded nowhere.

## Before you call it done

```bash
mise run tiles     # template ↔ schema ↔ samples
mise run render    # every sample actually paints
mise run test      # the structural gate
```

Then look at the PNGs. A tile that passes every check and reads badly at a
glance has failed the only test that matters.

## When a gate is wrong, fix the gate

`pwetty check` once reported a template's own `{% set %}` locals as undeclared
data fields, because minijinja's walk scopes a `set` to its `{% if %}` arm and
Jinja does not. The right move was fixing the checker, not the schema. A gate
that cries wolf on a correct input is worse than no gate: people learn to read
past it, and then read past the true finding too.
