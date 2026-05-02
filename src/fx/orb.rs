use super::easing::{damp, ease_in_out_sine, ease_out_cubic};
use crate::core::{hash_f, Mood};
use raylib::prelude::*;

// ═══════════════════════════════════════════════════════════════
//  Orb character AI — simple steering behaviours
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OrbBehavior {
    Wander,      // drift toward random waypoints
    Investigate, // move toward an edge or corner (curious)
    Orbit,       // circle a point of interest
    Rest,        // gentle sway near current position
}

/// Returns (max_speed, steer_force) for a given mood. Used by the
/// mood-blended motion pipeline so transitions ease instead of snap.
#[inline]
fn mood_motion_params(mood: Mood) -> (f32, f32) {
    match mood {
        Mood::Serene => (80.0, 120.0),
        Mood::Alert => (150.0, 220.0),
        Mood::Stressed => (250.0, 350.0),
        Mood::Critical => (380.0, 500.0),
    }
}

pub struct OrbAI {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub behavior: OrbBehavior,
    pub behavior_timer: f32,
    pub orbit_angle: f32,
    pub orbit_cx: f32,
    pub orbit_cy: f32,
    pub rest_anchor_x: f32,
    pub rest_anchor_y: f32,
    pub seed: u32,
    // ── Mood-eased transition state ──
    pub mood_from: Mood,
    pub mood_to: Mood,
    pub mood_blend: f32, // 0..1, eased over ~1.8s
    // ── Micro-saccades & dwell (curiosity polish) ──
    pub saccade_x: f32,
    pub saccade_y: f32,
    pub saccade_timer: f32,
    pub dwell_timer: f32, // > 0 ⇒ orb pauses to "look"
}

impl OrbAI {
    pub fn new(cx: f32, cy: f32) -> Self {
        Self {
            x: cx,
            y: cy,
            vx: 0.0,
            vy: 0.0,
            target_x: cx,
            target_y: cy,
            behavior: OrbBehavior::Wander,
            behavior_timer: 0.5,
            orbit_angle: 0.0,
            orbit_cx: cx,
            orbit_cy: cy,
            rest_anchor_x: cx,
            rest_anchor_y: cy,
            seed: 42,
            mood_from: Mood::Serene,
            mood_to: Mood::Serene,
            mood_blend: 1.0,
            saccade_x: 0.0,
            saccade_y: 0.0,
            saccade_timer: 0.5,
            dwell_timer: 0.0,
        }
    }

    /// Notify the orb of a mood change so motion parameters can ease
    /// between the old and new mood instead of snapping.
    pub fn set_mood(&mut self, new_mood: Mood) {
        if new_mood as u8 != self.mood_to as u8 {
            self.mood_from = self.mood_to;
            self.mood_to = new_mood;
            self.mood_blend = 0.0;
        }
    }

