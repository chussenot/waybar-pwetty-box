// Night drift — original shader for pwetty. A Catppuccin night sky: deep blue
// vertical gradient, a slow drifting nebula haze, and faint twinkling stars.
float hash(vec2 p){ p = fract(p * vec2(123.34, 456.21)); p += dot(p, p + 45.32); return fract(p.x * p.y); }
float noise(vec2 p){
    vec2 i = floor(p), f = fract(p); f = f * f * (3.0 - 2.0 * f);
    float a = hash(i), b = hash(i + vec2(1,0)), c = hash(i + vec2(0,1)), d = hash(i + vec2(1,1));
    return mix(mix(a,b,f.x), mix(c,d,f.x), f.y);
}
float fbm(vec2 p){ float v = 0.0, a = 0.5; for (int i = 0; i < 5; i++){ v += a * noise(p); p *= 2.0; a *= 0.5; } return v; }

// Star colour, settable via <bg preset="night" stars="#rrggbb"/> (the renderer
// expands a hex attr into stars_r/g/b). Used as a mild attention signal: warm
// stars say "look here", cool blue-white is the calm default.
uniform float stars_r; uniform float stars_g; uniform float stars_b;

// Invariants the star colour always satisfies, whatever uniforms arrive:
//   1. clamped to a valid [0,1] colour (no HDR blow-out, no negatives);
//   2. never black/invisible — an unset (all-zero) colour falls back to the
//      cool blue-white default, so stars always render;
//   3. only the STARS recolour; the gradient + nebula stay blue, so a tint
//      reads as an accent on the calm field, not a whole-tile colour shift.
vec3 star_color() {
    vec3 c = clamp(vec3(stars_r, stars_g, stars_b), 0.0, 1.0);
    return (c.r + c.g + c.b < 0.04) ? vec3(0.90, 0.95, 1.0) : c;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord){
    vec2 uv = fragCoord / iResolution.xy;
    float ar = iResolution.x / iResolution.y;
    vec2 p = vec2(uv.x * ar, uv.y);
    vec3 col = mix(vec3(0.02, 0.04, 0.10), vec3(0.05, 0.09, 0.21), uv.y);
    float n = fbm(p * 3.0 + vec2(iTime * 0.03, iTime * 0.015));
    col += vec3(0.10, 0.16, 0.34) * pow(n, 2.0) * 0.7;
    // Stars on a square cell grid. Each lit cell gets:
    //   - a per-star size in *device px* — most ~1px, some ~2px (a hash picks);
    //   - an independent twinkle (per-star rate + phase) with a WIDE amplitude
    //     so the blink is clearly visible, still peaking at full brightness;
    //   - scintillation: a per-star colour shimmering cool<->warm around the
    //     base star colour, so the field has subtle colour variety.
    float cells = 38.0;
    vec2 g = vec2(uv.x * ar, uv.y) * cells;
    vec2 gi = floor(g);
    float st = hash(gi);
    if (st > 0.92) {
        float cellpx = iResolution.y / cells;             // device px per cell
        float dpx = length(fract(g) - 0.5) * cellpx;      // px from star centre
        float rad = (hash(gi + 11.3) > 0.62) ? 1.0 : 0.5; // ~2px vs ~1px stars
        float dot = 1.0 - smoothstep(rad - 0.5, rad + 0.5, dpx);
        float ph  = st * 40.0;
        float spd = 0.5 + 1.6 * hash(gi + 3.7);           // some twinkle faster
        float tw  = 0.25 + 0.75 * (0.5 + 0.5 * sin(iTime * spd + ph)); // 0.25..1.0
        float warm = hash(gi + 5.1);                      // per-star hue bias
        vec3 chroma = mix(vec3(0.72, 0.84, 1.12), vec3(1.12, 0.96, 0.78), warm);
        vec3 sc = clamp(star_color() * chroma, 0.0, 1.0);
        sc *= 0.90 + 0.10 * sin(iTime * spd * 1.6 + ph * 1.3); // colour shimmer
        col += sc * dot * tw;
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
