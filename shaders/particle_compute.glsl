#version 430
// ═══════════════════════════════════════════════════════════════
//  Particle Compute Shader — GPU-driven mote simulation
//  Handles gravity, turbulence, sinusoidal drift, alpha envelope,
//  and lifetime management for up to 2M+ particles.
// ═══════════════════════════════════════════════════════════════
layout(local_size_x = 256) in;

// ── SoA Particle Data (SSBOs) ─────────────────────────────────
layout(std430, binding = 0) buffer PosX   { float pos_x[]; };
layout(std430, binding = 1) buffer PosY   { float pos_y[]; };
layout(std430, binding = 2) buffer VelX   { float vel_x[]; };
layout(std430, binding = 3) buffer VelY   { float vel_y[]; };
layout(std430, binding = 4) buffer Life   { float life[];  };
layout(std430, binding = 5) buffer MaxLife{ float max_life[]; };
layout(std430, binding = 6) buffer Size   { float psize[]; };
layout(std430, binding = 7) buffer Seed   { float seed[];  };
layout(std430, binding = 8) buffer Alpha  { float alpha[]; };

// ── Uniforms ──────────────────────────────────────────────────
uniform float u_dt;
uniform float u_time;
uniform float u_orb_x;
uniform float u_orb_y;
uniform float u_emit_rate;
uniform float u_speed_base;
uniform float u_turbulence;
uniform float u_drift;
uniform float u_max_life;
uniform float u_orb_radius;
uniform uint  u_particle_count;
uniform uint  u_spawn_offset;    // ring-buffer write offset for spawning

// ── GPU Noise ─────────────────────────────────────────────────
float hash(float n) { return fract(sin(n) * 43758.5453123); }

float noise1d(float p) {
    float fl = floor(p);
    float fc = fract(p);
    return mix(hash(fl), hash(fl + 1.0), fc);
}

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= u_particle_count) return;

    float l = life[idx];

    // ── Dead particle: skip physics ───────────────────────────
    if (l <= 0.0) {
        alpha[idx] = 0.0;
        return;
    }

    float dt = u_dt;
    float t  = u_time;
    float s  = seed[idx];

    // ── Sinusoidal drift (unique phase per particle) ──────────
    float phase = s * 6.283185 + t * (1.5 + s * 2.0);
    vel_x[idx] += sin(phase) * u_drift * dt * 60.0;
    vel_y[idx] += cos(phase) * u_drift * dt * 60.0;

    // ── Anti-gravity float ────────────────────────────────────
    vel_y[idx] -= 6.0 * dt;

    // ── Turbulence ────────────────────────────────────────────
    float turb_phase = s * 12.0 + t * 3.5;
    vel_x[idx] += cos(turb_phase) * u_turbulence * dt * 40.0;
    vel_y[idx] += sin(turb_phase) * u_turbulence * dt * 40.0;

    // ── Drag ──────────────────────────────────────────────────
    float drag = 1.0 - 1.5 * dt;
    vel_x[idx] *= drag;
    vel_y[idx] *= drag;

    // ── Integrate position ────────────────────────────────────
    pos_x[idx] += vel_x[idx] * dt;
    pos_y[idx] += vel_y[idx] * dt;

    // ── Lifetime decay ────────────────────────────────────────
    life[idx] -= dt;

    // ── Alpha envelope: fast fade-in (25%), slow fade-out ─────
    float life_frac = max(life[idx] / max_life[idx], 0.0);
    float fade_in  = min((1.0 - life_frac) * 4.0, 1.0);
    float fade_out = life_frac;
    alpha[idx] = fade_in * fade_out;

    // ── Size decay ────────────────────────────────────────────
    psize[idx] *= (0.5 + life_frac * 0.5);
}
