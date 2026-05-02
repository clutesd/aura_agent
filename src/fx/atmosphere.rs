// ══════════════════════════════════════════════════════════════════
//  Atmosphere — depth, framing & cinematic post-processing
// ══════════════════════════════════════════════════════════════════
//
//  Layers (paint order):
//    1. Nebula field   — drifting colored fog blobs behind everything
//    2. Horizon glow   — soft band suggesting earth-curve / floor light
//    3. God rays       — radial volumetric light from the orb (into glow RT)
//    4. Vignette       — corner darkening + mood-tinted edge wash
//    5. Chromatic frame— RGB edge separation for cinematic feel
//    6. Corner brackets— minimal HUD frame ticks
//
//  All routines are fire-and-forget; they take a draw handle and read-only
//  state. No allocations in hot paths.

use crate::core::{hash_f, Mood};
use raylib::prelude::*;

#[inline]
fn mood_color(mood: Mood) -> (u8, u8, u8) {
    match mood {
        Mood::Serene => (60, 170, 230),
        Mood::Alert => (220, 170, 80),
        Mood::Stressed => (240, 120, 70),
        Mood::Critical => (255, 70, 70),
    }
}

// ──────────────────────────────────────────────────────────────────
//  1. Nebula field — slow-drifting volumetric fog blobs
// ──────────────────────────────────────────────────────────────────
//  ~16 large soft circles with simplex-driven motion, very low alpha.
//  Tinted by mood and dimmed by weather. Adds parallax depth to dead
//  middle space without competing with foreground elements.
/// Same as `draw_nebula_field` but mixes the mood color toward a cool
/// night palette by `day_factor` (1.0=full mood, 0.0=full night-blue).
pub fn draw_nebula_field_tinted(
    d: &mut impl RaylibDraw,
    w: i32,
    h: i32,
    t: f32,
    mood: Mood,
    weather_dim: f32,
    day_factor: f32,
) {
    let (mr0, mg0, mb0) = mood_color(mood);
    // Night palette: deep cobalt blue (matches sky background)
    let (nr, ng, nb) = (28.0_f32, 40.0_f32, 80.0_f32);
    let df = day_factor.clamp(0.0, 1.0);
    let mr = (mr0 as f32 * df + nr * (1.0 - df)) as u8;
    let mg = (mg0 as f32 * df + ng * (1.0 - df)) as u8;
    let mb = (mb0 as f32 * df + nb * (1.0 - df)) as u8;
    let wf = w as f32;
    let hf = h as f32;

    for i in 0..18u32 {
        let s = hash_f(i.wrapping_mul(8191) + 17);
        let s2 = hash_f(i.wrapping_mul(2003) + 91);
        let s3 = hash_f(i.wrapping_mul(7919) + 211);

        // Slow orbital drift — each blob has its own period
        let speed = 0.012 + s * 0.025;
        let phase = i as f32 * 1.7;
        let cx = wf * (0.05 + s * 0.9) + (t * speed + phase).sin() * wf * 0.06;
        let cy = hf * (0.08 + s2 * 0.78) + (t * speed * 0.7 + phase * 1.3).cos() * hf * 0.05;

        // Radius pulses gently
        let base_r = wf * (0.10 + s3 * 0.18);
        let breathe = (t * 0.18 + i as f32 * 0.6).sin() * 0.08 + 1.0;
        let r = base_r * breathe;

        // Alpha is tiny — many blobs sum into atmosphere
        let a_base = (12.0 + s2 * 14.0) * weather_dim;
        let a = a_base.clamp(2.0, 28.0) as u8;

        // Color: bias toward mood but desaturate per-blob with hash
        let tint = 0.55 + s3 * 0.4;
        let cr = ((mr as f32 * tint) + (1.0 - tint) * 30.0) as u8;
        let cg = ((mg as f32 * tint) + (1.0 - tint) * 50.0) as u8;
        let cb = ((mb as f32 * tint) + (1.0 - tint) * 80.0) as u8;

        // Soft falloff via gradient circle
        d.draw_circle_gradient(
            cx as i32,
            cy as i32,
            r,
            Color::new(cr, cg, cb, a),
            Color::new(cr, cg, cb, 0),
        );
    }
}

// ──────────────────────────────────────────────────────────────────
//  2. Horizon glow — soft floor light, suggests ground & atmosphere
// ──────────────────────────────────────────────────────────────────
pub fn draw_horizon_glow(
    d: &mut impl RaylibDraw,
    w: i32,
    h: i32,
    mood: Mood,
    intensity: f32, // 0..1
) {
    let (mr, mg, mb) = mood_color(mood);
    let hf = h as f32;
    // Horizon sits ~62% down the screen — below orb level, above HUD
    let horizon_y = (hf * 0.62) as i32;
    let band_h = (hf * 0.30) as i32;

    // Vertical gradient strip from horizon down — gets stronger toward middle-bottom
    let steps = 24;
    for s in 0..steps {
        let frac = s as f32 / (steps - 1) as f32;
        // Bell curve — peaks ~middle of band
        let bell = (1.0 - (frac - 0.35).abs() * 2.4).max(0.0);
        let a = (bell * 14.0 * intensity) as u8;
        if a < 2 {
            continue;
        }
        let y = horizon_y + (frac * band_h as f32) as i32;
        let strip_h = (band_h as f32 / steps as f32).ceil() as i32 + 1;
        d.draw_rectangle(
            0,
            y,
            w,
            strip_h,
            Color::new(mr / 2, mg / 2 + 10, (mb as u16 + 40).min(255) as u8, a),
        );
    }

    // Thin bright horizon line — barely visible, anchors the eye
    let line_a = (24.0 * intensity) as u8;
    d.draw_rectangle(
        0,
        horizon_y,
        w,
        1,
        Color::new(
            (mr as u16 + 60).min(255) as u8,
            (mg as u16 + 80).min(255) as u8,
            255,
            line_a,
        ),
    );
}

