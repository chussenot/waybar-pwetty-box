---
title: GPU shaders
status: active
date: 2026-08-29
---

# GPU shaders

Two different things, deliberately kept apart: a **full-bleed background
shader** you point at a file, and a **mild masked accent** you drop into the
markup.

## `background_shader` — the full-bleed background

`background_shader` points at a **Shadertoy-style GLSL** fragment shader that
fills the whole tile, behind the content:

```jsonc
"background_shader": "/path/to/aurora.glsl",
"fps": 30,   // animate
"format": "<span weight='bold' foreground='#ffffff'>{{ time }}</span>"
```

The shader defines `void mainImage(out vec4 fragColor, in vec2 fragCoord)` and
receives `iResolution` / `iTime` / `iFrame` (paste-from-shadertoy.com friendly).
It's rendered on our own GL context into a texture, read back, and composited as
the background; the Pango content draws on top. The file is **hot-reloaded** when
it changes — recompiled only when its mtime actually moves — and compile errors
are logged rather than aborting the host.

### Data-reactive shaders

`shader_uniforms` binds tile data into `float` uniforms the shader can use — so
the background *responds* to the data:

```jsonc
"exec": "cpu-load.sh",                       // emits e.g. {"load": 6.4}
"background_shader": "reactive.glsl",         // declares: uniform float u_load;
"shader_uniforms": { "u_load": "{{ (load | float) / 8.0 }}" },
"format": "<span weight='bold' foreground='#ffffff'>load {{ load }}</span>"
```

Each uniform value is a template evaluated against the data (`true`/`false` →
1/0, otherwise parsed as a float). See
[`examples/shaders/reactive.glsl`](../examples/shaders/reactive.glsl) (calm teal
→ intense red as `u_load` rises) and
[`examples/shaders/aurora.glsl`](../examples/shaders/aurora.glsl).

## `<bg preset="…"/>` — the mild masked accent

Where `background_shader` is a full-bleed, opaque background, the `<bg>` markup
tag is the *subtle* counterpart: a **bundled preset** drawn as a faint
translucent layer, **clipped to the focus bubble** (the same rounded-rect as the
active card) with a **sharp 20px edge fade** — a graphical accent for *some*
tiles, not a show-off.

```jsonc
"format": "<bg preset='night'/><span size='xx-large' weight='bold'>{{ shortcut }}</span> …"
```

- `preset` — a bundled shader (`night` = deep-blue sky + drifting nebula +
  twinkling stars; `caustic` = night-blue water). Registered in `src/shader.rs`
  from [`shaders/`](../shaders).
- `alpha` — layer opacity (default `0.28`); `fade` — steep edge-cliff width in px
  (default `20`); `falloff` — the slow vignette radius in px (default = the
  bubble's short half-extent) stacked under the cliff, so the layer is brightest
  deep inside and gently fades toward the edges.
- Any other attribute becomes a uniform: a plain number → a `float` (e.g.
  `speed='0.4'`); a hex colour → three `name_r/g/b` floats.

`night` specifics: `alpha` controls only the **blue field** (gradient + nebula);
the **stars carry their own opacity** (`stars_alpha`, default `0.9`) so they stay
crisp while the field is faint. `stars="#rrggbb"` tints the stars (clamped,
defaults to cool blue-white). `stars_gain` (default `1`) lifts star brightness +
persistence: crank it with `stars_alpha="1"` and a warm `stars` colour to turn
the calm field into an actual **attention grab** — e.g.
`<bg preset='night' stars='#ffb84d' stars_alpha='1' stars_gain='2.6'/>`, pairing
nicely with `<pulse>` on a tile that wants your eye.

The mask mirrors the focus bubble exactly (one shared `focus_bubble()`), so it
lines up with the active-card border. Like any shader it repaints per frame
(capped at the anim fps), so reach for it on a *few* tiles, not all of them.
