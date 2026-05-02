#version 330
precision highp float;
// ═══════════════════════════════════════════════════════════════
//  Filmic Tone Mapping — Uncharted 2 style
//  Maps HDR glow buffer to LDR for display.
//  Includes exposure control, filmic operator, and gamma correction.
// ═══════════════════════════════════════════════════════════════

in vec2 fragTexCoord;
out vec4 finalColor;

uniform sampler2D texture0;  // scene (main RT)
uniform sampler2D texture1;  // bloom (accumulated glow)
uniform float u_bloom_strength;
uniform float u_exposure;

// Uncharted 2 tone mapping curve
vec3 uncharted2_tonemap(vec3 x) {
    float A = 0.15;
    float B = 0.50;
    float C = 0.10;
    float D = 0.20;
    float E = 0.02;
    float F = 0.30;
    return ((x*(A*x+C*B)+D*E)/(x*(A*x+B)+D*F))-E/F;
}

void main() {
    vec3 scene = texture(texture0, fragTexCoord).rgb;
    vec3 bloom = texture(texture1, fragTexCoord).rgb;

    // Combine scene + bloom
    vec3 hdr = scene + bloom * u_bloom_strength;

    // Apply exposure
    hdr *= u_exposure;

    // Tone map
    float W = 11.2; // linear white point
    vec3 curr = uncharted2_tonemap(hdr);
    vec3 white_scale = 1.0 / uncharted2_tonemap(vec3(W));
    vec3 mapped = curr * white_scale;

    // Gamma correction
    mapped = pow(mapped, vec3(1.0 / 2.2));

    finalColor = vec4(mapped, 1.0);
}
