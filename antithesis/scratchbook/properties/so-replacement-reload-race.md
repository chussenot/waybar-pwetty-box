# so-replacement-reload-race

Focus: version compatibility — mid-run replacement of the plugin binary. The
documented deployment points waybar's `module_path` directly at the build
output (README:35, README:45 "/abs/path/to/target/release/libpwetty_box.so";
examples/waybar-config.jsonc:8 same pattern), so every `cargo build --release`
rewrites the .so **while waybar has it mapped**, and every SIGUSR2 reload
re-dlopens whatever bytes are at that path at that instant. Version transition
is therefore a race, not an event.

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths / validated mechanism

- **Host never dlcloses; every module construction dlopens fresh.** Waybar
  0.15.0 `src/modules/cffi.cpp:19`: each `CFFI` constructor does
  `void* handle = dlopen(dynlib_path.c_str(), RTLD_LAZY);` into a *local*
  that is never stored and never dlclosed; the destructor calls only
  `hooks_.deinit` (cffi.cpp:97-101). So a reload issues one fresh `dlopen`
  call per constructed module (M calls for M modules). Same-inode dlopen
  just bumps the refcount on the existing mapping; a genuinely new second
  mapping appears only when the path resolves to a new inode (i.e., after a
  replacement).
- **Reload ordering is destroy-all-then-construct-all** (verified from 0.15.0
  tag source, 2026-07-22 — see Investigation Log): the old generation is
  fully destroyed by `bars.clear()` (src/client.cpp:314) at the end of the
  quitting `Client::main` iteration, *before* the next iteration constructs
  any new module. Two generations of **live instances** never coexist across
  a reload; what coexists is **mapped code** (the old mapping is never
  unloaded). The real mixed-version window is *within* one reload: each of
  the M constructions does its own `dlopen(path)` sequentially
  (`getModules`, bar.cpp:527-581), so a replacement landing mid-pass yields
  modules 1..k on the old inode's code and k+1..M on the new (or skipped, if
  the file is torn) — mixed versions inside a single new generation.
- **dlopen failure is contained (validated).** A dlopen error throws
  `std::runtime_error{"Failed to load CFFI module: ..."}` (cffi.cpp:20-22);
  `Bar::getModules` wraps `factory.makeModule` in
  `catch (const std::exception&)` → `spdlog::warn("module {}: {}")` and skips
  the module (waybar 0.15.0 src/bar.cpp:532, 552, 577-579, fetched from tag).
  Host survives; the tile is simply **absent until the next reload**, with
  only a warn log.
- **Replacement mode determines the hazard class (empirically checked).**
  - Cargo rebuild: measured in this worktree — `target/release/
    libpwetty_box.so` inode changed 1767658 → 1767665 across a
    touch-and-rebuild, and target/deps are hardlinks of one inode. So cargo's
    relink **unlinks and recreates**: waybar's existing mapping keeps the old
    (now anonymous) inode — no SIGBUS from a rebuild alone. The exposure is
    the **write window**: during the ~seconds the linker writes the new 7.7MB
    file, the path names a truncated ELF; a reload in that window gets a clean
    dlopen failure (contained, above).
  - In-place copy (`cp new.so path`, a natural manual deploy): truncates and
    rewrites the *mapped* inode. Any not-yet-resident page fault in old plugin
    code then raises SIGBUS in waybar — host death (standard mmap semantics;
    not locally reproduced — see open questions). Note the main checkout
    currently exhibits a *different* inode for `target/release/
    libpwetty_box.so` vs `deps/` (60356637 vs 60356851), i.e. a copy not a
    hardlink — evidence that both file-management modes occur in practice on
    this machine.
  - Go toolchain (`go install`, relevant to the producer binary and to any
    future Go-built artifact; probed 2026-07-22 with strace + go1.26.5 source,
    see Investigation Log): **never truncates the destination inode in
    place**. Same-filesystem work dir → single atomic `rename()`; the default
    config on this machine ($WORK on tmpfs `/tmp`, destination on ext4) hits
    `EXDEV` and falls back to **unlink + create-new + io.Copy** — a non-atomic
    window where the path is briefly absent, then names a partial binary, but
    the old inode always survives for anyone executing or mapping it. So the
    SIGBUS-class mode (3) arises only from a bare manual `cp`, never from
    cargo, `go install`, or agentic-db's `mise run install` (which does
    `cp` to a temp name + `mv -f` — atomic rename, mise.toml:19-23).
    Caution for the workload's detection logic: inode-number equality is NOT
    proof of in-place overwrite — in the probe, ext4 reused the freed inode
    number across an unlink+recreate (strace showed `unlinkat` + `openat
    O_CREAT|O_TRUNC` while `ls -i` reported the same number).
- **ABI-shifted re-dlopen.** After replacement, reload runs the old .so's
  destructor path (the p9c abort — owned by `module-teardown-never-aborts-host`)
  and then the new .so's `wbcffi_init`. The two copies have independent Rust
  statics, thread-locals, EGL contexts; no cross-copy sharing was found. The
  new copy re-declares `wbcffi_version` (=1) and is re-checked by the host
  (cffi.cpp:31), so a plugin built against a future ABI would be rejected
  per-module, not crash.