    pub fn pick_behavior(&mut self, mood: Mood, t: f32, w: f32, h: f32) {
        self.seed = self.seed.wrapping_add((t * 1000.0) as u32);
        let roll = hash_f(self.seed);

        self.behavior = match mood {
            Mood::Serene => {
                // mostly rest and gentle wander
                if roll < 0.35 {
                    OrbBehavior::Rest
                } else if roll < 0.70 {
                    OrbBehavior::Wander
                } else if roll < 0.88 {
                    OrbBehavior::Orbit
                } else {
                    OrbBehavior::Investigate
                }
            }
            Mood::Alert => {
                // curious — more investigation and wander
                if roll < 0.15 {
                    OrbBehavior::Rest
                } else if roll < 0.45 {
                    OrbBehavior::Wander
                } else if roll < 0.65 {
                    OrbBehavior::Investigate
                } else {
                    OrbBehavior::Orbit
                }
            }
            Mood::Stressed => {
                // erratic — lots of wander and investigate
                if roll < 0.05 {
                    OrbBehavior::Rest
                } else if roll < 0.40 {
                    OrbBehavior::Wander
                } else if roll < 0.75 {
                    OrbBehavior::Investigate
                } else {
                    OrbBehavior::Orbit
                }
            }
            Mood::Critical => {
                // frantic — never rests
                if roll < 0.50 {
                    OrbBehavior::Wander
                } else if roll < 0.80 {
                    OrbBehavior::Investigate
                } else {
                    OrbBehavior::Orbit
                }
            }
        };

        // Duration the behavior lasts
        let base_dur = match self.behavior {
            OrbBehavior::Wander => 4.0,
            OrbBehavior::Investigate => 3.5,
            OrbBehavior::Orbit => 5.0,
            OrbBehavior::Rest => 6.0,
        };
        let mood_scale = match mood {
            Mood::Serene => 1.4,
            Mood::Alert => 1.0,
            Mood::Stressed => 0.7,
            Mood::Critical => 0.5,
        };
        self.behavior_timer = base_dur * mood_scale + hash_f(self.seed.wrapping_add(1)) * 3.0;

        // Set up behavior-specific state — full screen roaming
        let margin_x = w * 0.03;
        let margin_y = h * 0.03;
        match self.behavior {
            OrbBehavior::Wander => {
                self.target_x = margin_x + hash_f(self.seed.wrapping_add(2)) * (w - margin_x * 2.0);
                self.target_y = margin_y + hash_f(self.seed.wrapping_add(3)) * (h - margin_y * 2.0);
            }
            OrbBehavior::Investigate => {
                // pick an edge or corner to inspect — can go right up to the edges
                let edge = (hash_f(self.seed.wrapping_add(4)) * 8.0) as u32;
                match edge {
                    0 => {
                        self.target_x = margin_x;
                        self.target_y = hash_f(self.seed.wrapping_add(5)) * h;
                    }
                    1 => {
                        self.target_x = w - margin_x;
                        self.target_y = hash_f(self.seed.wrapping_add(5)) * h;
                    }
                    2 => {
                        self.target_x = hash_f(self.seed.wrapping_add(5)) * w;
                        self.target_y = margin_y;
                    }
                    3 => {
                        self.target_x = hash_f(self.seed.wrapping_add(5)) * w;
                        self.target_y = h - margin_y;
                    }
                    // corners
                    4 => {
                        self.target_x = margin_x;
                        self.target_y = margin_y;
                    }
                    5 => {
                        self.target_x = w - margin_x;
                        self.target_y = margin_y;
                    }
                    6 => {
                        self.target_x = margin_x;
                        self.target_y = h - margin_y;
                    }
                    _ => {
                        self.target_x = w - margin_x;
                        self.target_y = h - margin_y;
                    }
                }
            }
            OrbBehavior::Orbit => {
                // orbit around a random point — full screen
                self.orbit_cx = w * 0.1 + hash_f(self.seed.wrapping_add(6)) * w * 0.8;
                self.orbit_cy = h * 0.1 + hash_f(self.seed.wrapping_add(7)) * h * 0.8;
                self.orbit_angle = hash_f(self.seed.wrapping_add(8)) * std::f32::consts::TAU;
            }
            OrbBehavior::Rest => {
                self.rest_anchor_x = self.x;
                self.rest_anchor_y = self.y;
            }
        }
    }

