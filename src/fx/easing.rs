// ═══════════════════════════════════════════════════════════════
//  Easing primitives — small, allocation-free, inline-friendly.
//  Used to swap linear interpolation and pow-based drag for
//  organic, frame-rate-independent motion across the codebase.
// ═══════════════════════════════════════════════════════════════

#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

#[inline]
pub fn ease_in_out_sine(t: f32) -> f32 {
    0.5 - 0.5 * (std::f32::consts::PI * t.clamp(0.0, 1.0)).cos()
}

#[inline]
#[allow(dead_code)]
pub fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158_f32;
    let c3 = c1 + 1.0;
    let u = t.clamp(0.0, 1.0) - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}

#[inline]
#[allow(dead_code)]
pub fn ease_out_expo(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 {
        1.0
    } else {
        1.0 - 2f32.powf(-10.0 * t)
    }
}

/// Frame-rate independent exponential smoothing — a drop-in replacement
/// for the `(k).powf(dt)` drag pattern. `half_life` is the time in seconds
/// for the value to close half the gap to `target`.
#[inline]
pub fn damp(current: f32, target: f32, half_life: f32, dt: f32) -> f32 {
    let k = 1.0 - 0.5_f32.powf(dt / half_life.max(1e-4));
    current + (target - current) * k
}
