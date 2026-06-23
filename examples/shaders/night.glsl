// Night drift — original shader for pwetty. A Catppuccin night sky: deep blue
// vertical gradient, a slow drifting nebula haze, and faint twinkling stars.
float hash(vec2 p){ p = fract(p * vec2(123.34, 456.21)); p += dot(p, p + 45.32); return fract(p.x * p.y); }
float noise(vec2 p){
    vec2 i = floor(p), f = fract(p); f = f * f * (3.0 - 2.0 * f);
    float a = hash(i), b = hash(i + vec2(1,0)), c = hash(i + vec2(0,1)), d = hash(i + vec2(1,1));
    return mix(mix(a,b,f.x), mix(c,d,f.x), f.y);
}
float fbm(vec2 p){ float v = 0.0, a = 0.5; for (int i = 0; i < 5; i++){ v += a * noise(p); p *= 2.0; a *= 0.5; } return v; }
void mainImage(out vec4 fragColor, in vec2 fragCoord){
    vec2 uv = fragCoord / iResolution.xy;
    float ar = iResolution.x / iResolution.y;
    vec2 p = vec2(uv.x * ar, uv.y);
    vec3 col = mix(vec3(0.02, 0.04, 0.10), vec3(0.05, 0.09, 0.21), uv.y);
    float n = fbm(p * 3.0 + vec2(iTime * 0.03, iTime * 0.015));
    col += vec3(0.10, 0.16, 0.34) * pow(n, 2.0) * 0.7;
    vec2 g = vec2(uv.x * ar, uv.y) * 64.0;
    vec2 gi = floor(g); float st = hash(gi);
    if (st > 0.93) {
        float d = length(fract(g) - 0.5);
        float tw = 0.5 + 0.5 * sin(iTime * 3.0 + st * 30.0);
        col += vec3(0.70, 0.80, 1.0) * smoothstep(0.45, 0.0, d) * tw * 0.9;
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
