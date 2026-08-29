---
title: The documentation contract
status: active
date: 2026-08-29
---

# The documentation contract

Gated by [`scripts/check-docs.py`](../scripts/check-docs.py), which runs as the
`docs` leg of `mise run check`. If a rule here is not in that script, it is a
preference, not a rule.

## Front matter

Every Markdown file starts with YAML front matter:

```yaml
---
title: The documentation contract
status: active
date: 2026-08-29
---
```

- `title` — non-empty.
- `status` — `draft` | `active` | `superseded`.
- `date` — `YYYY-MM-DD`, the day the content was last meaningfully true.

ADRs add `decision-makers`, and `supersedes` when they replace an earlier one.

### The exemptions, and why they exist

Front matter is skipped for files a *tool* owns and rewrites. A rule a tool
un-applies is a fight, not a rule:

| Exempt | Owner | Why |
|---|---|---|
| `tiles/*/README.md` | this repo's build | `include_str!`-embedded into the `.so` and printed verbatim by `pwetty list`/`schema`. Front matter there is noise in a CLI's output. |
| `AGENTS.md`, `CLAUDE.md` | `bd setup` | carry generated, delimited blocks that `bd` rewrites in place. |
| `.beads/`, `.agents/` | `bd setup` | generated wholesale. |
| `antithesis/` | the Antithesis property-discovery run | carries its own front-matter schema (`sut_path`, `commit`, `external_references`) and is regenerated as a set. Imposing a second schema on top would be overwritten on the next sweep. |

`antithesis/` is the one exemption that was *added* rather than inherited. The
alternative was to write `title`/`status`/`date` into ~60 generated property
files — cost: a 60-file diff that the next discovery run silently reverts, and a
gate that then fails on a tree nobody touched. The exemption costs a scratchbook
that documents its own provenance in its own vocabulary, which is what it was
already doing. See [the study](studies/antithesis-property-sweep.md) for what
that sweep produced and which parts have been promoted into gated docs.

Links inside `.beads/`, `.agents/` and `antithesis/` are not resolved either:
they point at absolute paths on the machine that generated them and at sibling
checkouts this repo does not vendor. Tile READMEs *are* link-checked — their
links (`./schema.json`, `./samples/`) are part of the contract a producer reads.

## Links

Relative links and images must resolve on disk, and `#anchor` must match a
heading in the target file. External `http(s)` links are not fetched.

## Where things live

| Kind | Location | Shape |
|---|---|---|
| Decisions | `docs/adr/NNNN-<slug>.md` | context, the decision, **priced** alternatives, consequences |
| Field evidence | `docs/studies/<slug>.md` | what actually happened, what it cost, what rule it produced |
| Reference | `docs/<topic>.md` | how a thing works |
| Contracts | `tiles/<name>/README.md` | what a producer must emit |

The README answers **why** and links out. Reference material that grows in the
README belongs in `docs/` — one place per fact.

## Diagrams

**Architecture diagrams are Mermaid, in the Markdown.** They are diffable,
they render on GitHub, and nobody has to find the source file for the picture.
Images-as-diagrams (an exported PNG of a box-and-arrow drawing) are banned.

**Screenshots of rendered tiles are different, and they are core content.** This
is a visual project: a tile's README without a picture of the tile is missing its
most useful line. Those images are allowed anywhere, on one condition — they are
produced by `pwetty render` against real data, never mocked up by hand. A
screenshot is evidence; a mock-up is a claim.

## Alternatives are priced

An ADR that lists an alternative without saying what it would have cost has not
considered it. "Rejected: too complex" is not a price. "Rejected: a second
config path through `resolve()`, and every producer would then have to know the
tile's layout" is.
