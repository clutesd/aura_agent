#version 430
// ═══════════════════════════════════════════════════════════════
//  Particle Vertex Shader — GPU Instanced rendering
//  Each instance reads its position/size/alpha from SSBO via
//  gl_InstanceID and applies billboard transformation.
// ═══════════════════════════════════════════════════════════════

// Per-vertex attributes (single unit quad: 4 vertices)
in vec3 vertexPosition;
in vec2 vertexTexCoord;

// SSBOs bound identically to the compute shader
layout(std430, binding = 0) buffer PosX  { float pos_x[]; };
layout(std430, binding = 1) buffer PosY  { float pos_y[]; };
layout(std430, binding = 6) buffer Size  { float psize[]; };
layout(std430, binding = 8) buffer Alpha { float alpha[]; };

uniform vec2 u_resolution;
uniform float u_mood;  // 0=Serene, 1=Alert, 2=Stressed, 3=Critical

out vec2 fragUV;
out float fragAlpha;
out float fragMood;

void main() {
    int id = gl_InstanceID;

    float a = alpha[id];
    if (a < 0.005) {
        // Degenerate triangle — GPU skips rasterization
        gl_Position = vec4(0.0, 0.0, 0.0, 0.0);
        fragAlpha = 0.0;
        return;
    }

    float px = pos_x[id];
    float py = pos_y[id];
    float sz = psize[id];

    // Billboard: scale the unit quad by particle size
    vec2 screen_pos = vec2(px, py);
    vec2 offset = vertexPosition.xy * sz;
    vec2 final_pos = screen_pos + offset;

    // Convert from screen pixels to NDC [-1, 1]
    vec2 ndc = (final_pos / u_resolution) * 2.0 - 1.0;
    ndc.y = -ndc.y; // flip Y for OpenGL

    gl_Position = vec4(ndc, 0.0, 1.0);
    fragUV = vertexTexCoord;
    fragAlpha = a;
    fragMood = u_mood;
}
