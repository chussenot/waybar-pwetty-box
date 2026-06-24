// Caustic water — adapted for pwetty (GLES3 mainImage) from "Tileable Water
// Caustic" by Dave Hoskins, Shadertoy MdlXz8 (CC BY-NC-SA 3.0). Recoloured to a
// Catppuccin night-blue palette: a dark base with cool blue caustic glints.
#define TAU 6.28318530718
#define ITER 5
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    float time = iTime * 0.5 + 23.0;
    vec2 uv = fragCoord / iResolution.xy;
    vec2 p = mod(uv * TAU * 1.6, TAU) - 250.0;
    vec2 i = p;
    float c = 1.0;
    float inten = 0.005;
    for (int n = 0; n < ITER; n++) {
        float t = time * (1.0 - (3.5 / float(n + 1)));
        i = p + vec2(cos(t - i.x) + sin(t + i.y), sin(t - i.y) + cos(t + i.x));
        c += 1.0 / length(vec2(p.x / (sin(i.x + t) / inten), p.y / (cos(i.y + t) / inten)));
    }
    c /= float(ITER);
    c = 1.17 - pow(c, 1.4);
    float k = pow(abs(c), 8.0);
    vec3 base = vec3(0.035, 0.075, 0.17);   // night-blue water
    vec3 glow = vec3(0.34, 0.49, 0.92);     // Catppuccin blue caustic
    vec3 col = base + glow * k;
    // u_alpha (declared by the masked wrapper) is this layer's overall opacity.
    fragColor = vec4(clamp(col, 0.0, 1.0), u_alpha);
}
