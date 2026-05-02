use super::grain_frame_hash;
use crate::core::{hash_f, Mood};
use raylib::prelude::*;

// ════════════════════════════════════════════════════════════════
//  Weather System — physics-driven, layered, cinematic.
//
//  Components:
//   • Particle pool         (rain / snow / sleet / storm)
//   • Splash pool           (bottom-of-frame impact rings)
//   • Lens-drop pool        (camera-glass droplets that drip)
//   • Cloud pool            (2-layer parallax volumetric blobs)
//   • Lightning controller  (multi-stroke + ground glow + cloud-illum)
//   • Wind controller       (slow drift + stochastic gust events)
//   • Atmosphere tint       (storm desaturation overlay)
// ════════════════════════════════════════════════════════════════

pub const MAX_WEATHER_PARTICLES: usize = 480;
pub const MAX_LENS_DROPS: usize = 16;
pub const MAX_CLOUDS: usize = 16;
pub const MAX_SPLASHES: usize = 64;
pub const MAX_LIGHTNING_BOLTS: usize = 4;

// ── Particle ────────────────────────────────────────────────────
#[derive(Clone, Copy)]
pub struct WeatherParticle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub size: f32,
    pub alpha: f32,
    pub depth: f32, // 0=far, 1=near
    pub life: f32,
    pub seed: f32,
    pub kind: u8,  // 0=rain 1=snow 2=sleet
    pub spin: f32, // for snow rotation
}
impl Default for WeatherParticle {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            size: 1.0,
            alpha: 0.0,
            depth: 0.5,
            life: 0.0,
            seed: 0.0,
            kind: 0,
            spin: 0.0,
        }
    }
}

// ── Lens drop (camera glass) ────────────────────────────────────
#[derive(Clone, Copy)]
pub struct LensDrop {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub alpha: f32,
    pub life: f32,
    pub vy: f32,      // drip velocity
    pub trail_y: f32, // where it spawned (for trail draw)
}
impl Default for LensDrop {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            radius: 0.0,
            alpha: 0.0,
            life: 0.0,
            vy: 0.0,
            trail_y: 0.0,
        }
    }
}

// ── Cloud blob ──────────────────────────────────────────────────
#[derive(Clone, Copy)]
pub struct Cloud {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub width: f32,
    pub height: f32,
    pub alpha: f32,
    pub seed: f32,
    pub flicker: f32, // internal lightning illum (0..1, decays)
    pub layer: u8,    // 0=back parallax, 1=front parallax
}
impl Default for Cloud {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            width: 0.0,
            height: 0.0,
            alpha: 0.0,
            seed: 0.0,
            flicker: 0.0,
            layer: 0,
        }
    }
}

// ── Splash (rain hitting bottom) ────────────────────────────────
#[derive(Clone, Copy, Default)]
pub struct Splash {
    pub x: f32,
    pub y: f32,
    pub age: f32, // 0..1 (1 = dead)
    pub size: f32,
    pub alpha: f32,
}

// ── Lightning bolt geometry ─────────────────────────────────────
#[derive(Clone, Copy, Default)]
pub struct Bolt {
    pub seed: u32,
    pub x_top: f32,
    pub x_ground: f32,
    pub life: f32, // remaining seconds visible
    pub max_life: f32,
    pub strokes_left: u8, // multi-stroke count
    pub next_stroke_in: f32,
    pub intensity: f32, // 0..1, scaled by mood
}

// ── Main system ─────────────────────────────────────────────────
pub struct WeatherFX {
    pub particles: [WeatherParticle; MAX_WEATHER_PARTICLES],
    pub lens_drops: [LensDrop; MAX_LENS_DROPS],
    pub clouds: [Cloud; MAX_CLOUDS],
    pub splashes: [Splash; MAX_SPLASHES],
    pub bolts: [Bolt; MAX_LIGHTNING_BOLTS],
    pub cloud_count: usize,
    pub active_count: usize,

    pub wind_x: f32,
    pub wind_target: f32,
    pub gust_strength: f32, // current gust amplitude (px/s)
    pub gust_t: f32,        // remaining gust duration
    pub gust_dir: f32,      // -1 / +1
    pub next_gust_in: f32,

    pub flash_white: f32, // 0..1 full-screen flash intensity (decays)
    pub ground_glow: f32, // 0..1 short-lived ground strike glow
    pub initialized_for: u16,
}

impl WeatherFX {
    pub fn new() -> Self {
        Self {
            particles: [WeatherParticle::default(); MAX_WEATHER_PARTICLES],
            lens_drops: [LensDrop::default(); MAX_LENS_DROPS],
            clouds: [Cloud::default(); MAX_CLOUDS],
            splashes: [Splash::default(); MAX_SPLASHES],
            bolts: [Bolt::default(); MAX_LIGHTNING_BOLTS],
            cloud_count: 0,
            active_count: 0,
            wind_x: 0.0,
            wind_target: 0.0,
            gust_strength: 0.0,
            gust_t: 0.0,
            gust_dir: -1.0,
            next_gust_in: 6.0,
            flash_white: 0.0,
            ground_glow: 0.0,
            initialized_for: u16::MAX,
        }
    }