## Failure scenario

Developer runs `cargo build --release` (10-60s) while waybar is up; a config
edit or scripted `pkill -USR2 waybar` lands inside the linker's write window.
Outcome A (cargo mode): dlopen fails, **all ten `cffi/pwetty#N` modules vanish
from the bar** with only warn logs, and stay gone until someone reloads again
— the "upgrade" reads as the plugin silently disappearing. Outcome B (cp-mode
deploy): waybar SIGBUSes at an arbitrary later page fault — a delayed,
hard-to-attribute full-bar crash whose trigger (the earlier `cp`) is minutes
gone. Outcome C (torn reload after successful replace): old-code teardown
aborts (p9c) — already catalogued separately.

## Suggested assertions (net-new)

- Workload `Always` (process liveness, checked after every
  replace/reload/draw cycle): message **"waybar survives plugin
  shared-object replacement racing reload"**. `Always` fits: host survival
  must hold on every evaluation regardless of interleaving. Expected to hold
  in unlink+recreate mode and to *fail* in truncate-in-place mode — the split
  result is itself the finding (it turns "how you copy the file" into a
  documented deployment contract).
- Workload `Sometimes`: a reload landed inside the replacement write window
  and waybar logged the CFFI load failure (grep the log for `Failed to load
  CFFI module`). Message: **"reload dlopened the plugin mid-replacement and
  skipped the module"**. Proves the race window was actually explored; without
  it a green Always is vacuous.
- Workload `Sometimes` (recovery liveness): after a completed replacement, a
  subsequent reload rendered tiles again (new build loaded; detect via a
  build-stamp the workload embeds in the .so filename-adjacent metadata or a
  probe tile). Message: **"plugin reloaded successfully from the replaced
  shared object"**.

## Antithesis topology notes

Highly testable, no second SUT version required: the workload needs the built
.so plus a mutator that replaces it in all three modes — (1) write-to-temp +
rename (atomic), (2) unlink + slow rewrite (cargo-shaped, tunable window),
(3) in-place truncate + rewrite (cp-shaped) — interleaved with SIGUSR2
reloads and normal tile traffic. Two .so builds (identical source, different
build-stamp) make the recovery Sometimes checkable. Runs on the proven
headless stack (cage + niri + waybar, SUT analysis §6).

## Key observations

- The config comments' intent (a pinned copy outside target/) would eliminate
  the entire class; the property documents the cost of the shortcut the README
  actually teaches.
- **Live deployment confirms the hazard is live, not hypothetical (resolved
  2026-07-22).** The user's real config (`~/.config/waybar/config.jsonc`)
  points all 20 `cffi/pwetty#N` modules' `module_path` at
  `/home/chussenot/Documents/waybar-pwetty-box/target/release/libpwetty_box.so`
  — the raw build output. The config's own comment (lines 204-207) claims the
  opposite: "The module_path is the PINNED build (~/.local/lib/pwetty), so
  pwetty rebuilds can't break this bar; promote a new build with
  `pwetty-promote`" — but `~/.local/lib/pwetty/` does not exist and no
  `pwetty-promote` exists on PATH, in `~/.local/bin`, or anywhere in this
  repo. The mitigation was designed (in prose) and never implemented, and
  the comment now actively misleads: every `cargo build --release` in the
  main checkout rewrites the .so waybar has mapped, exactly the scenario
  this property tortures.
- Never-dlclose means every completed replacement+reload cycle **permanently
  grows** waybar's mapped-code footprint by one plugin copy (~7.7MB) — a slow
  leak that compounds with the reload producer-chain leak
  (`reload-conserves-producer-chains`); N upgrade cycles = N+1 resident
  copies.
- The mid-write dlopen is assumed to fail *cleanly*. dlopen validates ELF
  headers/program headers, but a file torn at a page boundary after valid
  headers is less charted — see open questions.

## Open questions

- Does dlopen of a partially-written .so always fail cleanly, or can a file
  torn after valid ELF headers map successfully and fault later? Why it
  matters: a clean failure caps outcome A at "module absent"; a successful
  map of garbage upgrades it to undefined behavior in-process — the Always
  assertion's expected-pass envelope changes, and the slow-rewrite mutator
  should specifically stage tears at section boundaries.
- Is SIGBUS-on-truncate actually reachable through waybar's access pattern
  (are cold plugin pages ever faulted late, or is everything resident after
  first draw)? Why it matters: if the .so is fully resident early, cp-mode is
  survivable in practice and the Always would (misleadingly) pass; the
  workload should fault cold paths post-replacement (e.g. first hover, first
  shader compile) to make the check honest.

### Investigation Log

#### Does waybar's SIGUSR2 reload path destroy old modules before or after constructing new ones? Does reload re-dlopen the .so?

2026-07-22.

