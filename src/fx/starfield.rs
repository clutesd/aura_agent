// ═══════════════════════════════════════════════════════════════
//  Starfield — multi-layer parallax stars + shooting stars
//  Stars are NOT just background dots: they drift, twinkle with
//  varied frequencies, flare in response to thought pulses, and
//  the brightest tier feeds the bloom buffer for soft halation.
// ═══════════════════════════════════════════════════════════════

use crate::core::{hash_f, Mood};
use crate::fx::synapse::ThoughtPulse;
use raylib::prelude::*;

#[derive(Clone, Copy)]
pub struct Star {
    pub x: f32,
    pub y: f32,
    /// 0 = deep (small/dim), 1 = mid, 2 = near (large/bright)
    pub layer: u8,
    /// 0..1 base luminance
    pub base: f32,
    /// per-star twinkle phase
    pub phase: f32,
    /// per-star twinkle frequency (Hz-ish)
    pub freq: f32,
    /// 0..1 hue lean (cool→cyan, warm→amber/violet)
    pub tint: f32,
    /// per-star drift velocity
    pub vx: f32,
    pub vy: f32,
    /// flare accumulator (0..1) — boosted by passing pulse rings, decays
    pub flare: f32,
}

#[derive(Clone, Copy)]
pub struct ShootingStar {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max_life: f32,
    pub tint: f32,
}

pub struct Starfield {
    pub stars: Vec<Star>,
    pub shooting: Vec<ShootingStar>,
    spawn_timer: f32,
    w: f32,
    h: f32,
}

impl Starfield {
    pub fn new(w: i32, h: i32) -> Self {
        let wf = w as f32;
        let hf = h as f32;
        // Three layers, weighted toward mid/deep - BOOSTED for visibility
        let layer_counts = [400u32, 280, 180]; // deep, mid, near
        let mut stars = Vec::with_capacity(860);
        let mut seed: u32 = 0;
        for (li, &count) in layer_counts.iter().enumerate() {
            for _ in 0..count {
                let h0 = hash_f(seed.wrapping_mul(7) + 1);
                let h1 = hash_f(seed.wrapping_mul(7) + 2);
                let h2 = hash_f(seed.wrapping_mul(7) + 3);
                let h3 = hash_f(seed.wrapping_mul(7) + 4);
                let h4 = hash_f(seed.wrapping_mul(7) + 5);
                let h5 = hash_f(seed.wrapping_mul(7) + 6);
                seed = seed.wrapping_add(1);
                // MUCH brighter for visibility over nebula/aurora
                let base = match li {
                    0 => 0.35 + h2 * 0.35, // deep: brighter baseline
                    1 => 0.55 + h2 * 0.40, // mid: very visible
                    _ => 0.75 + h2 * 0.25, // near: brilliant
                };
                // Drift speed scales with layer (near drifts more)
                let speed = match li {
                    0 => 0.6,
                    1 => 1.4,
                    _ => 2.6,
                };
                let dir = h5 * std::f32::consts::TAU;
                stars.push(Star {
                    x: h0 * wf,
                    y: h1 * hf,
                    layer: li as u8,
                    base,
                    phase: h3 * std::f32::consts::TAU,
                    // mostly slow shimmer with rare fast twinklers
                    freq: 0.4 + h4 * 1.6 + if h4 > 0.92 { 3.0 } else { 0.0 },
                    tint: h2,
                    vx: dir.cos() * speed,
                    vy: dir.sin() * speed * 0.4, // mostly horizontal drift
                    flare: 0.0,
                });
            }
        }
        Self {
            stars,
            shooting: Vec::new(),
            spawn_timer: 1.5, // spawn shooting stars much faster
            w: wf,
            h: hf,
        }
    }

