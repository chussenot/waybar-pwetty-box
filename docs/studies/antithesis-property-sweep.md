---
title: "Study: what the Antithesis property sweep actually produced"
status: active
date: 2026-08-29
---

# Study: what the Antithesis property sweep actually produced

Field evidence. A property-discovery sweep ran against this repository at commit
`f87ec19` and left `antithesis/scratchbook/` behind: a catalogue of 51 properties
with evidence files, four evaluation lenses, a deployment topology, and an
explicit finding that the codebase carries **no** Antithesis instrumentation at
all. No campaign has been run. This is the record of what the sweep was worth
anyway.

## The headline: five real bugs, fixed before any run

The sweep's value did not come from fault injection. It came from writing down
what the code *claims* precisely enough that reading it disproved five of the
claims. All five were fixed in one commit (`48cf74c`), none of them by running
anything:

| Found | Failure mode |
|---|---|
| Module teardown on SIGUSR2 reload | Leaked a background thread *and* a live producer chain per reload; the 150 ms dirty-poll timer held the widget ref forever. |
| Stream reader shutdown ordering | A decode error or over-cap line stalled indefinitely on a quiet producer, because the reader waited on a loop exit before killing the child. |
| Unbounded stream lines | An unterminated over-cap line grew without limit. Now capped at 64 KiB and treated like a decode error. |
| `<icon src>` reads | A FIFO or a huge file could wedge or bloat the whole bar. Now `fs::metadata`-guarded: regular file, ≤ 1 MiB. |
| Shader render-target creation | GL context loss panicked *inside the GTK draw callback* — a panic across the FFI boundary, i.e. aborting waybar. Now degrades to a transparent frame. |

Four of the five are liveness or resource bugs on the reload/stream path, which
is the path a bar spends its whole life on and the one hardest to observe: the
symptom of every one of them is "the bar is fine, and then it quietly isn't".

## The second-order finding

The evaluation lenses caught something about the sweep itself. The first
catalogue of 41 properties had **zero** covering backend ingress and state
derivation — the layer with six historical bug fixes, the buggiest in the
system. Two independent lenses converged on it, and the exclusion turned out to
be an accident of how the attack surface had been ranked, not a decision anyone
made.

Ten properties were added to close that and four other gaps. One of them
(`idle-decay-reaches-static`) found a live divergence at HEAD while being
written: a session with a NULL `last_talk` renders bright-and-animating forever
through one path and dimmest through another.

**The transferable part**: an attack-surface ranking is a hypothesis about where
bugs are, and it should be checked against where bugs have actually been. "Which
layer has the most fix commits" is a question with an answer in `git log`, and
it disagreed with the ranking.

## What is deliberately *not* here

No `antithesis_sdk` dependency, no `assert_always!`/`assert_sometimes!` calls,
no `ANTITHESIS_*` environment handling. Instrumenting ~51 properties for a
campaign nobody has commissioned would add a dependency and a few hundred
assertion sites to a bar module, in exchange for nothing until a run is bought.
That is [deferral row 6](../adr/0003-yagni-deferral-register.md), and its
reversal trigger is exactly "a run is commissioned".

The scratchbook also holds about fourteen `(needs human input)` design-intent
questions — what *should* happen on GL failure, on an out-of-range `idle_level`,
on an unknown session state. Those are open questions, not findings, and they are
the most useful thing in the directory for anyone extending the render path.

## Why the scratchbook is exempt from the docs gate

`antithesis/scratchbook/` carries the discovery tool's own front-matter schema
(`sut_path`, `commit`, `updated`, `external_references`) and is regenerated as a
set. Layering `title`/`status`/`date` on top would be reverted by the next sweep
and would leave the `docs` gate failing on a tree nobody edited. The exemption,
and the general principle behind it, are in
[the documentation contract](../documentation.md#the-exemptions-and-why-they-exist).

What has been promoted *out* of the scratchbook and into gated documentation is
this study and the deferral row — the conclusions, not the working. The
scratchbook stays as the working.
