#version 330
precision highp float;

// ═══════════════════════════════════════════════════════════════
//  Orb SDF Fragment Shader
//  Volumetric Raymarching with Signed Distance Fields, FBM noise,
//  subsurface scattering via Beer-Lambert law, and soft shadows.
// ═══════════════════════════════════════════════════════════════

in vec2 fragTexCoord;
out vec4 finalColor;

// ── Uniforms ──────────────────────────────────────────────────
uniform vec2 u_resolution;      // screen size in pixels
uniform vec2 u_orb_pos;         // orb center in screen pixels
uniform float u_time;           // elapsed time in seconds
uniform float u_radius;         // base orb radius in pixels
uniform float u_cpu;            // CPU load 0–100
uniform float u_mood;           // 0=Serene, 1=Alert, 2=Stressed, 3=Critical
uniform float u_temperature;    // weather temperature in Celsius
uniform float u_weather_flags;  // packed weather bits

// ── Constants ─────────────────────────────────────────────────
const int MAX_STEPS = 64;
const float MIN_DIST = 0.5;
const float MAX_DIST = 800.0;
const float EPSILON = 0.5;
const float PI = 3.14159265359;

// ── Simplex Noise 2D (GPU implementation) ─────────────────────
// Compact GLSL simplex noise from Ashima Arts (MIT license)
vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec2 mod289(vec2 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec3 permute(vec3 x) { return mod289(((x * 34.0) + 1.0) * x); }

float snoise(vec2 v) {
    const vec4 C = vec4(0.211324865405, 0.366025403784,
                        -0.577350269189, 0.024390243902);
    vec2 i  = floor(v + dot(v, C.yy));
    vec2 x0 = v - i + dot(i, C.xx);
    vec2 i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
    vec4 x12 = x0.xyxy + C.xxzz;
    x12.xy -= i1;
    i = mod289(i);
    vec3 p = permute(permute(i.y + vec3(0.0, i1.y, 1.0))
                             + i.x + vec3(0.0, i1.x, 1.0));
    vec3 m = max(0.5 - vec3(dot(x0, x0), dot(x12.xy, x12.xy),
                            dot(x12.zw, x12.zw)), 0.0);
    m = m * m;
    m = m * m;
    vec3 x = 2.0 * fract(p * C.www) - 1.0;
    vec3 h = abs(x) - 0.5;
    vec3 ox = floor(x + 0.5);
    vec3 a0 = x - ox;
    m *= 1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h);
    vec3 g;
    g.x = a0.x * x0.x + h.x * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;
    return 130.0 * dot(m, g);
}

// ── Fractal Brownian Motion ───────────────────────────────────
float fbm(vec2 p, int octaves) {
    float sum = 0.0;
    float amp = 0.5;
    float freq = 1.0;
    for (int i = 0; i < 6; i++) {
        if (i >= octaves) break;
        sum += snoise(p * freq) * amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    return sum;
}

// ── Signed Distance Functions ─────────────────────────────────

// Sphere SDF: f(p) = |p - c| - r
float sdSphere(vec2 p, vec2 center, float radius) {
    return length(p - center) - radius;
}

// Smooth minimum (polynomial) for organic merging
// smin(a, b, k) blends two SDFs within distance k
float smin(float a, float b, float k) {
    float h = max(k - abs(a - b), 0.0);
    return min(a, b) - h * h / (k * 4.0);
}

// Capsule SDF: distance to the line segment a→b with thickness `th`.
// Used for the jellyfish's hanging tendrils.
float sdCapsule(vec2 p, vec2 a, vec2 b, float th) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-4), 0.0, 1.0);
    return length(pa - ba * h) - th;
}

