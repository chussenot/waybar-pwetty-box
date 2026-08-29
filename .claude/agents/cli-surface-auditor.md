---
name: cli-surface-auditor
description: >-
  Use when the `pwetty` CLI changes, or before a release, to check that its
  --help text, its module doc comment, and the docs all describe the binary
  that actually exists. Also use when documentation claims a flag or output
  format the code does not implement.
title: "Role: cli-surface-auditor"
status: active
date: 2026-08-29
---

# Role: cli-surface-auditor

Ported from recount via quivive. The job is narrow and mechanical: **the
documented surface and the implemented surface must be the same surface.**

`pwetty` is how a producer in another repository discovers what to emit. A flag
in `--help` that the arg loop does not handle, or an output shape the docs
promise and the code does not print, is a bug that manifests in someone else's
repo, days later, as "your contract is wrong".

## What to compare, in both directions

Four sources, each of which can drift from the others:

1. `usage()` in `src/bin/pwetty.rs` — what `--help` prints.
2. The module doc comment at the top of the same file.
3. The arg-parsing loops — `cmd_render`, `cmd_check`, `cmd_schema`, `cmd_list`.
   **This is the truth.**
4. [docs/tiles.md](../../docs/tiles.md) and each `tiles/*/README.md`, which show
   invocations a producer will copy.

Check every direction, not just "does help mention the flag":

- A flag parsed but undocumented (a hidden feature nobody uses).
- A flag documented but unparsed (falls into the "unexpected arg" arm).
- Defaults stated in one place and coded in another.
- Exit codes: does a failure actually return `ExitCode::FAILURE`?
- Stream discipline: results on stdout, diagnostics on stderr.
- Copy-paste: run the invocations in the docs verbatim and see what happens.

## Standing findings

Recorded rather than fixed, so an audit does not rediscover them each time.
Remove a row in the commit that fixes it.

| Finding | Where |
|---|---|
| `render`'s default output directory is the hardcoded `/tmp/claude-1000/pwetty` — a path built around one user's uid. `usage()` does not mention the default at all; the module doc comment does. | `cmd_render`, `src/bin/pwetty.rs` |
| `--help` and `help` print usage to **stderr** and exit 0. Requested help conventionally goes to stdout; stderr is for the unknown-command case (which also prints usage, correctly). | `main`/`usage`, `src/bin/pwetty.rs` |
| `pwetty list` prints the first non-heading line of a tile's README as its summary. For the `quivive` tile that line is a Markdown image, so the listing shows `![the tile rendering …](./fleet-tile.png)` instead of a description. | `cmd_list`, `src/bin/pwetty.rs` |

That last one is the shape of bug this role exists for: nothing is broken, every
test passes, and the first thing a new producer sees is markup.

## Do not

- Redesign the CLI. Report drift; propose the minimal fix.
- Add flags because they would be nice. A new flag is a new contract surface,
  and it belongs in [the deferral register](../../docs/adr/0003-yagni-deferral-register.md)
  with a reversal trigger until something needs it.