    /// Update drift, twinkle flares, and spawn shooting stars.
    pub fn update(&mut self, dt: f32, mood: Mood, pulses: &[ThoughtPulse], thought_burst: bool) {
        // Mood-driven cosmic wind
        let wind = match mood {
            Mood::Serene => -2.0, // gentle leftward drift
            Mood::Alert => 4.0,
            Mood::Stressed => 8.0,
            Mood::Critical => 14.0,
        };
        let w = self.w;
        let h = self.h;

        for s in &mut self.stars {
            let parallax = match s.layer {
                0 => 0.15,
                1 => 0.35,
                _ => 0.7,
            };
            s.x += (s.vx + wind * parallax) * dt;
            s.y += s.vy * dt;
            // wrap around screen edges (keeps the field full forever)
            if s.x < -2.0 {
                s.x += w + 4.0;
            }
            if s.x > w + 2.0 {
                s.x -= w + 4.0;
            }
            if s.y < -2.0 {
                s.y += h + 4.0;
            }
            if s.y > h + 2.0 {
                s.y -= h + 4.0;
            }

            // Pulse-ring flare accumulator
            let mut boost = 0.0_f32;
            for p in pulses {
                let dx = s.x - p.x;
                let dy = s.y - p.y;
                let dist = (dx * dx + dy * dy).sqrt();
                let ring = (dist - p.radius).abs();
                if ring < 80.0 {
                    let prox = 1.0 - ring / 80.0;
                    boost += prox * p.alpha;
                }
            }
            // Thought-burst sparkle: when AI starts thinking, all near-tier
            // stars receive a small uniform shimmer kick.
            if thought_burst && s.layer == 2 {
                boost += 0.3;
            }
            s.flare = (s.flare * (1.0 - dt * 2.0) + boost).min(1.5);
        }

        // Shooting-star spawn — rarer in Serene, more frequent under stress
        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 {
            let interval = match mood {
                Mood::Serene => 4.5, // much more frequent
                Mood::Alert => 3.0,
                Mood::Stressed => 2.0,
                Mood::Critical => 1.2,
            };
            self.spawn_timer = interval + hash_f((dt * 1e6) as u32 ^ 0xA17F) * interval * 0.6;
            self.spawn_shooting(mood);
        }

        // Update shooting stars
        for s in &mut self.shooting {
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.life -= dt;
        }
        self.shooting.retain(|s| {
            s.life > 0.0
                && s.x > -200.0
                && s.x < self.w + 200.0
                && s.y > -200.0
                && s.y < self.h + 200.0
        });
    }

    fn spawn_shooting(&mut self, mood: Mood) {
        let seed = (self.spawn_timer * 9173.0) as u32 ^ 0xC0FFEE;
        let h0 = hash_f(seed);
        let h1 = hash_f(seed.wrapping_add(1));
        let h2 = hash_f(seed.wrapping_add(2));
        let h3 = hash_f(seed.wrapping_add(3));

        // Spawn from a screen edge moving inward + downward
        let from_left = h0 < 0.5;
        let x = if from_left { -40.0 } else { self.w + 40.0 };
        let y = h1 * self.h * 0.5; // upper half
        let speed = 320.0 + h2 * 380.0;
        let angle = if from_left {
            0.18 + h3 * 0.35 // down-right
        } else {
            std::f32::consts::PI - 0.18 - h3 * 0.35 // down-left
        };
        let life = 0.8 + h2 * 0.9;
        let tint = if matches!(mood, Mood::Critical | Mood::Stressed) {
            0.8 + h3 * 0.2 // warm red/amber under duress
        } else {
            h3 * 0.5 // cool cyan/white
        };
        self.shooting.push(ShootingStar {
            x,
            y,
            vx: angle.cos() * speed,
            vy: angle.sin() * speed,
            life,
            max_life: life,
            tint,
        });
    }

    /// Tint a star's color by its hue lean and mood palette.
    fn star_color(tint: f32, brightness: f32, mood: Mood, alpha: u8) -> Color {
        // Cool baseline (white-cyan) → warm tint pull (amber/violet)
        let (r, g, b) = if tint > 0.85 {
            // Disable rare amber/yellow coloring to reduce visual noise:
            // map the extreme warm tint to a neutral white instead.
            (235.0, 240.0, 250.0)
        } else if tint > 0.7 {
            // soft warm
            (240.0, 225.0, 210.0)
        } else if tint > 0.4 {
            // neutral white
            (235.0, 240.0, 250.0)
        } else if tint > 0.15 {
            // cool cyan-white
            (200.0, 225.0, 255.0)
        } else {
            // deep blue
            (170.0, 210.0, 255.0)
        };
        // Mood subtly biases the field
        let (mr, mg, mb) = match mood {
            Mood::Serene => (1.00, 1.00, 1.05),
            Mood::Alert => (1.05, 1.00, 0.95),
            Mood::Stressed => (1.10, 0.95, 0.85),
            Mood::Critical => (1.20, 0.85, 0.80),
        };
        let cr = (r * brightness * mr).min(255.0) as u8;
        let cg = (g * brightness * mg).min(255.0) as u8;
        let cb = (b * brightness * mb).min(255.0) as u8;
        Color::new(cr, cg, cb, alpha)
    }