    /// Reinitialize pools for a new weather type.
    pub fn setup(&mut self, wc: u16, w: f32, h: f32, mood: Mood, wind_kph: f32) {
        let (is_clear, is_cloudy, is_fog, is_rain, is_snow, is_storm) = super::classify_weather(wc);
        let is_sleet = wc == 66 || wc == 67;

        let mood_mul = match mood {
            Mood::Serene => 0.6_f32,
            Mood::Alert => 0.85,
            Mood::Stressed => 1.2,
            Mood::Critical => 1.6,
        };

        // Density curve per regime
        let base_count = if is_storm {
            380
        } else if is_rain {
            if (51..=55).contains(&wc) {
                100
            } else {
                220
            }
        } else if is_snow {
            200
        } else if is_sleet {
            180
        } else {
            0
        };
        self.active_count = ((base_count as f32 * mood_mul) as usize).min(MAX_WEATHER_PARTICLES);

        // Wind target: weather baseline + live wind, scaled by mood
        let wind_mag = (wind_kph.max(0.0) * 3.0).min(240.0);
        let baseline = if is_storm {
            130.0
        } else if is_rain {
            55.0
        } else if is_snow {
            18.0
        } else {
            0.0
        };
        let combined = (baseline + wind_mag * 0.65) * mood_mul;
        self.wind_target = if combined > 0.0 { -combined } else { 0.0 };
        self.next_gust_in = 4.0 + hash_f(wc as u32 * 19) * 5.0;

        // ── Cloud pool: 2-layer parallax (back layer dimmer & slower) ──
        let cloud_n = if is_storm {
            14
        } else if is_rain {
            11
        } else if is_snow {
            10
        } else if is_cloudy {
            9
        } else if is_fog {
            5
        } else if is_clear {
            3
        } else {
            0
        };
        self.cloud_count = cloud_n.min(MAX_CLOUDS);
        for i in 0..self.cloud_count {
            let c = &mut self.clouds[i];
            c.seed = hash_f((i as u32).wrapping_mul(2693).wrapping_add(wc as u32 + 41));
            c.layer = if i % 3 == 0 { 0 } else { 1 }; // 1/3 back layer
            c.x = hash_f((i as u32).wrapping_mul(3517).wrapping_add(7)) * w * 1.4 - w * 0.2;
            c.y = h * 0.04 + hash_f((i as u32).wrapping_mul(4111)) * h * 0.32;
            let size_mul = if is_storm {
                1.7
            } else if is_rain || is_cloudy {
                1.35
            } else if is_snow {
                1.20
            } else {
                0.95
            };
            let layer_scale = if c.layer == 0 { 1.35 } else { 0.95 };
            c.width = (115.0 + c.seed * 230.0) * size_mul * layer_scale;
            c.height = c.width * (0.28 + c.seed * 0.18);
            let layer_alpha = if c.layer == 0 { 0.65 } else { 1.0 };
            c.alpha = (if is_clear {
                0.18
            } else if is_storm {
                0.72
            } else {
                0.46 + c.seed * 0.20
            }) * layer_alpha;
            let base_drift = -(8.0 + c.seed * 18.0);
            let layer_speed = if c.layer == 0 { 0.55 } else { 1.0 };
            c.vx = (base_drift - wind_mag * 0.10) * layer_speed;
            c.flicker = 0.0;
        }

        // Reset transient pools
        for s in self.splashes.iter_mut() {
            *s = Splash::default();
        }
        for b in self.bolts.iter_mut() {
            *b = Bolt::default();
        }
        for d in self.lens_drops.iter_mut() {
            *d = LensDrop::default();
        }
        self.flash_white = 0.0;
        self.ground_glow = 0.0;

        // Particles
        for i in 0..self.active_count {
            let p = &mut self.particles[i];
            p.seed = hash_f((i as u32).wrapping_mul(7919).wrapping_add(wc as u32));
            p.depth = hash_f((i as u32).wrapping_mul(6271).wrapping_add(13));
            p.x = hash_f((i as u32).wrapping_mul(4001)) * w * 1.2 - w * 0.1;
            p.y = hash_f((i as u32).wrapping_mul(3001)) * h * 1.3 - h * 0.15;
            p.spin = hash_f((i as u32).wrapping_mul(5273)) * std::f32::consts::TAU;

            // Mixed-precip kinds: sleet alternates rain & snow particles
            p.kind = if is_snow {
                1
            } else if is_sleet {
                if i & 1 == 0 {
                    0
                } else {
                    1
                }
            } else {
                0
            };

            p.size = if p.kind == 1 {
                1.2 + p.depth * 2.6
            } else {
                1.0 + p.depth * 1.4
            };
            p.alpha = 0.18 + p.depth * 0.55;
            p.life = 1.0 + p.seed * 2.0;
            let depth_speed = 0.4 + p.depth * 0.6;
            if p.kind == 1 {
                p.vy = (25.0 + p.seed * 40.0) * depth_speed;
                p.vx = self.wind_target * depth_speed * 0.3;
            } else {
                p.vy = (260.0 + p.seed * 320.0) * depth_speed;
                p.vx = self.wind_target * depth_speed;
            }
        }
        self.initialized_for = wc;
    }