    pub fn update(&mut self, dt: f32, t: f32, mood: Mood, w: f32, h: f32) {
        // ── Mood transition: ease parameters instead of snapping ──
        self.set_mood(mood);
        self.mood_blend = (self.mood_blend + dt / 1.8).min(1.0);
        let me = ease_in_out_sine(self.mood_blend);
        let (s_from, f_from) = mood_motion_params(self.mood_from);
        let (s_to, f_to) = mood_motion_params(self.mood_to);
        let max_speed = s_from + (s_to - s_from) * me;
        let steer_force = f_from + (f_to - f_from) * me;

        // ── Micro-saccades & dwell: makes "investigate" feel curious ──
        self.saccade_timer -= dt;
        if self.saccade_timer <= 0.0 {
            let s1 = hash_f((t * 977.0) as u32 ^ self.seed);
            let s2 = hash_f(self.seed.wrapping_add(91));
            let s3 = hash_f(self.seed.wrapping_add(92));
            let s4 = hash_f(self.seed.wrapping_add(93));
            let amp = match mood {
                Mood::Serene => 18.0,
                Mood::Alert => 28.0,
                Mood::Stressed => 42.0,
                Mood::Critical => 60.0,
            };
            let ang = s1 * std::f32::consts::TAU;
            self.saccade_x = ang.cos() * amp;
            self.saccade_y = ang.sin() * amp;
            // Dwell more often when calm (the orb "thinks")
            let dwell_chance = match mood {
                Mood::Serene => 0.55,
                Mood::Alert => 0.35,
                Mood::Stressed => 0.15,
                Mood::Critical => 0.05,
            };
            self.dwell_timer = if s2 < dwell_chance {
                0.4 + s3 * 1.2
            } else {
                0.0
            };
            self.saccade_timer = 0.25 + s4 * 0.6;
        }
        self.dwell_timer = (self.dwell_timer - dt).max(0.0);

        self.behavior_timer -= dt;
        if self.behavior_timer <= 0.0 {
            self.pick_behavior(mood, t, w, h);
        }

        // Compute desired velocity based on behavior. Wander/Investigate
        // get a saccade offset so the orb glances around its target.
        let (dx, dy) = match self.behavior {
            OrbBehavior::Wander | OrbBehavior::Investigate => {
                let tx = self.target_x + self.saccade_x;
                let ty = self.target_y + self.saccade_y;
                (tx - self.x, ty - self.y)
            }
            OrbBehavior::Orbit => {
                let orbit_r = match mood {
                    Mood::Serene => 150.0,
                    Mood::Alert => 200.0,
                    Mood::Stressed => 260.0,
                    Mood::Critical => 320.0,
                };
                let orbit_speed = match mood {
                    Mood::Serene => 0.4,
                    Mood::Alert => 0.6,
                    Mood::Stressed => 0.9,
                    Mood::Critical => 1.4,
                };
                self.orbit_angle += orbit_speed * dt;
                let goal_x = self.orbit_cx + self.orbit_angle.cos() * orbit_r;
                let goal_y = self.orbit_cy + self.orbit_angle.sin() * orbit_r;
                (goal_x - self.x, goal_y - self.y)
            }
            OrbBehavior::Rest => {
                // gentle sway around anchor — visible but calm
                let sway_x = self.rest_anchor_x + (t * 0.4).sin() * 40.0 + (t * 0.17).cos() * 25.0;
                let sway_y =
                    self.rest_anchor_y + (t * 0.3 + 1.0).cos() * 35.0 + (t * 0.23).sin() * 20.0;
                (sway_x - self.x, sway_y - self.y)
            }
        };

        let dist = (dx * dx + dy * dy).sqrt().max(0.001);
        let mut desired_speed = max_speed;

        // Dwell: orb pauses briefly to "look" at something.
        if self.dwell_timer > 0.0 {
            desired_speed *= 0.08;
        }

        // Cubic arrive — smoother decel into target than linear arrive.
        let arrive_radius = 140.0;
        if dist < arrive_radius {
            desired_speed *= ease_out_cubic(dist / arrive_radius);
        }

        let desired_vx = (dx / dist) * desired_speed;
        let desired_vy = (dy / dist) * desired_speed;

        // Steering = desired - current, clamped by force
        let mut sx = desired_vx - self.vx;
        let mut sy = desired_vy - self.vy;
        let sm = (sx * sx + sy * sy).sqrt();
        if sm > steer_force {
            sx = sx / sm * steer_force;
            sy = sy / sm * steer_force;
        }

        // Apply steering as acceleration (force/mass, mass=1)
        self.vx += sx * dt * 8.0;
        self.vy += sy * dt * 8.0;

        // Clamp speed
        let spd = (self.vx * self.vx + self.vy * self.vy).sqrt();
        if spd > max_speed {
            self.vx = self.vx / spd * max_speed;
            self.vy = self.vy / spd * max_speed;
        }

        // Soft edge repulsion — keep away from the very edge
        let margin = 0.04; // 4% of screen
        let repulse_strength = max_speed * 4.0;
        if self.x < w * margin {
            self.vx += repulse_strength * (1.0 - self.x / (w * margin)) * dt;
        }
        if self.x > w * (1.0 - margin) {
            self.vx -= repulse_strength * ((self.x - w * (1.0 - margin)) / (w * margin)) * dt;
        }
        if self.y < h * margin {
            self.vy += repulse_strength * (1.0 - self.y / (h * margin)) * dt;
        }
        if self.y > h * (1.0 - margin) {
            self.vy -= repulse_strength * ((self.y - h * (1.0 - margin)) / (h * margin)) * dt;
        }

        // Frame-rate-independent damping (replaces (0.4).powf(dt) drag).
        self.vx = damp(self.vx, 0.0, 0.9, dt);
        self.vy = damp(self.vy, 0.0, 0.9, dt);

        // Integrate position
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Hard clamp as safety net — full screen access
        self.x = self.x.clamp(10.0, w - 10.0);
        self.y = self.y.clamp(10.0, h - 10.0);
    }
}

// ═══════════════════════════════════════════════════════════════
//  Orb Phosphor Trail — fading luminous wake behind the orb
// ═══════════════════════════════════════════════════════════════