// ── Main SDF Scene ────────────────────────────────────────────
// Returns distance to nearest surface at point p.
float sceneSDF(vec2 p) {
    vec2 center = u_orb_pos;
    float r = u_radius;

    // Breathing oscillation (replaces CPU-side breath calculation)
    float breath = sin(u_time * 0.3 * 2.0 * PI) * 0.09 + 1.0;
    r *= breath;

    // ── Bubble surface-tension wobble ─────────────────────────
    // Three angular harmonics produce a soft, fluid radius modulation
    // that makes the orb breathe like a soap bubble rather than a rigid
    // sphere. Mode 2/3/5 give a pleasant non-uniform, non-repeating shape.
    vec2 toC = p - center;
    float ang = atan(toC.y, toC.x);
    float wobble = sin(ang * 2.0 + u_time * 0.62) * 0.028
                 + sin(ang * 3.0 - u_time * 0.44) * 0.018
                 + sin(ang * 5.0 + u_time * 0.92) * 0.012;
    r *= 1.0 + wobble;

    // ── Jellyfish bell shaping ───────────────────────
    vec2 q = toC / max(r, 1.0);
    float lower = smoothstep(0.0, 0.95, q.y);   // 0 on top, 1 at bottom
    float upper = smoothstep(0.0, 0.95, -q.y);  // 0 on bottom, 1 at top
    // Strong dome compression on top — a vertical squash so the crown is
    // clearly hemispherical, not round.
    r *= 1.0 - upper * 0.22;
    // Strong bell flare on the bottom — unmistakable medusa skirt.
    float ruffle = sin(ang * 7.0 + u_time * 1.6) * 0.5
                 + sin(ang * 11.0 - u_time * 2.3) * 0.3;
    float flare = lower * (0.55 + 0.12 * sin(u_time * 1.1));
    r *= 1.0 + flare;
    r *= 1.0 + lower * ruffle * 0.09;

    // ── Edge deformation (the "bubble against glass" effect) ──
    // When the orb approaches a screen edge, we bend its silhouette by
    // perturbing the SAMPLED RADIUS as a function of angle relative to
    // the nearest edge. This produces visible flattening on the side that
    // touches the edge while preserving the orb's overall area, so it
    // never shrinks to invisibility near a corner.
    vec2 edge_dist = vec2(
        min(u_orb_pos.x, u_resolution.x - u_orb_pos.x),
        min(u_orb_pos.y, u_resolution.y - u_orb_pos.y)
    );
    // Influence ramps in only when the orb is within ~1.4× its radius of
    // an edge, and saturates softly. Strength is capped to 0.22 so the
    // shape stays recognisable even pinned in a corner.
    float infl_x = smoothstep(r * 1.4, r * 0.4, edge_dist.x);
    float infl_y = smoothstep(r * 1.4, r * 0.4, edge_dist.y);
    vec2 edge_dir = vec2(
        u_orb_pos.x < u_resolution.x * 0.5 ? -1.0 : 1.0,
        u_orb_pos.y < u_resolution.y * 0.5 ? -1.0 : 1.0
    );
    vec2 push = edge_dir * vec2(infl_x, infl_y);
    float push_mag = length(push);
    if (push_mag > 0.01) {
        vec2 outward = push / push_mag;
        // Cosine of the angle between the fragment direction and the
        // outward (into-the-edge) direction. +1 on the squashed side,
        // −1 on the bulging side.
        float toC_len = max(length(toC), 1.0);
        float cos_a = dot(toC / toC_len, outward);
        float strength = clamp(push_mag, 0.0, 1.0) * 0.22;
        // Push the surface IN on the squashed side, and slightly OUT on
        // the opposite side — preserves area, mimics surface tension
        // flattening against a barrier.
        r *= 1.0 - strength * cos_a;
    }

    vec2 p_w = p;

    // FBM surface perturbation — three frequency layers matching
    // the original 0.85 Hz, 0.55 Hz, 1.3 Hz oscillators
    float noise_scale = 0.008;
    float perturbation = 0.0;
    perturbation += fbm(p_w * noise_scale + vec2(u_time * 0.85, 0.0), 4) * 6.0;
    perturbation += fbm(p_w * noise_scale * 0.7 + vec2(0.0, u_time * 0.55 + 1.5), 3) * 4.0;
    perturbation += fbm(p_w * noise_scale * 1.3 + vec2(u_time * 1.3 + 3.0, u_time * 0.3), 3) * 2.5;

    // Mood-scaled perturbation intensity
    float mood_intensity = mix(0.45, 1.0, u_mood / 3.0);
    perturbation *= mood_intensity;

    float main_orb = sdSphere(p_w, center, r) + perturbation;

    // Sub-orbs: small orbiting blobs that merge into the main orb via smin
    float sub1_angle = u_time * 0.62 + 0.7;
    vec2 sub1_pos = center + vec2(cos(sub1_angle) * r * 0.16, abs(sin(sub1_angle)) * r * 0.24);
    float sub1 = sdSphere(p_w, sub1_pos, r * 0.08);

    float sub2_angle = u_time * -0.54 + 2.2;
    vec2 sub2_pos = center + vec2(cos(sub2_angle) * r * 0.18, abs(sin(sub2_angle)) * r * 0.28);
    float sub2 = sdSphere(p_w, sub2_pos, r * 0.07);

    vec2 sub3_pos = center + vec2(0.0, -r * 0.12);
    float sub3 = sdSphere(p_w, sub3_pos, r * 0.10);

    // Merge sub-orbs into main via smooth minimum
    float k = r * 0.20; // blending radius
    float d = smin(main_orb, sub1, k);
    d = smin(d, sub2, k);
    d = smin(d, sub3, k);

    // ── Hanging tendrils ───────────────────────────
    // Seven thick capsules hanging below the bell, swaying horizontally.
    vec2 skirt_anchor = center + vec2(0.0, r * 0.55);
    float tendril_d = 1e6;
    for (int ti = 0; ti < 7; ti++) {
        float fi = float(ti);
        float u = (fi - 3.0) / 3.0;                    // -1..+1 across skirt
        float ax = u * r * 0.78;
        float ay_off = (1.0 - abs(u)) * r * 0.10;
        vec2 a = center + vec2(ax, r * 0.55 + ay_off);
        float len = r * (1.85 - abs(u) * 0.55);
        float sway1 = sin(u_time * 1.05 + fi * 1.7) * r * 0.20;
        float sway2 = sin(u_time * 1.85 + fi * 0.9) * r * 0.09;
        vec2 b = a + vec2(sway1 + sway2, len);
        // Thicker tendrils so they read at small orb sizes.
        float th = r * (0.10 - abs(u) * 0.025);
        float td = sdCapsule(p_w, a, b, th);
        tendril_d = min(tendril_d, td);
    }
    d = smin(d, tendril_d, r * 0.20);

    return d;
}