    /// Advance all simulation state.
    pub fn update(
        &mut self,
        dt: f32,
        t: f32,
        w: f32,
        h: f32,
        orb_x: f32,
        orb_y: f32,
        mood: Mood,
        wc: u16,
    ) {
        let (_is_clear, _is_cloudy, _is_fog, _is_rain, is_snow, is_storm) =
            super::classify_weather(wc);
        let is_rain_like = (51..=67).contains(&wc) || (80..=82).contains(&wc) || is_storm;

        // ── Wind: slow drift + stochastic gust events ──
        self.wind_x += (self.wind_target - self.wind_x) * dt * 0.5;
        self.next_gust_in -= dt;
        if self.next_gust_in <= 0.0 {
            // Schedule a gust whose magnitude scales with weather intensity
            let intensity = if is_storm {
                1.6
            } else if is_rain_like {
                1.0
            } else if is_snow {
                0.5
            } else {
                0.3
            };
            let mood_mul = match mood {
                Mood::Serene => 0.5_f32,
                Mood::Alert => 0.8,
                Mood::Stressed => 1.3,
                Mood::Critical => 1.8,
            };
            let roll = hash_f(grain_frame_hash(t).wrapping_mul(8081));
            self.gust_strength = (60.0 + roll * 140.0) * intensity * mood_mul;
            self.gust_t = 0.7 + roll * 1.6;
            // Direction: bias same as wind, but allow opposite for chaos in storms
            self.gust_dir = if is_storm && roll > 0.7 { 1.0 } else { -1.0 };
            self.next_gust_in = 3.5 + hash_f(grain_frame_hash(t).wrapping_mul(173)) * 6.0;
        }
        // Gust ease-out
        let gust_env = if self.gust_t > 0.0 {
            (self.gust_t / 1.6).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.gust_t = (self.gust_t - dt).max(0.0);
        let gust_now = self.gust_strength * gust_env * self.gust_dir;
        // Layered turbulence (cheap pseudo-noise)
        let turb = (t * 0.7).sin() * 18.0 + (t * 1.9 + 3.0).sin() * 11.0 + (t * 4.3).sin() * 6.0;
        let mood_gust = match mood {
            Mood::Serene => 0.3,
            Mood::Alert => 0.6,
            Mood::Stressed => 1.2,
            Mood::Critical => 2.0,
        };

        // ── Clouds ──
        let cloud_wind_mul = 0.014;
        for i in 0..self.cloud_count {
            let c = &mut self.clouds[i];
            let layer_speed = if c.layer == 0 { 0.55 } else { 1.0 };
            c.x += (c.vx
                + (self.wind_x + gust_now * 0.4) * cloud_wind_mul * (1.0 + c.seed) * layer_speed)
                * dt;
            let bob = (t * (0.15 + c.seed * 0.20) + c.seed * 6.0).sin() * 0.7;
            c.y += bob * dt;
            let extent = c.width * 0.7;
            if c.x + extent < -50.0 {
                c.x = w + extent;
            }
            if c.x - extent > w + 50.0 {
                c.x = -extent;
            }
            c.flicker = (c.flicker - dt * 1.6).max(0.0);
        }

        // Decay screen-wide effects
        self.flash_white = (self.flash_white - dt * 4.0).max(0.0);
        self.ground_glow = (self.ground_glow - dt * 1.4).max(0.0);

        // ── Lightning controller (storms only) ──
        if is_storm {
            // Schedule new bolts
            for b in self.bolts.iter_mut() {
                if b.life <= 0.0 && b.strokes_left == 0 {
                    let mood_rate = match mood {
                        Mood::Serene => 0.0015,
                        Mood::Alert => 0.0035,
                        Mood::Stressed => 0.0075,
                        Mood::Critical => 0.0140,
                    };
                    let roll = hash_f(grain_frame_hash(t).wrapping_mul(60013).wrapping_add(7));
                    if (roll as f64) < mood_rate {
                        b.seed = grain_frame_hash(t).wrapping_mul(2654435761);
                        let nx = hash_f(b.seed.wrapping_add(11));
                        b.x_top = nx * w * 0.7 + w * 0.15;
                        b.x_ground = b.x_top + (hash_f(b.seed.wrapping_add(13)) - 0.5) * w * 0.25;
                        b.intensity = 0.7 + hash_f(b.seed.wrapping_add(17)) * 0.3;
                        b.max_life = 0.13 + hash_f(b.seed.wrapping_add(19)) * 0.07;
                        b.life = b.max_life;
                        // 1–3 return strokes
                        b.strokes_left = 1 + (hash_f(b.seed.wrapping_add(23)) * 3.0) as u8;
                        b.next_stroke_in = 0.0;
                        // Flash + ground glow
                        let mood_flash = match mood {
                            Mood::Serene => 0.4_f32,
                            Mood::Alert => 0.6,
                            Mood::Stressed => 0.85,
                            Mood::Critical => 1.0,
                        };
                        self.flash_white =
                            (self.flash_white + 0.55 * b.intensity * mood_flash).min(1.0);
                        self.ground_glow = (self.ground_glow + 0.85 * b.intensity).min(1.0);
                        // Light up nearest cloud
                        let mut best = 0usize;
                        let mut best_d = f32::MAX;
                        for ci in 0..self.cloud_count {
                            let d = (self.clouds[ci].x - b.x_top).abs();
                            if d < best_d {
                                best_d = d;
                                best = ci;
                            }
                        }
                        if self.cloud_count > 0 {
                            self.clouds[best].flicker = 1.0;
                        }
                        break; // one new bolt per frame max
                    }
                }
            }
            // Tick bolts: re-strike if scheduled
            for b in self.bolts.iter_mut() {
                if b.life > 0.0 {
                    b.life -= dt;
                } else if b.strokes_left > 0 {
                    if b.next_stroke_in <= 0.0 {
                        b.life = b.max_life * 0.55;
                        b.strokes_left -= 1;
                        b.seed = b.seed.wrapping_mul(1103515245).wrapping_add(12345);
                        b.next_stroke_in = 0.04 + hash_f(b.seed) * 0.10;
                        let mood_flash = match mood {
                            Mood::Serene => 0.3_f32,
                            Mood::Alert => 0.5,
                            Mood::Stressed => 0.7,
                            Mood::Critical => 0.9,
                        };
                        self.flash_white =
                            (self.flash_white + 0.32 * b.intensity * mood_flash).min(1.0);
                        self.ground_glow = (self.ground_glow + 0.45 * b.intensity).min(1.0);
                    } else {
                        b.next_stroke_in -= dt;
                    }
                }
            }
        }

        // ── Particles ──
        let orb_repulse_r = 92.0_f32;
        let orb_r2 = orb_repulse_r * orb_repulse_r;
        let ground_y = h - 6.0;

        for i in 0..self.active_count {
            let p = &mut self.particles[i];
            let depth_speed = 0.4 + p.depth * 0.6;

            // Per-kind gravity & dynamics
            let gravity = if p.kind == 1 { 35.0 } else { 470.0 };
            p.vy += gravity * depth_speed * dt;

            p.vx += (self.wind_x * depth_speed
                + gust_now * (0.4 + 0.6 * p.depth)
                + turb * mood_gust * p.depth)
                * dt;

            if p.kind == 1 {
                let sway = (t * (0.8 + p.seed * 1.2) + p.seed * 30.0).sin() * 28.0 * depth_speed;
                p.vx += (sway - p.vx * 0.3) * dt * 2.0;
                p.spin += (0.4 + p.seed * 1.6) * dt;
            }

            let max_vy = if p.kind == 1 {
                80.0 * depth_speed
            } else {
                640.0 * depth_speed
            };
            let max_vx = if is_storm { 220.0 } else { 110.0 };
            p.vy = p.vy.clamp(-50.0, max_vy);
            p.vx = p.vx.clamp(-max_vx, max_vx);

            p.x += p.vx * dt;
            p.y += p.vy * dt;

            // Orb force-field
            let dx = p.x - orb_x;
            let dy = p.y - orb_y;
            let dist2 = dx * dx + dy * dy;
            if dist2 < orb_r2 && dist2 > 1.0 {
                let dist = dist2.sqrt();
                let force = (1.0 - dist / orb_repulse_r) * 850.0;
                let nx = dx / dist;
                let ny = dy / dist;
                p.vx += nx * force * dt;
                p.vy += ny * force * dt;
                p.alpha = (p.alpha + (1.0 - dist / orb_repulse_r) * 0.45).min(1.0);
            } else {
                let base_alpha = 0.18 + p.depth * 0.55;
                p.alpha += (base_alpha - p.alpha) * dt * 3.0;
            }

            // Ground impact: rain/sleet → splash; snow recycles silently
            if p.y >= ground_y && p.kind != 1 && p.depth > 0.35 {
                let (sx, sd, ss) = (p.x, p.depth, p.seed);
                self.try_spawn_splash(sx, ground_y, sd, ss);
                self.recycle(i, w, h, t);
                continue;
            }

            // Wrap recycle
            if p.y > h + 30.0 || p.y < -60.0 || p.x < -80.0 || p.x > w + 80.0 {
                self.recycle(i, w, h, t);
            }
        }

        // ── Splashes ──
        for s in self.splashes.iter_mut() {
            if s.alpha > 0.0 {
                s.age += dt * 3.5; // ~0.28s lifetime
                if s.age >= 1.0 {
                    s.alpha = 0.0;
                    s.age = 1.0;
                } else {
                    s.alpha = (1.0 - s.age) * 0.85;
                }
            }
        }

        // ── Lens drops ──
        let lens_gravity = if is_storm { 35.0 } else { 18.0 };
        for drop in self.lens_drops.iter_mut() {
            if drop.life > 0.0 {
                drop.life -= dt;
                drop.alpha = (drop.life / 3.0).clamp(0.0, 1.0) * 0.30;
                // Gravity-driven drip (small drops stick longer)
                if drop.life < 2.4 {
                    drop.vy += lens_gravity * dt;
                    drop.vy = drop.vy.min(60.0);
                    // Larger drops drip faster
                    let drip = drop.vy * (drop.radius / 8.0).clamp(0.4, 1.6);
                    drop.y += drip * dt;
                }
            }
        }

        // Spawn lens drops (rain/storm only)
        if is_rain_like {
            let spawn_chance = if is_storm { 0.04 } else { 0.012 };
            let roll = hash_f(grain_frame_hash(t).wrapping_mul(8191));
            if roll < spawn_chance {
                if let Some(slot) = self.lens_drops.iter_mut().find(|d| d.life <= 0.0) {
                    let r = 3.5 + hash_f(grain_frame_hash(t).wrapping_add(303)) * 7.5;
                    slot.x = hash_f(grain_frame_hash(t).wrapping_add(101)) * w;
                    slot.y = hash_f(grain_frame_hash(t).wrapping_add(202)) * h * 0.85;
                    slot.trail_y = slot.y;
                    slot.radius = r;
                    slot.alpha = 0.30;
                    slot.life = 2.0 + hash_f(grain_frame_hash(t).wrapping_add(404)) * 2.5;
                    slot.vy = 0.0;
                }
            }
        }
    }

    fn try_spawn_splash(&mut self, x: f32, y: f32, depth: f32, seed: f32) {
        for s in self.splashes.iter_mut() {
            if s.alpha <= 0.0 {
                s.x = x;
                s.y = y;
                s.age = 0.0;
                s.size = 4.0 + depth * 8.0 + seed * 3.0;
                s.alpha = 0.85;
                return;
            }
        }
    }

    fn recycle(&mut self, i: usize, w: f32, h: f32, t: f32) {
        let p = &mut self.particles[i];
        let depth_speed = 0.4 + p.depth * 0.6;
        p.y = -10.0 - p.seed * 40.0;
        p.x = hash_f(((i as u32).wrapping_add(grain_frame_hash(t))).wrapping_mul(4999)) * w * 1.2
            - w * 0.1;
        if p.kind == 1 {
            p.vy = (25.0 + p.seed * 40.0) * depth_speed;
        } else {
            p.vy = (260.0 + p.seed * 320.0) * depth_speed;
        }
        let _ = h;
    }

    // ════════════════════════════════════════════════════════════
    //  DRAWING
    // ════════════════════════════════════════════════════════════

    /// Soft volumetric fog bands (codes 45/48). Vertical gradient + drift.
    pub fn draw_fog<D: RaylibDraw>(&self, d: &mut D, w: i32, h: i32, t: f32) {
        let h_f = h as f32;
        for i in 0..10u32 {
            let s = hash_f(i * 13 + 7000);
            let y_base = s * h_f;
            let y = y_base + (t * (1.5 + s * 3.5) + s * 10.0).sin() * 28.0;
            let band_h = (60.0 + s * 120.0) as i32;
            // Vertical gradient using draw_rectangle_gradient_v
            let core_a = (16.0 + s * 22.0) as u8;
            d.draw_rectangle_gradient_v(
                0,
                y as i32,
                w,
                band_h,
                Color::new(140, 155, 175, 0),
                Color::new(140, 155, 175, core_a),
            );
            d.draw_rectangle_gradient_v(
                0,
                (y as i32) + band_h / 2,
                w,
                band_h / 2,
                Color::new(140, 155, 175, core_a),
                Color::new(140, 155, 175, 0),
            );
        }
    }

    /// 2-layer parallax cloud render. Back layer first, then front layer.
    /// Storm clouds get internal flicker glow; non-storm clouds get a top
    /// silver-lining highlight that reads as light from above.
    pub fn draw_clouds<D: RaylibDraw>(&self, d: &mut D, wc: u16, mood: Mood) {
        if self.cloud_count == 0 {
            return;
        }
        let (is_clear, is_cloudy, _is_fog, is_rain, is_snow, is_storm) =
            super::classify_weather(wc);

        let (cr, cg, cb) = if is_storm {
            (38u8, 36u8, 56u8)
        } else if is_rain {
            (95, 100, 115)
        } else if is_snow {
            (210, 218, 230)
        } else if is_cloudy {
            (170, 178, 195)
        } else if is_clear {
            (200, 215, 230)
        } else {
            (160, 170, 185)
        };

        let warm_shift = match mood {
            Mood::Serene => 0i16,
            Mood::Alert => 6,
            Mood::Stressed => 12,
            Mood::Critical => 20,
        };
        let cr_m = (cr as i16 + warm_shift).clamp(0, 255) as u8;
        let cg_m = (cg as i16 + warm_shift / 2).clamp(0, 255) as u8;
        let cb_m = (cb as i16 - warm_shift / 2).clamp(0, 255) as u8;

        // Two passes: layer 0 (back, dimmer) then layer 1 (front)
        for layer in [0u8, 1u8] {
            for i in 0..self.cloud_count {
                let c = &self.clouds[i];
                if c.layer != layer {
                    continue;
                }
                let layer_dim = if layer == 0 { 0.65 } else { 1.0 };

                // Soft drop-shadow underbelly (deepens form)
                let shadow_a = (c.alpha * 70.0 * layer_dim) as u8;
                if shadow_a > 4 && !is_clear {
                    d.draw_ellipse(
                        c.x as i32,
                        (c.y + c.height * 0.55) as i32,
                        c.width * 0.55,
                        c.height * 0.45,
                        Color::new(
                            (cr_m as u16 * 6 / 10) as u8,
                            (cg_m as u16 * 6 / 10) as u8,
                            (cb_m as u16 * 7 / 10) as u8,
                            shadow_a,
                        ),
                    );
                }

                let blob_n = 5;
                for b in 0..blob_n {
                    let frac = b as f32 / (blob_n - 1) as f32;
                    let off_x = (frac - 0.5) * c.width * 0.70;
                    let off_y = (b as f32 * 0.13 - 0.13) * c.height
                        + (c.seed * 6.28 + b as f32 * 1.7).sin() * c.height * 0.12;
                    let bell = 1.0 - (frac - 0.5).abs() * 1.4;
                    let rx = c.width * (0.30 + bell.max(0.0) * 0.20);
                    let ry = c.height * (0.55 + (b as f32 * 0.21).sin() * 0.20);

                    let body_a = (c.alpha * 255.0 * layer_dim).clamp(0.0, 230.0) as u8;
                    d.draw_ellipse(
                        (c.x + off_x) as i32,
                        (c.y + off_y) as i32,
                        rx,
                        ry,
                        Color::new(cr_m, cg_m, cb_m, body_a),
                    );

                    // Silver lining (top highlight) — non-storm
                    if !is_storm {
                        let hl_a = (c.alpha * 110.0 * layer_dim) as u8;
                        if hl_a > 4 {
                            d.draw_ellipse(
                                (c.x + off_x) as i32,
                                (c.y + off_y - ry * 0.40) as i32,
                                rx * 0.55,
                                ry * 0.42,
                                Color::new(
                                    (cr_m as u16 + 40).min(255) as u8,
                                    (cg_m as u16 + 35).min(255) as u8,
                                    (cb_m as u16 + 28).min(255) as u8,
                                    hl_a,
                                ),
                            );
                        }
                    }
                }

                // Storm flicker: internal lightning glow (warm-cool)
                if is_storm && c.flicker > 0.05 {
                    let f = c.flicker;
                    let a = (f * 220.0).min(230.0) as u8;
                    d.draw_ellipse(
                        c.x as i32,
                        c.y as i32,
                        c.width * 0.50,
                        c.height * 0.95,
                        Color::new(225, 220, 255, a),
                    );
                    let a2 = (f * 110.0).min(180.0) as u8;
                    d.draw_ellipse(
                        c.x as i32,
                        c.y as i32,
                        c.width * 0.78,
                        c.height * 1.20,
                        Color::new(160, 160, 220, a2),
                    );
                }
            }
        }
    }

    /// Rain / snow / sleet particles. Replaces the old inline draw loop.
    pub fn draw_particles<D: RaylibDraw>(&self, d: &mut D, h: f32, wc: u16) {
        let (_clr, _cld, _fg, _rn, is_snow, _st) = super::classify_weather(wc);
        let _ = is_snow;

        for i in 0..self.active_count {
            let p = &self.particles[i];
            if p.y < -40.0 || p.y > h + 40.0 {
                continue;
            }
            let a = (p.alpha * 255.0).clamp(0.0, 255.0) as u8;

            if p.kind == 1 {
                // Snowflake: small filled dot + rotated faint cross hint at near depth
                let sz = (p.size * 0.85).max(1.0);
                d.draw_circle(p.x as i32, p.y as i32, sz, Color::new(225, 232, 245, a));
                if p.depth > 0.6 {
                    let arm = sz * 1.6;
                    let cs = p.spin.cos();
                    let sn = p.spin.sin();
                    let ax = cs * arm;
                    let ay = sn * arm;
                    let bx = -sn * arm * 0.8;
                    let by = cs * arm * 0.8;
                    let arm_a = (a as u16 * 5 / 10) as u8;
                    let col = Color::new(235, 240, 250, arm_a);
                    d.draw_line(
                        (p.x - ax) as i32,
                        (p.y - ay) as i32,
                        (p.x + ax) as i32,
                        (p.y + ay) as i32,
                        col,
                    );
                    d.draw_line(
                        (p.x - bx) as i32,
                        (p.y - by) as i32,
                        (p.x + bx) as i32,
                        (p.y + by) as i32,
                        col,
                    );
                }
            } else {
                // Rain / sleet streak with motion-blur tail (3-segment fade)
                let speed = (p.vx * p.vx + p.vy * p.vy).sqrt().max(1.0);
                let len_total = (speed * 0.045 * p.size).clamp(6.0, 38.0);
                let nx = p.vx / speed;
                let ny = p.vy / speed;

                // Gradient streak via 3 segments with falling alpha behind the head.
                let segs = 3;
                for s in 0..segs {
                    let f0 = s as f32 / segs as f32;
                    let f1 = (s + 1) as f32 / segs as f32;
                    let alpha_seg = (a as f32 * (1.0 - f0).powf(0.8)) as u8;
                    let x1 = p.x - nx * len_total * f1;
                    let y1 = p.y - ny * len_total * f1;
                    let x2 = p.x - nx * len_total * f0;
                    let y2 = p.y - ny * len_total * f0;
                    let col = if p.kind == 0 {
                        Color::new(150, 190, 230, alpha_seg)
                    } else {
                        // Sleet: cooler & paler
                        Color::new(195, 215, 235, alpha_seg)
                    };
                    d.draw_line(x1 as i32, y1 as i32, x2 as i32, y2 as i32, col);
                }
                // Bright head (near layer)
                if p.depth > 0.55 {
                    let hd = Color::new(220, 235, 255, a);
                    d.draw_line(
                        p.x as i32,
                        p.y as i32,
                        (p.x + nx * 2.0) as i32,
                        (p.y + ny * 2.0) as i32,
                        hd,
                    );
                }
            }
        }
    }

    /// Splashes from rain hitting the bottom of the frame.
    pub fn draw_splashes<D: RaylibDraw>(&self, d: &mut D) {
        for s in self.splashes.iter() {
            if s.alpha <= 0.0 {
                continue;
            }
            let r = s.size * (0.4 + s.age * 1.2);
            let a = (s.alpha * 200.0) as u8;
            // Expanding ring
            d.draw_circle_lines(s.x as i32, s.y as i32, r, Color::new(170, 200, 230, a));
            // Two micro-droplet pixels arcing up from the impact
            if s.age < 0.45 {
                let lift = (1.0 - s.age * 2.2).max(0.0) * 6.0;
                let drop_a = (a as u16 * 7 / 10) as u8;
                d.draw_pixel(
                    (s.x - r * 0.6) as i32,
                    (s.y - lift) as i32,
                    Color::new(190, 210, 230, drop_a),
                );
                d.draw_pixel(
                    (s.x + r * 0.6) as i32,
                    (s.y - lift * 0.85) as i32,
                    Color::new(190, 210, 230, drop_a),
                );
            }
        }
    }

    /// Lens drops + their drip trails. Drawn after the CRT/grain pass so they
    /// read as on the physical camera glass.
    pub fn draw_lens_drops<D: RaylibDraw>(&self, d: &mut D) {
        for drop in self.lens_drops.iter() {
            if drop.life <= 0.0 || drop.alpha <= 0.01 {
                continue;
            }
            let a = (drop.alpha * 255.0).clamp(0.0, 255.0) as u8;

            // Drip trail (line from spawn to current y)
            if drop.y > drop.trail_y + 1.0 {
                let trail_a = (a as u16 * 4 / 10) as u8;
                d.draw_line(
                    drop.x as i32,
                    drop.trail_y as i32,
                    drop.x as i32,
                    drop.y as i32,
                    Color::new(170, 195, 225, trail_a),
                );
            }

            // Refraction edge (outer ring) — slight squash for elongated drips
            d.draw_circle_lines(
                drop.x as i32,
                drop.y as i32,
                drop.radius,
                Color::new(180, 200, 230, a),
            );
            // Inner fill
            d.draw_circle(
                drop.x as i32,
                drop.y as i32,
                drop.radius * 0.6,
                Color::new(160, 185, 210, a / 3),
            );
            // Specular highlight (tiny bright dot upper-left)
            let spec_a = (a as u16 * 9 / 10).min(255) as u8;
            d.draw_circle(
                (drop.x - drop.radius * 0.35) as i32,
                (drop.y - drop.radius * 0.35) as i32,
                (drop.radius * 0.18).max(1.0),
                Color::new(230, 240, 255, spec_a),
            );
        }
    }

    /// Lightning: full-screen flash + bolts + ground glow + branching forks.
    pub fn draw_lightning<D: RaylibDraw>(&self, d: &mut D, w: i32, h: i32, mood: Mood) {
        // Sky flash overlay (decays in update)
        if self.flash_white > 0.01 {
            let mood_mul = match mood {
                Mood::Serene => 0.7_f32,
                Mood::Alert => 0.9,
                Mood::Stressed => 1.1,
                Mood::Critical => 1.3,
            };
            let a = (self.flash_white * 95.0 * mood_mul).min(255.0) as u8;
            d.draw_rectangle(0, 0, w, h, Color::new(210, 215, 255, a));
        }

        // Ground glow (where the bolt struck)
        if self.ground_glow > 0.01 {
            let h_f = h as f32;
            let w_f = w as f32;
            // Find brightest bolt to anchor glow
            let mut bx = w_f * 0.5;
            let mut best = 0.0;
            for b in self.bolts.iter() {
                if b.life > 0.0 && b.intensity > best {
                    best = b.intensity;
                    bx = b.x_ground;
                }
            }
            let glow_h = (h_f * 0.18).min(140.0);
            // Vertical gradient glow rising from the ground
            d.draw_rectangle_gradient_v(
                0,
                (h_f - glow_h) as i32,
                w,
                glow_h as i32,
                Color::new(200, 200, 255, 0),
                Color::new(220, 220, 255, (self.ground_glow * 80.0) as u8),
            );
            // Hotspot halo at strike base
            let halo = (90.0 * self.ground_glow) as f32;
            for ring in 0..4 {
                let rr = halo * (1.0 + ring as f32 * 0.5);
                let aa = ((self.ground_glow * 90.0) / (1.0 + ring as f32)) as u8;
                d.draw_circle(
                    bx as i32,
                    (h_f - 4.0) as i32,
                    rr,
                    Color::new(230, 230, 255, aa / 3),
                );
            }
        }

        // Bolts (only while lit)
        for b in self.bolts.iter() {
            if b.life <= 0.0 {
                continue;
            }
            let lit = (b.life / b.max_life.max(0.001)).clamp(0.0, 1.0);
            let bolt_a = (lit * 230.0 * b.intensity).min(255.0) as u8;
            self.draw_bolt(d, b, h, bolt_a);
        }
    }

    /// Single fractal bolt (trunk + branches). Deterministic per b.seed.
    fn draw_bolt<D: RaylibDraw>(&self, d: &mut D, b: &Bolt, h: i32, a: u8) {
        let h_f = h as f32;
        let seg_count = 18u32;
        let seg_h = h_f / seg_count as f32;

        // Path lerps from x_top (sky) to x_ground (impact)
        let mut bx = b.x_top;
        let mut by = 0.0_f32;
        for seg in 0..seg_count {
            let t_lin = (seg + 1) as f32 / seg_count as f32;
            let path_x = b.x_top + (b.x_ground - b.x_top) * t_lin;
            let jitter = (hash_f(b.seed.wrapping_add(seg).wrapping_mul(31)) - 0.5) * 90.0;
            let nx = path_x + jitter;
            let ny = by + seg_h;

            // Outer halo
            d.draw_line(
                (bx as i32) - 1,
                by as i32,
                (nx as i32) - 1,
                ny as i32,
                Color::new(190, 180, 240, a / 3),
            );
            d.draw_line(
                (bx as i32) + 1,
                by as i32,
                (nx as i32) + 1,
                ny as i32,
                Color::new(190, 180, 240, a / 3),
            );
            // Core
            d.draw_line(
                bx as i32,
                by as i32,
                nx as i32,
                ny as i32,
                Color::new(220, 215, 255, a),
            );
            // Bright spine (one-pixel hot center)
            d.draw_line(
                bx as i32,
                by as i32,
                nx as i32,
                ny as i32,
                Color::new(255, 255, 255, a / 2),
            );

            // Branches (~30% per seg, never on first/last)
            let roll = hash_f(b.seed.wrapping_add(seg).wrapping_mul(73));
            if seg > 0 && seg < seg_count - 1 && roll > 0.70 {
                let branch_dir = if hash_f(b.seed.wrapping_add(seg).wrapping_mul(137)) > 0.5 {
                    1.0
                } else {
                    -1.0
                };
                let branch_len = seg_h * (0.7 + roll * 1.3);
                let bx2 = nx + branch_dir * (24.0 + roll * 50.0);
                let by2 = ny + branch_len;
                d.draw_line(
                    nx as i32,
                    ny as i32,
                    bx2 as i32,
                    by2 as i32,
                    Color::new(180, 170, 230, (a as u16 * 6 / 10) as u8),
                );
                // Sub-branch
                if roll > 0.88 {
                    let bx3 = bx2 + branch_dir * (15.0 + roll * 22.0);
                    let by3 = by2 + branch_len * 0.5;
                    d.draw_line(
                        bx2 as i32,
                        by2 as i32,
                        bx3 as i32,
                        by3 as i32,
                        Color::new(160, 150, 210, (a as u16 * 4 / 10) as u8),
                    );
                }
            }
            bx = nx;
            by = ny;
        }
    }

    /// Condition-wide atmospheric color grade. This sits behind the weather
    /// particles and clouds, so precipitation changes the whole screen instead
    /// of looking like sprites pasted over a blue sky.
    pub fn draw_atmosphere_tint<D: RaylibDraw>(&self, d: &mut D, w: i32, h: i32, wc: u16, t: f32) {
        let (is_clear, is_cloudy, is_fog, is_rain, is_snow, is_storm) = super::classify_weather(wc);
        let (r, g, b, base_a, band_a) = if is_storm {
            (24, 18, 42, 54, 34)
        } else if is_rain {
            (22, 34, 48, 42, 24)
        } else if is_snow {
            (175, 205, 230, 28, 18)
        } else if is_fog {
            (125, 135, 142, 46, 30)
        } else if is_cloudy {
            (52, 58, 68, 32, 16)
        } else if is_clear {
            (10, 18, 28, 8, 0)
        } else {
            (30, 36, 46, 18, 8)
        };

        if base_a > 0 {
            d.draw_rectangle(0, 0, w, h, Color::new(r, g, b, base_a));
        }

        if band_a > 0 {
            let h_f = h as f32;
            for i in 0..4u32 {
                let seed = hash_f(wc as u32 * 97 + i * 131);
                let band_h = (h_f * (0.16 + seed * 0.12)) as i32;
                let speed = if is_storm {
                    24.0
                } else if is_rain {
                    16.0
                } else {
                    7.0
                };
                let y = ((t * speed + seed * h_f * 1.7) % (h_f + band_h as f32)) - band_h as f32;
                let alpha = (band_a as f32 * (0.55 + seed * 0.45)) as u8;
                d.draw_rectangle_gradient_v(
                    0,
                    y as i32,
                    w,
                    band_h,
                    Color::new(r, g, b, 0),
                    Color::new(r, g, b, alpha),
                );
                d.draw_rectangle_gradient_v(
                    0,
                    y as i32 + band_h / 2,
                    w,
                    band_h / 2,
                    Color::new(r, g, b, alpha),
                    Color::new(r, g, b, 0),
                );
            }
        }
    }
}
