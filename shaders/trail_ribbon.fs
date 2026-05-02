#version 330
precision highp float;
// ═══════════════════════════════════════════════════════════════
//  Trail Ribbon Fragment Shader — continuous phosphor wake
//  Replaces 48 individual draw_circle_gradient calls with a
//  single triangle-strip ribbon textured with flowing energy.
// ═══════════════════════════════════════════════════════════════

in float fLife;      // 0 = newest, 1 = oldest
in float fSpeed;     // orb speed at this point (0-1 normalized)
in float fWidth;     // perpendicular distance from spine (-1 to 1)

out vec4 finalColor;

uniform float u_time;
uniform vec3 u_color_teal;
uniform vec3 u_color_icy;

// Simple 1D hash noise for energy flow
float hash(float n) { return fract(sin(n) * 43758.5453123); }

void main() {
    // Quadratic alpha fade over lifetime
    float life_alpha = (1.0 - fLife) * (1.0 - fLife);

    // Speed factor — no trail when resting
    float speed_alpha = clamp(fSpeed, 0.0, 1.0);

    // Edge fade — ribbon thins at edges
    float edge = 1.0 - abs(fWidth);
    edge = edge * edge;

    float alpha = life_alpha * speed_alpha * edge;
    if (alpha < 0.01) discard;

    // Flowing energy texture — procedural noise scrolling backward
    float flow = hash(fLife * 20.0 + u_time * 2.0) * 0.3;
    float energy = smoothstep(0.0, 0.5, 1.0 - fLife) + flow;

    // Color: blend teal core → icy halo based on edge distance
    vec3 color = mix(u_color_teal, u_color_icy, abs(fWidth) * 0.6);
    color *= energy;

    finalColor = vec4(color, alpha * 0.47); // match original 120/255 alpha
}
