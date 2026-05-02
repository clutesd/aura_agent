#version 330
precision highp float;
// ═══════════════════════════════════════════════════════════════
//  Dual Kawase Downsample — Phase 6 Bloom Pipeline
//  Samples 5 points in a diagonal cross pattern for a high-quality
//  blur with minimal texture fetches (half as many as Gaussian).
// ═══════════════════════════════════════════════════════════════

in vec2 fragTexCoord;
out vec4 finalColor;

uniform sampler2D texture0;
uniform vec2 u_half_pixel;  // 0.5 / source_resolution

void main() {
    vec2 uv = fragTexCoord;

    vec4 sum = texture(texture0, uv) * 4.0;
    sum += texture(texture0, uv - u_half_pixel);
    sum += texture(texture0, uv + u_half_pixel);
    sum += texture(texture0, uv + vec2(u_half_pixel.x, -u_half_pixel.y));
    sum += texture(texture0, uv - vec2(u_half_pixel.x, -u_half_pixel.y));

    finalColor = sum / 8.0;
}
