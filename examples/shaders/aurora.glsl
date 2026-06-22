// Tile background shader — Shadertoy-style: define mainImage(out, in).
// Uniforms provided: iResolution (vec3), iTime (float), iFrame (int).
// A flowing cosine-palette plasma with a gentle moving sheen.
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float t = iTime;

    vec3 col = 0.5 + 0.5 * cos(t + uv.xyx * vec3(3.0, 4.0, 5.0) + vec3(0.0, 2.0, 4.0));

    // diagonal sheen sweeping across over time
    float sweep = sin((uv.x + uv.y) * 6.0 - t * 1.5);
    col += 0.08 * sweep;

    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
