# shader-recompile-gl-object-leak — evidence

No Antithesis instrumentation exists anywhere in this codebase (see
`existing-assertions.md`); every assertion suggested here is net-new.

## Claim

Background-shader recompilation must not grow the number of live GL objects:
after any sequence of shader-file edits (valid or invalid GLSL), the GL
objects attributable to the tile's ShaderPass stay constant per tile. At
f87ec19 this is violated on **three** distinct code paths, one of which
retries **per frame**.

## Code paths (all verified by direct code read at f87ec19)

1. **No `Drop` for `ShaderPass`** (`src/shader.rs:175-180`): the struct owns
   `program`, `vao`, and an optional `(fbo, texture)` target, and has no Drop
   impl. `Engine::refresh_shader` (`src/lib.rs:57-79`) replaces
   `self.shader = Some(p)` on every successful recompile — the old pass is
   dropped silently, leaking 1 program + 1 VAO + 1 FBO + 1 texture per
   file-touch into the shared EGL context.
2. **`compile()` error path leaks shader objects** (`src/shader.rs:330-339`):
   on compile failure it returns `Err(info_log)` **without**
   `delete_shader` — the failed shader object leaks. Worse, in `link()`
   (`src/shader.rs:313-328`) the `?` on the fragment compile leaks the
   already-compiled **vertex** shader too. So one failed compile attempt
   leaks 2 shader objects.
3. **Link-failure path leaks the program** (`src/shader.rs:320-327`): on
   `!link_status` it deletes both shaders but returns
   `Err(get_program_info_log(program))` without `delete_program`.

The multiplier — **per-frame retry for a failing shader file**
(`src/lib.rs:57-79`): the early-return guard is
`self.shader.is_some() && mtime == self.shader_mtime`. A file whose compile
fails sets `self.shader = None`, so the guard never passes again — every
subsequent draw re-reads the file, re-wraps, re-compiles, re-logs. And a
configured `background_shader` sets `forced = true` (`src/lib.rs:332`), so
the tick callback queues draws at `target_fps` (default 30, or 60 for the
demo config) continuously. Net: a shader file left with a syntax error leaks
2 GL shader objects × 30/s ≈ 216,000 objects/hour, plus one heap-allocated
info-log string and file read per frame, in the plugin's long-lived EGL
context inside waybar.

Contrast: `ShaderCache` (span `<glow>`/`<bg>` presets, shader.rs:94-109)
caches the `Err` permanently — no retry storm there, but also one-shot leak
of the failed objects, and permanently-dead effect until restart
(inconsistent policy noted in sut-analysis §7).

## Failure scenario

Shader hot-reload is an advertised feature (claim L8: "compile error
disables, doesn't wedge"). A user iterating on a shader saves an
intermediate broken version and walks away; waybar's memory and the GL
namespace grow at 30Hz until restart. Under Mesa/llvmpipe (the Antithesis
topology), GL objects are host heap — this presents as monotonic waybar RSS
growth. If GL name/ID allocation ever fails instead, `create_texture()/
create_framebuffer().unwrap()` (`src/shader.rs:277,300`) turns exhaustion
into a panic in a GTK signal handler → host abort (crossing into the
draw-path-panic property family — different property, same neighborhood).

## Suggested assertions (net-new)

SUT-side (authoritative — GL object counts are invisible to the workload):

- `AlwaysOrUnreachable`: "live ShaderPass GL object count stays constant per
  tile across recompiles" — instrument ShaderPass creation/deletion with an
  atomic counter (programs+shaders+VAOs+FBOs+textures created minus
  deleted); assert it stays ≤ a small constant × (number of shader-bearing
  tiles). `AlwaysOrUnreachable` because the path only runs when a harness
  tile configures a file-based `background_shader` — the claude preset does
  not (verified: no `background_shader` in tiles/claude/tile.json); "never
  executed" must remain acceptable for workload variants without such a
  tile.
- `Sometimes(compile_error_retry_observed)`: "a failing shader file was
  retried on a subsequent frame" — confirms the storm regime was reached
  (this is the exploration anchor for the 30Hz retry loop).

Workload-observable proxy:

- `Always`: "waybar RSS slope stays flat while a broken shader file is
  configured" — workload writes syntactically-invalid GLSL into the
  configured shader path, waits N seconds, samples RSS slope. Coarser but
  needs no SUT change; distinguishable from `stream-ingest-memory-bounded`
  by trigger (no adversarial producer running).

## Key observations

- Leak paths 2 and 3 (error-path shader/program leaks) are new findings from
  this pass — sut-analysis §2 recorded only the no-Drop recompile leak. The
  per-frame retry multiplies whichever error path the broken file hits.
- The "leak into the dying context" teardown policy (claim S9 rationale) is
  about *teardown*; this property is about *steady-state* growth in a
  context that lives as long as waybar — the policy does not excuse it.
- Fault-injection angle beyond file edits: filesystem faults that make
  `fs::metadata` flap (mtime `None` ↔ `Some`) or make `read_to_string` fail
  intermittently also churn `refresh_shader` — mtime errors are swallowed
  into `None` (src/lib.rs:61), and `None != Some(t)` re-triggers compile on
  recovery. Cheap for Antithesis to explore with fs fault injection.

## Open questions

- Under Mesa llvmpipe, at what rate does the 2-objects-per-frame leak grow
  RSS — is the workload-observable proxy sensitive enough within a run, or
  is the SUT-side counter mandatory? Matters: decides whether this property
  ships in the no-SUT-change first wave or waits for instrumentation.
- Does sustained GL name allocation in Mesa ever fail (returning error →
  the shader.rs unwraps → abort), or do names grow indefinitely? Matters: if
  exhaustion aborts, this property has an S1 crash escalation and should be
  cross-linked to the draw-path-panic property; if not, it stays a pure
  resource property.