pub const TRAIL_LEN: usize = 48;

#[derive(Clone, Copy)]
pub struct TrailPoint {
    pub x: f32,
    pub y: f32,
    pub age: f32,   // seconds since recorded
    pub speed: f32, // orb speed when this point was captured
}

impl Default for TrailPoint {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            age: 0.0,
            speed: 0.0,
        }
    }
}

pub struct OrbTrail {
    pub points: [TrailPoint; TRAIL_LEN],
    pub head: usize,
    pub timer: f32,           // accumulator for spacing samples
    pub sample_interval: f32, // seconds between samples
}

impl OrbTrail {
    pub fn new() -> Self {
        Self {
            points: [TrailPoint::default(); TRAIL_LEN],
            head: 0,
            timer: 0.0,
            sample_interval: 0.03, // ~33 samples/sec
        }
    }

    pub fn update(&mut self, dt: f32, orb_x: f32, orb_y: f32, orb_vx: f32, orb_vy: f32) {
        // Age all existing points
        for p in self.points.iter_mut() {
            p.age += dt;
        }

        // Sample new position at fixed interval
        self.timer += dt;
        if self.timer >= self.sample_interval {
            self.timer = 0.0;
            let speed = (orb_vx * orb_vx + orb_vy * orb_vy).sqrt();
            self.points[self.head] = TrailPoint {
                x: orb_x,
                y: orb_y,
                age: 0.0,
                speed,
            };
            self.head = (self.head + 1) % TRAIL_LEN;
        }
    }

