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
    // Stars: fewer, larger cells -> bigger multi-pixel dots; near-max brightness
    // with only a gentle, slow twinkle (so they read as bright points, not noise).
    vec2 g = vec2(uv.x * ar, uv.y) * 38.0;
    vec2 gi = floor(g); float st = hash(gi);
    if (st > 0.93) {
        float d = length(fract(g) - 0.5);
        float tw = 0.72 + 0.28 * sin(iTime * 0.8 + st * 30.0);
        float dot = smoothstep(0.42, 0.06, d);
        col += star_color() * dot * tw;
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
