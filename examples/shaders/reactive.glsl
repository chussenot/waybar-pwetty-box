// Data-reactive background shader.
// Custom uniform `u_load` (0..1) is fed from tile data via `shader_uniforms`,
// e.g.  "shader_uniforms": { "u_load": "{{ (value | float) / 8.0 }}" }.
// Calm slow teal at low load → fast intense red as load rises.
uniform float u_load;

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float load = clamp(u_load, 0.0, 1.0);

    // animate faster under load
    float t = iTime * (0.3 + load * 2.5);

    // hue: teal (calm) → red (hot)
    vec3 calm = vec3(0.10, 0.62, 0.55);
    vec3 hot  = vec3(0.96, 0.28, 0.32);
    vec3 base = mix(calm, hot, load);

    // travelling waves whose amplitude grows with load
    float wave = sin(uv.x * 9.0 + t * 3.0) * (0.18 + load * 0.5)
               + sin(uv.y * 5.0 - t * 1.7) * 0.12;
    float glow = 0.55 + 0.45 * wave;

    fragColor = vec4(clamp(base * glow, 0.0, 1.0), 1.0);
}