// ── Surface Normal via Gradient Estimation ────────────────────
vec2 estimateNormal(vec2 p) {
    float dx = sceneSDF(vec2(p.x + EPSILON, p.y)) - sceneSDF(vec2(p.x - EPSILON, p.y));
    float dy = sceneSDF(vec2(p.x, p.y + EPSILON)) - sceneSDF(vec2(p.x, p.y - EPSILON));
    return normalize(vec2(dx, dy));
}

// ── Soft Shadow (2D approximation) ────────────────────────────
float softShadow(vec2 ro, vec2 rd, float maxDist, float k_shadow) {
    float res = 1.0;
    float t = 2.0;
    for (int i = 0; i < 24; i++) {
        vec2 p = ro + rd * t;
        float d = sceneSDF(p);
        if (d < MIN_DIST) return 0.0;
        res = min(res, k_shadow * d / t);
        t += max(d, 1.0);
        if (t > maxDist) break;
    }
    return clamp(res, 0.0, 1.0);
}

// ── Color Mapping ─────────────────────────────────────────────
// Maps CPU load and mood to the orb's core color palette.

vec3 coreColor(float depth) {
    // Base palette from CPU load (matching original logic)
    vec3 low  = vec3(0.24, 0.86, 0.55);  // green/teal (cpu < 30%)
    vec3 mid  = vec3(0.47, 0.31, 0.78);  // violet (30-60%)
    vec3 high = vec3(1.0, 0.55, 0.24);   // orange-red (>60%)

    float cpu_norm = u_cpu / 100.0;
    vec3 base;
    if (cpu_norm < 0.3) {
        base = mix(low, mid, cpu_norm / 0.3);
    } else if (cpu_norm < 0.6) {
        base = mix(mid, high, (cpu_norm - 0.3) / 0.3);
    } else {
        base = mix(high, vec3(1.0, 0.3, 0.1), min((cpu_norm - 0.6) / 0.4, 1.0));
    }

    // Weather palette injection (subsurface effect)
    // Temperature modulates hue: cold → icy blue, hot → warm amber
    float temp_norm = clamp((u_temperature + 20.0) / 60.0, 0.0, 1.0);
    vec3 cold_tint = vec3(0.55, 0.85, 0.95);  // icy
    vec3 warm_tint = vec3(0.95, 0.75, 0.45);  // amber
    vec3 weather_tint = mix(cold_tint, warm_tint, temp_norm);

    // Beer-Lambert subsurface scattering approximation:
    // Light absorption increases with depth inside the volume.
    // sigma_t = absorption coefficient (higher = more opaque core)
    float sigma_t = 0.025 + u_mood * 0.008;
    float transmittance = exp(-sigma_t * max(depth, 0.0));

    // Surface: weather-tinted; core: base CPU color (dense, vibrant)
    vec3 color = mix(base, weather_tint, transmittance * 0.4);

    return color;
}

