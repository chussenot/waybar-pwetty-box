# shader-recompile-only-on-mtime-change

Focus: idempotency and replay — draw idempotency. Drawing the same content
twice with unchanged inputs must not perform new side effects. The
background-shader hot-reload check violates this in its failure state:
per-frame file read + GLSL recompile + stderr log + **leaked GL objects per
attempt**, escalating toward the draw-path `unwrap()` panics (host abort).

All suggested assertions are **net-new**; the codebase has no Antithesis
instrumentation (see `existing-assertions.md`).

## Code paths

- `src/lib.rs:54-79` — `Engine::refresh_shader`, called on **every GL draw**
  (`src/lib.rs:261`). The no-op guard is
  `if self.shader.is_some() && mtime == self.shader_mtime { return; }`.
  Because it requires `shader.is_some()`, any prior failure (compile error,
  unreadable file, missing file → `self.shader = None`) makes every subsequent
  draw re-read and re-compile **despite an unchanged mtime**. Note
  `self.shader_mtime = mtime` *is* stored before the attempt (lib.rs:65) — the
  state to make the attempt-once semantics work exists; only the guard ignores
  it.
- `src/shader.rs:313-328` — GL object leaks on every failed attempt:
  - `compile()` (shader.rs:330-339): on compile failure the created shader
    object is returned as `Err(log)` **without `delete_shader`** — leaked.
  - `link()`: if the fragment shader fails to compile, the already-compiled
    vertex shader `v` leaks (early `?` at shader.rs:315); if linking fails,
    the `program` leaks (Err return at shader.rs:326 without
    `delete_program`).
  So a persistent compile error leaks ~2 GL objects **per frame** (vertex
  shader compiles fine every time + the failed fragment shader object), at up
  to 30fps (`DEFAULT_ANIM_FPS`) since `background_shader` forces the tick
  callback (`src/lib.rs:332`).
- `ShaderPass` has **no `Drop` impl** (struct at shader.rs; confirmed by
  sut-analysis §5 p9c fix-completeness check: femtovg's is the only GL-calling
  destructor). On a *successful* hot-reload, `self.shader = Some(p)`
  (lib.rs:68) drops the old pass, leaking its program + VAO + FBO + texture —
  one full pass per mtime change.
- Escalation endpoint: `src/shader.rs:277,300` —
  `create_texture().unwrap()` / `create_framebuffer().unwrap()` in the
  per-frame path. GL name/memory exhaustion from the leaks turns a cosmetic
  retry storm into a panic inside a GTK signal handler → waybar SIGABRT
  (sut-analysis §7 item 2, §12 surface 2).
- Contrast (the codebase's *other* policy): span-level `ShaderCache` stores
  `HashMap<String, Result<ShaderPass, String>>` (`src/shader.rs:39-41`) —
  failures are cached **permanently**, insert-once. The two shader paths have
  opposite failure idempotency; neither is the intended middle ("retry on
  change").

## Failure scenario

1. User edits `background_shader` file and introduces a GLSL error (or the
   editor's write-replace leaves the file momentarily missing/empty).
2. Next draw: read + compile fails → `shader = None`, stderr log.
3. Every subsequent frame at ~30fps: fs read, full GLSL compile, stderr line,
   ~2 leaked GL shader objects. Hours of this: hundreds of thousands of leaked
   names + a flooded journal.
4. Eventually (Mesa-dependent): object allocation fails → `unwrap()` panic in
   the draw signal → the whole waybar process aborts (S1 severity — every bar
   on the desktop dies because a shader file had a typo).

A milder variant needs no error at all: a `touch` loop / live-editing session
leaks one full ShaderPass (program+VAO+FBO+texture) per mtime change.

## Suggested assertions (net-new)

- SUT `Always` in `refresh_shader`: track `last_attempted_mtime` and assert a
  read+compile attempt happens only when `mtime != last_attempted_mtime`:
  message **"background shader is recompiled only when its file mtime
  changes"**. `Always` because this is a per-evaluation invariant of every
  draw. **Known-violated at f87ec19** whenever a compile has failed — an
  immediate finding under the workload below, then a regression guard once the
  guard is fixed to drop the `shader.is_some()` conjunct (or gate retries on
  an interval).
- SUT `Unreachable` wrapping the `Err` returns of `link`/`compile` *after* GL
  objects were created but not deleted: message
  **"shader compile failure path leaked a live GL object"** — fires on every
  failed attempt today; documents the leak independent of the retry storm.
- Workload `Sometimes`: cycle the shader file through
  valid → broken → missing → valid states with randomized timing and assert
  the tile recovers to shader rendering after restoration: message
  **"background shader recovered to rendering after file was broken and
  restored"** — the hot-reload *liveness* the retry storm exists to provide;
  keeps a fix from over-correcting into ShaderCache-style permanent failure
  caching.

## Fault requirements

Workload-driven file manipulation (write/truncate/rm/touch of the shader
path). Antithesis fs fault injection (transient read errors) reaches the same
retry state without file changes. Long-run exploration observes GL object
growth; a run reaching the `unwrap()` panic converts this from resource
finding to crash finding.

## Key observations

- The intended invariant is clearly "compile once per file version" — the
  mtime field exists and is updated before the attempt; only the guard's
  `shader.is_some()` conjunct breaks idempotency, and only in the failure
  state. This is a one-line-fix-shaped property.
- The per-frame retry *is* what makes hot-reload recover after the user fixes
  the file (L8 in sut-analysis §4). A correct fix must retry **on mtime
  change** (which editing the file provides) rather than never — hence the
  workload `Sometimes` above guarding the liveness side.

## Open questions

- Is per-frame retry for file-based shaders deliberate (hot-reload UX) rather
  than an oversight? If deliberate, the invariant weakens to "at most one
  attempt per observed (mtime)" — which the code *still* violates in the
  failure state, so the property survives; only the fix shape changes
  (respect stored `shader_mtime` vs add a retry interval).
- What is the practical GL name/memory exhaustion threshold under Mesa
  surfaceless llvmpipe (the Antithesis-feasible stack, sut-analysis §6)?
  Decides whether the escalation-to-panic endpoint is reachable within a run's
  duration or only the leak-rate observation is.