    /// Main draw — call once on the back buffer (after nebula, before aurora).
    pub fn draw<D: RaylibDraw>(&self, d: &mut D, t: f32, mood: Mood, alpha_mul: f32) {
        for s in &self.stars {
            // Twinkle: combine slow breathing + sharp shimmer
            let tw = (t * s.freq + s.phase).sin() * 0.5 + 0.5;
            let shimmer = (t * (s.freq * 3.7 + 1.0) + s.phase * 1.3)
                .sin()
                .max(0.0)
                .powf(4.0);
            let lum =
                (s.base * (0.55 + 0.45 * tw) + shimmer * 0.35 + s.flare * 0.6).clamp(0.0, 1.6);

            // Per-layer base alpha - MUCH BRIGHTER
            let layer_a = match s.layer {
                0 => 200.0,
                1 => 240.0,
                _ => 255.0,
            };
            let alpha = (lum * layer_a * alpha_mul).min(255.0) as u8;
            if alpha < 6 {
                continue;
            }

            let col = Self::star_color(s.tint, lum.min(1.0), mood, alpha);

            // Size by layer + flare bump - LARGER
            let sz = match s.layer {
                0 => {
                    if lum > 0.6 {
                        2
                    } else {
                        1
                    }
                }
                1 => {
                    if lum > 0.7 || s.flare > 0.3 {
                        3
                    } else {
                        2
                    }
                }
                _ => {
                    if lum > 0.8 || s.flare > 0.2 {
                        4
                    } else {
                        3
                    }
                }
            };

            d.draw_rectangle(s.x as i32, s.y as i32, sz, sz, col);

            // Diffraction cross on the brightest near-layer stars - MORE PROMINENT
            if s.layer >= 1 && (lum > 0.7 || s.flare > 0.3) {
                let cross_a = ((lum - 0.5).max(0.0) * 240.0 + s.flare * 200.0).min(255.0) as u8;
                let cc = Color::new(col.r, col.g, col.b, cross_a);
                let len = 4 + ((lum + s.flare) * 6.0) as i32;
                let cx = s.x as i32;
                let cy = s.y as i32;
                d.draw_line(cx - len, cy, cx + len, cy, cc);
                d.draw_line(cx, cy - len, cx, cy + len, cc);
            }
        }

        // Shooting stars — gradient streak
        for s in &self.shooting {
            let life_t = (s.life / s.max_life).clamp(0.0, 1.0);
            // Fade in fast, fade out slower
            let env = (life_t * 1.4).min(1.0) * (1.0 - (1.0 - life_t).powi(3));
            let speed = (s.vx * s.vx + s.vy * s.vy).sqrt().max(1.0);
            let nx = s.vx / speed;
            let ny = s.vy / speed;
            let len = 28.0 + (1.0 - life_t) * 60.0;
            let head = (s.x, s.y);
            let segs = 6;
            for i in 0..segs {
                let t0 = i as f32 / segs as f32;
                let t1 = (i + 1) as f32 / segs as f32;
                let x0 = head.0 - nx * len * t0;
                let y0 = head.1 - ny * len * t0;
                let x1 = head.0 - nx * len * t1;
                let y1 = head.1 - ny * len * t1;
                let a = (env * (1.0 - t0) * 220.0) as u8;
                if a < 4 {
                    continue;
                }
                let col = Self::star_color(s.tint, 1.0, mood, a);
                d.draw_line_ex(
                    Vector2::new(x0, y0),
                    Vector2::new(x1, y1),
                    1.0 + (1.0 - t0) * 1.5,
                    col,
                );
            }
            // Bright head dot
            let head_a = (env * 255.0) as u8;
            d.draw_circle(
                s.x as i32,
                s.y as i32,
                1.6,
                Self::star_color(s.tint, 1.0, mood, head_a),
            );
        }
    }

    /// Bloom-buffer feed — only the brightest stars + shooting heads, so
    /// the bloom pass gives them soft ethereal halos without washing out
    /// the entire field.
    pub fn draw_glow<D: RaylibDraw>(&self, d: &mut D, t: f32, mood: Mood, alpha_mul: f32) {
        for s in &self.stars {
            // Include ALL layers in bloom for maximum glow
            let tw = (t * s.freq + s.phase).sin() * 0.5 + 0.5;
            let shimmer = (t * (s.freq * 3.7 + 1.0) + s.phase * 1.3)
                .sin()
                .max(0.0)
                .powf(4.0);
            let lum = s.base * (0.55 + 0.45 * tw) + shimmer * 0.35 + s.flare * 0.8;
            // Lower threshold for bloom contribution
            if lum < 0.4 {
                continue;
            }
            let alpha = ((lum - 0.3) * 255.0 * alpha_mul).min(255.0) as u8;
            if alpha < 8 {
                continue;
            }
            let col = Self::star_color(s.tint, lum.min(1.3), mood, alpha);
            let r = match s.layer {
                0 => 1.5,
                1 => 2.5,
                _ => 3.5,
            };
            d.draw_circle(s.x as i32, s.y as i32, r, col);
        }

        for s in &self.shooting {
            let life_t = (s.life / s.max_life).clamp(0.0, 1.0);
            let env = (life_t * 1.4).min(1.0) * (1.0 - (1.0 - life_t).powi(3));
            let a = (env * 255.0 * alpha_mul) as u8; // brighter
            if a < 6 {
                continue;
            }
            d.draw_circle(
                s.x as i32,
                s.y as i32,
                5.0,
                Self::star_color(s.tint, 1.2, mood, a),
            ); // larger radius
        }
    }
}
