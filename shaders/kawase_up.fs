#version 330
precision highp float;
// ═══════════════════════════════════════════════════════════════
//  Dual Kawase Upsample — Phase 6 Bloom Pipeline
//  Samples 8 points (diamond pattern) for smooth tent-filter
//  upscale accumulation. Energy-conserving weights.
// ═══════════════════════════════════════════════════════════════

in vec2 fragTexCoord;
out vec4 finalColor;

uniform sampler2D texture0;
uniform vec2 u_half_pixel;  // 0.5 / source_resolution

void main() {
    vec2 uv = fragTexCoord;

    vec4 sum = vec4(0.0);

    // Four diagonal samples — weight 2 each (8 total weight from these)
    sum += texture(texture0, uv + vec2(-u_half_pixel.x * 2.0, 0.0));
    sum += texture(texture0, uv + vec2( u_half_pixel.x * 2.0, 0.0));
    sum += texture(texture0, uv + vec2(0.0, -u_half_pixel.y * 2.0));
    sum += texture(texture0, uv + vec2(0.0,  u_half_pixel.y * 2.0));

    // Four corner samples — weight 1 each (4 total weight from these)
    sum += texture(texture0, uv + vec2(-u_half_pixel.x, -u_half_pixel.y)) * 2.0;
    sum += texture(texture0, uv + vec2( u_half_pixel.x, -u_half_pixel.y)) * 2.0;
    sum += texture(texture0, uv + vec2(-u_half_pixel.x,  u_half_pixel.y)) * 2.0;
    sum += texture(texture0, uv + vec2( u_half_pixel.x,  u_half_pixel.y)) * 2.0;

    finalColor = sum / 12.0;
}