// ──────────────────────────────────────────────────────────────────
//  3. God rays — radial volumetric light from the orb
// ──────────────────────────────────────────────────────────────────
//  Drawn into the glow render-target so the bloom pass amplifies them.
//  A handful of long thin triangles emanate from the orb position,
//  rotating very slowly, with sin-modulated intensity. Creates the
//  illusion of dust catching the orb's light.

// ──────────────────────────────────────────────────────────────────
//  4. Vignette — corner darkening + mood-tinted edge wash
// ──────────────────────────────────────────────────────────────────
//  Drawn at the very end of post-processing, before HUD if you want
//  HUD to remain crisp. Uses concentric darkened rectangles emulating
//  a soft radial falloff via gradient circles in the four corners.
pub fn draw_vignette(
    d: &mut impl RaylibDraw,
    w: i32,
    h: i32,
    mood: Mood,
    strength: f32, // 0..1 — overall darkness
) {
    let (mr, mg, mb) = mood_color(mood);
    let wf = w as f32;
    let hf = h as f32;
    let _ = (mr, mg, mb, wf); // mood tint reserved for future edge wash use

    // Corner radial gradients removed — they were reading as a heavy
    // left/right "filter" over the scene. Keep only the very subtle
    // top + bottom widescreen edge wash below.
    let _clear = Color::new(0, 0, 0, 0);

    // Subtle top + bottom edge wash for cinematic widescreen feel
    let wash_h = (hf * 0.04) as i32;
    let wash_a = (110.0 * strength) as u8;
    for i in 0..wash_h {
        let frac = i as f32 / wash_h as f32;
        let a = ((1.0 - frac) * wash_a as f32) as u8;
        let c = Color::new(0, 0, 0, a);
        d.draw_rectangle(0, i, w, 1, c);
        d.draw_rectangle(0, h - i - 1, w, 1, c);
    }
}

// ──────────────────────────────────────────────────────────────────
//  5. Chromatic edge aberration — subtle RGB separation at edges
// ──────────────────────────────────────────────────────────────────
//  Cheap fake: red wash on left/top, blue wash on right/bottom edges.
//  Sells the "lens" feeling without an actual shader pass.
pub fn draw_chromatic_edges(
    d: &mut impl RaylibDraw,
    w: i32,
    h: i32,
    strength: f32, // 0..1
) {
    let edge = ((w.min(h)) as f32 * 0.06) as i32;
    let max_a = (28.0 * strength) as u8;
    if max_a < 2 {
        return;
    }

    for i in 0..edge {
        let frac = 1.0 - (i as f32 / edge as f32);
        let a = (frac * frac * max_a as f32) as u8;
        if a < 1 {
            continue;
        }
        // Red bleed on left edge
        d.draw_rectangle(i, 0, 1, h, Color::new(180, 30, 50, a / 3));
        // Blue bleed on right edge
        d.draw_rectangle(w - i - 1, 0, 1, h, Color::new(30, 80, 200, a / 3));
        // Cool top, warm bottom (subtle)
        d.draw_rectangle(0, i, w, 1, Color::new(40, 60, 120, a / 5));
        d.draw_rectangle(0, h - i - 1, w, 1, Color::new(100, 50, 80, a / 5));
    }
}

// ──────────────────────────────────────────────────────────────────
//  6. Cinematic corner brackets — minimal HUD frame ticks
// ──────────────────────────────────────────────────────────────────
pub fn draw_corner_brackets(
    d: &mut impl RaylibDraw,
    w: i32,
    h: i32,
    color: Color,
    inset: i32,
    arm_len: i32,
    thickness: i32,
) {
    let x0 = inset;
    let y0 = inset;
    let x1 = w - inset;
    let y1 = h - inset;

    // Top-left
    d.draw_rectangle(x0, y0, arm_len, thickness, color);
    d.draw_rectangle(x0, y0, thickness, arm_len, color);
    // Top-right
    d.draw_rectangle(x1 - arm_len, y0, arm_len, thickness, color);
    d.draw_rectangle(x1 - thickness, y0, thickness, arm_len, color);
    // Bottom-left
    d.draw_rectangle(x0, y1 - thickness, arm_len, thickness, color);
    d.draw_rectangle(x0, y1 - arm_len, thickness, arm_len, color);
    // Bottom-right
    d.draw_rectangle(x1 - arm_len, y1 - thickness, arm_len, thickness, color);
    d.draw_rectangle(x1 - thickness, y1 - arm_len, thickness, arm_len, color);
}
