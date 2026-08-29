---
title: Versioning and the changelog
status: active
date: 2026-08-29
---

# Versioning and the changelog

[cocogitto](https://github.com/cocogitto/cocogitto) (`cog`), configured by
[`cog.toml`](../cog.toml), **from the commit that added it forward** — not from
day one. The repository predates the convention; roughly a hundred commits
before it do not follow conventional-commit rules and never will.

## The baseline tag

`cog.toml` sets `from_latest_tag = true`, and the last pre-convention commit is
tagged `v0.1.0`. Together those two facts mean `cog check` and `cog bump` never
walk pre-convention history: everything they see is a commit written under the
convention.

Cut the tag **before** the first conventional commit, not after. A tag placed
later leaves a window of ungoverned commits inside the range cog reads, and
`cog check` fails on history nobody can rewrite.

```bash
git tag v0.1.0 <last-pre-convention-commit>
git push origin v0.1.0
```

## Commits

Conventional commits, **with a scope**:

```
feat(tiles): add the quivive fleet-presence tile
fix(cli): stop reporting a template's own {% set %} names as data
docs(adr): record the install/restart contract
build(ci): run mise check on GitHub Actions
```

The scope is not optional here even though the conventional-commit spec makes it
so, because this repository has clearly separable areas (`tiles`, `cli`, `lib`,
`markup`, `shader`, `ci`, `docs`, `tasks`) and a scope-less subject line loses
the only cheap signal about what a commit touched. CI enforces presence; it does
**not** enforce an allowlist of scope names, because a new area should not need a
config change before its first commit can land.

Commit bodies explain **why**. The diff already says what.

Never name an AI, model, or assistant in a commit, tag, or PR.

## Bumps

Only through cog:

```bash
cog bump --auto      # or --patch / --minor / --major
```

It derives the version from the commits since the last tag, writes the changelog,
tags, and commits. Hand-editing the version in `Cargo.toml` and tagging by hand
produces a tag whose changelog cog will not be able to regenerate.

## The changelog has two layers

[`CHANGELOG.md`](../CHANGELOG.md) carries:

1. **A generated record** — what changed, written by `cog bump` from the commit
   subjects. Do not edit it; it will be regenerated.
2. **Hand-written notes** — *why* a release looks the way it does, in a section
   above the generated record. Migration steps, a decision that a release
   embodies, a warning about an interaction. That section is yours; cog never
   touches it.

A generated changelog answers "what landed". Only a human answers "and what does
that mean for me".

### The insertion trap

`cog bump` finds the **first** occurrence of its separator literal in
`CHANGELOG.md` and splices the new release immediately after it. In cocogitto
7.0.0 that literal is a dash-space-dash-space-dash Markdown rule.

So the hand-written preamble must **describe** that separator without
**containing** it. A preamble that spells the separator out gets the release
notes spliced into the middle of your own explanation — a mistake already paid
for once, in quivive 0.1.0. YAML front matter's own `---` fence is a different
literal and is safe.

The `cog` version is pinned transitively: CI installs cocogitto through
`cocogitto/cocogitto-action`, pinned by commit SHA, and the action carries the
`cog` version in its own `version.env`. Moving the action moves the binary —
which is exactly what should be reviewed when the separator literal could change
under us.

## In CI

`cog check` runs on pull requests only, over the PR range (`BASE..HEAD`). It
never runs over the whole history: the baseline tag makes that safe in principle,
and the explicit range makes it safe regardless of what tags a shallow CI
checkout happens to have fetched. See [testing.md](testing.md#ci).