    pub fn draw(&self, d: &mut impl RaylibDraw, teal: Color, icy: Color, h: i32, alpha_mul: f32) {
        let orb_scale = h as f32 / 480.0 * 0.6;
        let max_age = 1.4_f32; // trail fully fades over this many seconds

        // Draw oldest first so newest layers on top
        for i in 0..TRAIL_LEN {
            let idx = (self.head + i) % TRAIL_LEN;
            let p = &self.points[idx];
            if p.age > max_age || p.speed < 15.0 {
                continue;
            }

            let life = 1.0 - (p.age / max_age);
            // Trail intensity: proportional to speed and remaining life
            let speed_factor = (p.speed / 200.0).clamp(0.0, 1.0);
            let alpha = life * life * speed_factor * alpha_mul; // quadratic fade
            if alpha < 0.01 {
                continue;
            }

            let radius = (8.0 + speed_factor * 18.0) * life * orb_scale;
            let a = (alpha * 120.0).min(255.0) as u8;

            // Inner glow — orb's teal/icy color
            d.draw_circle_gradient(
                p.x as i32,
                p.y as i32,
                radius,
                Color::new(teal.r, teal.g, teal.b, a),
                Color::new(0, 0, 0, 0),
            );
            // Outer halo — softer, wider
            if life > 0.3 {
                let halo_a = (alpha * 40.0).min(255.0) as u8;
                d.draw_circle_gradient(
                    p.x as i32,
                    p.y as i32,
                    radius * 1.8,
                    Color::new(icy.r, icy.g, icy.b, halo_a),
                    Color::new(0, 0, 0, 0),
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Orb Emission Particles — luminous motes radiating from the orb
// ═══════════════════════════════════════════════════════════════

pub const ORB_MOTE_MAX: usize = 80;

#[derive(Clone, Copy)]
pub struct OrbMote {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,     // remaining lifetime in seconds
    pub max_life: f32, // total lifetime (for alpha calc)
    pub size: f32,
    pub seed: f32, // per-mote random [0,1]
}

impl Default for OrbMote {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            life: 0.0,
            max_life: 1.0,
            size: 1.0,
            seed: 0.0,
        }
    }
}

pub struct OrbEmitter {
    pub motes: [OrbMote; ORB_MOTE_MAX],
    pub timer: f32,
    pub spawn_counter: u32,
}

impl OrbEmitter {
    pub fn new() -> Self {
        Self {
            motes: [OrbMote::default(); ORB_MOTE_MAX],
            timer: 0.0,
            spawn_counter: 0,
        }
    }

    pub fn update(&mut self, dt: f32, t: f32, orb_x: f32, orb_y: f32, mood: Mood, h: f32) {
        let orb_scale = h / 480.0 * 0.6;

        // Mood-driven parameters
        let (emit_rate, speed_base, speed_var, drift, turbulence, max_life) = match mood {
            Mood::Serene => (3.0, 12.0, 8.0, 0.3, 0.1, 2.8),
            Mood::Alert => (6.0, 20.0, 14.0, 0.5, 0.3, 2.2),
            Mood::Stressed => (10.0, 35.0, 20.0, 0.8, 0.6, 1.6),
            Mood::Critical => (16.0, 55.0, 30.0, 1.2, 1.0, 1.2),
        };

        // Update existing motes
        for m in self.motes.iter_mut() {
            if m.life <= 0.0 {
                continue;
            }
            m.life -= dt;
            // Sinusoidal drift (each mote has unique phase from seed)
            let phase = m.seed * 6.283 + t * (1.5 + m.seed * 2.0);
            m.vx += phase.sin() * drift * dt * 60.0;
            m.vy += phase.cos() * drift * dt * 60.0 - 6.0 * dt; // gentle upward float

            // Turbulence (mood-driven)
            let turb_phase = m.seed * 12.0 + t * 3.5;
            m.vx += turb_phase.cos() * turbulence * dt * 40.0;
            m.vy += turb_phase.sin() * turbulence * dt * 40.0;

            // Drag
            m.vx *= 1.0 - 1.5 * dt;
            m.vy *= 1.0 - 1.5 * dt;

            m.x += m.vx * dt;
            m.y += m.vy * dt;
        }

        // Spawn new motes
        self.timer += dt * emit_rate;
        while self.timer >= 1.0 {
            self.timer -= 1.0;
            // Find a dead slot
            if let Some(m) = self.motes.iter_mut().find(|m| m.life <= 0.0) {
                self.spawn_counter = self.spawn_counter.wrapping_add(1);
                let s = hash_f(self.spawn_counter.wrapping_mul(2654435761));
                let s2 = hash_f(
                    self.spawn_counter
                        .wrapping_mul(1013904223)
                        .wrapping_add(777),
                );
                // Emit from orb surface (random angle)
                let angle = s * std::f32::consts::TAU;
                let radius = (30.0 + s2 * 20.0) * orb_scale;
                let speed = (speed_base + s2 * speed_var) * orb_scale;
                m.x = orb_x + angle.cos() * radius;
                m.y = orb_y + angle.sin() * radius;
                m.vx = angle.cos() * speed;
                m.vy = angle.sin() * speed;
                m.life = max_life * (0.6 + s * 0.4);
                m.max_life = m.life;
                m.size = (2.0 + s2 * 4.0) * orb_scale;
                m.seed = s;
            }
        }
    }

    pub fn draw(
        &self,
        d: &mut impl RaylibDraw,
        teal: Color,
        icy: Color,
        mood: Mood,
        alpha_mul: f32,
    ) {
        // Mood-driven color
        let (r, g, b) = match mood {
            Mood::Serene => (teal.r, teal.g, teal.b),
            Mood::Alert => (
                ((teal.r as f32 * 0.6 + 200.0 * 0.4) as u8),
                ((teal.g as f32 * 0.7 + 180.0 * 0.3) as u8),
                ((teal.b as f32 * 0.8 + 80.0 * 0.2) as u8),
            ),
            Mood::Stressed => (220, 160, 70),
            Mood::Critical => (255, 100, 50),
        };

        for m in &self.motes {
            if m.life <= 0.0 {
                continue;
            }
            let life_frac = (m.life / m.max_life).clamp(0.0, 1.0);
            // Fade in fast, fade out slow (quadratic ease-out)
            let fade_in = ((1.0 - life_frac) * 4.0).min(1.0); // ramps to 1 in first 25%
            let fade_out = life_frac;
            let alpha = fade_in * fade_out * alpha_mul;
            if alpha < 0.01 {
                continue;
            }

            let a = (alpha * 180.0).min(255.0) as u8;
            let size = m.size * (0.5 + life_frac * 0.5); // shrink as they die

            // Core dot
            d.draw_circle_gradient(
                m.x as i32,
                m.y as i32,
                size,
                Color::new(r, g, b, a),
                Color::new(0, 0, 0, 0),
            );
            // Soft halo
            if alpha > 0.15 {
                let ha = (alpha * 50.0).min(255.0) as u8;
                d.draw_circle_gradient(
                    m.x as i32,
                    m.y as i32,
                    size * 2.5,
                    Color::new(icy.r, icy.g, icy.b, ha),
                    Color::new(0, 0, 0, 0),
                );
            }
        }
    }
}