// ── Raymarching ───────────────────────────────────────────────
void main() {
    // Convert fragment coordinate to screen-space pixels
    vec2 fragPixel = fragTexCoord * u_resolution;

    // Early discard: skip fragments far from the orb. Tendrils extend
    // ~1.8× radius below the bell, so use a larger bound for the lower
    // hemisphere.
    float dist_to_orb = length(fragPixel - u_orb_pos);
    float vy = (fragPixel.y - u_orb_pos.y) / max(u_radius, 1.0);
    float max_render_dist = u_radius * (vy > 0.0 ? 4.5 : 2.8);
    if (dist_to_orb > max_render_dist) {
        finalColor = vec4(0.0, 0.0, 0.0, 0.0);
        return;
    }

    // Raymarch origin is the fragment position
    vec2 ro = fragPixel;
    // In 2D SDF, we don't march along a ray — we evaluate the SDF directly
    // and use the distance for volumetric effects.
    float d = sceneSDF(ro);

    // Volumetric rendering: accumulate color based on SDF distance
    // Inside the volume (d < 0): full density
    // Near the surface (d ~ 0): edge glow
    // Outside (d > 0): exponential falloff for atmospheric glow

    float alpha = 0.0;
    vec3 color = vec3(0.0);

    vec2 halo_vec = fragPixel - u_orb_pos;
    float halo_len = max(length(halo_vec), 1.0);
    vec2 halo_dir = halo_vec / halo_len;
    float halo_ang = atan(halo_dir.y, halo_dir.x);
    float jelly_wave = 0.86
                     + 0.11 * sin(halo_ang * 4.0 - u_time * 1.0 + halo_len / (u_radius + 1.0) * 2.2)
                     + 0.05 * sin(halo_ang * 7.0 + u_time * 1.7);
    jelly_wave = clamp(jelly_wave, 0.72, 1.08);

    // Lower hemisphere veil gives a soft "space jellyfish" motion curtain.
    float lower_hemi = smoothstep(0.08, 0.96, halo_dir.y * 0.5 + 0.5);
    float veil_band = sin(halo_ang * 10.0 + u_time * 1.45 + halo_len * 0.065) * 0.5 + 0.5;
    float jelly_veil = lower_hemi * pow(veil_band, 2.6);
    float tendril_band = sin(halo_ang * 13.0 - u_time * 2.1 + halo_len * 0.09) * 0.5 + 0.5;
    float tendrils = lower_hemi * pow(tendril_band, 5.0);

    if (d < 0.0) {
        // ── Inside the orb volume ─────────────────────────────
        float depth = -d; // how far inside

        // Core density increases toward center
        float density = smoothstep(0.0, u_radius * 0.8, depth);
        // Solid bubble body — fully opaque core, slightly translucent skin
        alpha = mix(0.92, 1.0, density);

        // Surface normal for lighting
        vec2 normal = estimateNormal(ro);

        // Simulated directional light (upper-left)
        vec2 lightDir = normalize(vec2(-0.5, -0.7));
        float ndotl = max(dot(normal, -lightDir), 0.0);

        // Fresnel-like rim lighting (brighter at edges)
        float rim = 1.0 - density;
        rim = pow(rim, 2.0) * 1.5;

        // Get base color with Beer-Lambert subsurface
        color = coreColor(depth);

        // ── Bioluminescent palette injection ─────────────────
        // Slow internal plasma FBM gives the body a living, shimmering
        // organelle structure rather than a flat tinted sphere.
        vec2 plasma_uv = (ro - u_orb_pos) * 0.013;
        float plasma = fbm(plasma_uv + vec2(u_time * 0.18, -u_time * 0.11), 4);
        plasma = plasma * 0.5 + 0.5;
        float plasma2 = fbm(plasma_uv * 1.7 + vec2(-u_time * 0.07, u_time * 0.23), 3);
        plasma2 = plasma2 * 0.5 + 0.5;
        // Bioluminescent palette — abyssal aqua → electric cyan → icy white
        vec3 bio_deep    = vec3(0.10, 0.55, 0.78);
        vec3 bio_cyan    = vec3(0.45, 1.00, 1.20);
        vec3 bio_glow    = vec3(0.92, 1.15, 1.20);
        vec3 bio = mix(bio_deep, bio_cyan, smoothstep(0.20, 0.85, plasma));
        bio = mix(bio, bio_glow, smoothstep(0.55, 1.0, plasma * plasma2));
        // Bio palette dominates the body (96%) so the orb always reads as
        // a luminous bioluminescent creature regardless of CPU/weather.
        color = mix(color, bio, 0.96);

        // Apply lighting — ambient holds the body luminous; rim gives a
        // subtle edge accent without overwhelming the bell silhouette.
        float ambient = 1.20;
        float diffuse = ndotl * 0.30;
        float rim_w   = rim * 0.85;
        color *= (ambient + diffuse + rim_w);

        // Pulsing heart — modest so the bell shape isn't blown out.
        float center_glow = exp(-depth * depth / (u_radius * u_radius * 0.30));
        float heart_pulse = 0.85 + 0.25 * sin(u_time * 1.6 + plasma * 6.2831);
        color += bio_glow * center_glow * 1.40 * heart_pulse;
        // Shimmering internal cells — visible as faint moving brightness
        // pockets through the volume.
        float cell_shimmer = smoothstep(0.55, 0.95, plasma2) * (1.0 - smoothstep(u_radius * 0.45, u_radius * 0.95, depth));
        color += bio_cyan * cell_shimmer * 0.55;

        // Bubble specular sheen — a tight bright spot off-axis from the
        // light direction, plus a faint secondary highlight. Strongest
        // near the surface (low density), so it reads as a film highlight
        // rather than an interior glow.
        float spec = max(dot(normal, -lightDir), 0.0);
        float spec_tight = pow(spec, 28.0);
        float spec_soft  = pow(spec, 6.0) * 0.25;
        float surface_weight = pow(1.0 - density, 1.5);
        color += vec3(1.0, 1.0, 1.0) * (spec_tight + spec_soft) * surface_weight * 0.85;

        // Soft shadow from sub-orbs
        float shadow = softShadow(ro, lightDir, u_radius * 1.5, 8.0);
        color *= mix(0.7, 1.0, shadow);

    } else if (d < u_radius * 0.55) {
        // ── Tight bioluminescent rim glow ────────────────
        // Compact rim hugging the body so the silhouette reads cleanly.
        float glow_tight = exp(-d * d / (u_radius * u_radius * 0.045));
        float glow      = exp(-d * d / (u_radius * u_radius * 0.13));
        alpha  = glow_tight * 0.85 * jelly_wave;
        alpha += glow * 0.22 * jelly_wave;
        alpha += glow * jelly_veil * 0.18;
        alpha += glow * tendrils  * 0.16;

        vec3 bio_inner = vec3(0.65, 1.00, 1.10);
        vec3 bio_mid   = vec3(0.20, 0.78, 0.98);
        vec3 bio_outer = vec3(0.05, 0.22, 0.50);
        vec3 tendril_col = vec3(0.20, 0.55, 1.00);
        color = mix(bio_outer, bio_mid, glow);
        color = mix(color, bio_inner, glow_tight);
        color = mix(color, bio_inner, jelly_veil * glow * 0.45);
        color += tendril_col * tendrils * glow * 0.55;

    } else {
        // No far atmosphere — keeps the silhouette crisp; the bloom pass
        // adds the soft outer halo.
        alpha = 0.0;
        color = vec3(0.0);
    }

    // Pulsing emission (thought pulse hook — driven by mood)
    float pulse = sin(u_time * 2.0) * 0.5 + 0.5;
    float mood_pulse = mix(0.0, 0.15, u_mood / 3.0) * pulse;
    color += color * mood_pulse;

    // Gamma correction
    color = pow(color, vec3(1.0 / 2.2));

    finalColor = vec4(color, alpha);
}
