---
sut_path: /home/chussenot/Documents/waybar-pwetty-box
commit: f87ec19c3e40a62425b2145891c2b45d62a36363
updated: 2026-07-22
external_references:
  - path: /home/chussenot/agentic-db
    why: claude-status backend producing the tile-watch stream the claude tile consumes; data-contract assumptions span both repos
  - path: https://github.com/Alexays/Waybar
    why: host process that dlopens this cdylib via its CFFI module ABI; defines module lifecycle and FFI contract
---

# Existing Antithesis SDK Assertions

## Summary

**No Antithesis SDK instrumentation exists in this codebase.**

Scanned at commit `f87ec19` with case-insensitive search for `antithesis` across
all tracked files (excluding this `antithesis/` scratchbook itself):

- No Antithesis SDK crate in `Cargo.toml` / `Cargo.lock` (no `antithesis_sdk` or
  similar dependency).
- No calls to `assert_always!`, `assert_sometimes!`, `assert_reachable!`,
  `assert_unreachable!`, or their non-macro equivalents anywhere in `src/`,
  `examples/`, or `test/`.
- No `ANTITHESIS_*` environment variable references.

All instrumentation suggested by property evidence files in this scratchbook is
therefore **missing** (to be added); none is present or partially present.

## Assumptions

- ~~The scan covered the repo tree only; the external `~/agentic-db` backend
  repo was not scanned here.~~ **Closed 2026-07-22**: `~/agentic-db` was
  scanned (case-insensitive `antithesis`, full tree minus .git) — **no
  Antithesis SDK references there either**. The Go-side assertions the
  property catalog places in agentic-db are all net-new as well.

## Open Questions

- None.
