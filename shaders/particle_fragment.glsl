#version 430
precision highp float;
// ═══════════════════════════════════════════════════════════════
//  Particle Fragment Shader — soft gradient mote with halo
//  Replaces per-particle draw_circle_gradient calls.
// ═══════════════════════════════════════════════════════════════

in vec2 fragUV;
in float fragAlpha;
in float fragMood;

out vec4 finalColor;

void main() {
    // Distance from center of the quad (0,0 is center, 1 is edge)
    vec2 centered = fragUV * 2.0 - 1.0;
    float dist = length(centered);

    if (dist > 1.0) discard;

    // Soft radial gradient — core bright, edge transparent
    float core = exp(-dist * dist * 3.0);
    float halo = exp(-dist * dist * 0.8) * 0.3;
    float intensity = core + halo;

    // Mood-driven color palette
    // Serene=teal, Alert=amber-teal, Stressed=orange, Critical=red-orange
    vec3 color;
    if (fragMood < 0.5) {
        color = vec3(0.08, 0.75, 0.71); // teal
    } else if (fragMood < 1.5) {
        color = mix(vec3(0.08, 0.75, 0.71), vec3(0.78, 0.71, 0.31), fragMood - 0.5);
    } else if (fragMood < 2.5) {
        color = vec3(0.86, 0.63, 0.27); // orange
    } else {
        color = vec3(1.0, 0.39, 0.20);  // red-orange
    }

    float a = intensity * fragAlpha;
    if (a < 0.005) discard;

    finalColor = vec4(color * intensity, a);
}