- Examined: waybar 0.15.0 tag sources fetched from
  https://raw.githubusercontent.com/Alexays/Waybar/0.15.0/ — `src/main.cpp`,
  `src/client.cpp`, `src/bar.cpp`, `src/modules/cffi.cpp`.
- Found: strictly destroy-then-create, across two iterations of the
  `do { reload = false; ret = client->main(argc, argv); } while (reload);`
  loop (main.cpp:173-176). The RELOAD action (main.cpp:102-106) calls
  `Client::reset()` → `gtk_app->quit()` (client.cpp:318-322); the quitting
  `Client::main` then executes `bars.clear(); return 0;`
  (client.cpp:313-315), running every `CFFI::~CFFI` → `hooks_.deinit`
  (cffi.cpp:97-101) synchronously on the main thread. Only afterwards does
  the next `Client::main` iteration rebuild bars — `getModules`
  (bar.cpp:527-581) constructs modules sequentially, each new `CFFI`
  constructor performing its own `dlopen(dynlib_path.c_str(), RTLD_LAZY)`
  (cffi.cpp:19) into a local handle that is never dlclosed. Construction
  failures are caught per-module (`catch (const std::exception& e)` →
  `spdlog::warn("module {}: {}", ...)`, bar.cpp:577-578) and skip only that
  module.
- Not found: any overlap where an old module instance is still alive when a
  new one is constructed; any dlclose; any stored dlopen handle.
- Conclusion: resolved — no two live instance generations across a reload;
  mixed versions can only arise *within* one reload's sequential per-module
  dlopens racing the replacement (old-inode / new-inode / skipped-torn mix).
  Re-dlopen per module construction confirmed; the recovery `Sometimes`
  should distinguish a fully-populated new bar from a partially-populated
  one (k modules on the old inode, the rest new or skipped).

#### Does the live deployment point `module_path` at target/release or a pinned copy?

2026-07-22.

- Examined: `/home/chussenot/.config/waybar/config.jsonc` (read-only) — all
  20 `cffi/pwetty#N` `module_path` values and the bar-2 header comment;
  filesystem checks for `~/.local/lib/pwetty/` and a `pwetty-promote`
  binary/script (PATH, `~/.local/bin`, repo grep).
- Found: every module_path is the raw build output
  (`.../waybar-pwetty-box/target/release/libpwetty_box.so`). The comment at
  config.jsonc:204-207 claims a pinned build at `~/.local/lib/pwetty`
  promoted via `pwetty-promote`.
- Not found: `~/.local/lib/pwetty/` (directory absent), `pwetty-promote`
  (not on PATH, not in `~/.local/bin`, zero references in the repo).
- Conclusion: resolved — deployment is on the live build output; the pinned
  mitigation exists only as a stale comment. The replace-while-mapped hazard
  class is production-real (Key observations updated).

#### Does `go install` replace a binary atomically (rename) or in place?

2026-07-22.

- Examined: go1.26.5 toolchain source
  (`$GOROOT/src/cmd/go/internal/work/shell.go` — `moveOrCopyFile`,
  `CopyFile`, `mayberemovefile`; `exec.go:2000` install action); live probe:
  a throwaway module installed three times to a scratch `GOBIN` under
  `strace -f -e trace=rename,renameat,renameat2,unlink,unlinkat,openat`,
  with the default `GOTMPDIR` (tmpfs `/tmp`) and with `GOTMPDIR` on the same
  ext4 filesystem as `GOBIN`; `stat -f` on `/tmp` (tmpfs) vs `$HOME`
  (ext2/3/4).
- Found: `moveOrCopyFile` tries `os.Rename($WORK/b001/exe/a.out, dst)` —
  observed `renameat(...) = 0` when work dir and destination share a
  filesystem: **atomic replacement**, new inode. With the default tmpfs work
  dir the rename fails `EXDEV` and falls back to `CopyFile`, which is
  **remove-then-recreate**: `mayberemovefile(dst)` (observed `unlinkat`)
  followed by `openat(dst, O_WRONLY|O_CREAT|O_TRUNC)` on the now-absent path
  and `io.Copy` — non-atomic (path missing, then partial), but never a
  truncation of the *existing* inode, so a running/mapped old binary is
  unaffected (same protection class as cargo's unlink+recreate).
- Not found: any code path in the toolchain that opens an existing
  destination with `O_TRUNC` without unlinking first (the Windows-only
  rename-aside branch aside). Also noteworthy: the probe's ext4 reused the
  freed inode number across unlink+recreate, so inode-number comparison
  alone cannot distinguish the modes — syscall traces or content stamps can.
- Conclusion: resolved. `go install` is atomic same-fs, unlink+rewrite
  cross-fs (the default here), and never in-place. For this property the
  three-mode mutator taxonomy stands; mode (3) in-place truncate is reachable
  only via a bare `cp`-style deploy, which the workload must perform
  explicitly. No invariant or assertion-type change; the split-result
  expectation of the Always assertion is unchanged.
