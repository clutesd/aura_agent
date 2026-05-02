use crate::core::{hash_f, Mood};
use raylib::prelude::*;

// ══════════════════════════════════════════════════════════════════
//  Synaptic Web — neural constellation of consciousness
// ══════════════════════════════════════════════════════════════════
pub const SYNAPSE_MAX_NEURONS: usize = 32;

pub struct Neuron {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub radius: f32,
    pub birth_t: f32, // world-time when born
    pub mood: Mood,
    pub kind_idx: usize, // ThoughtKind ordinal for spatial hashing
    pub alpha: f32,      // 0→1 fade-in, then steady, then fade-out when recycled
    pub pulse: f32,      // 0→1 firing brightness (decays)
}

pub struct Synapse {
    pub a: usize,
    pub b: usize,
    pub strength: f32, // visual thickness / alpha multiplier
    pub fire_t: f32,   // 0..1 energy pulse traveling a→b (negative = inactive)
}

pub struct SynapticWeb {
    pub neurons: Vec<Neuron>,
    pub synapses: Vec<Synapse>,
}

impl SynapticWeb {
    pub fn new() -> Self {
        Self {
            neurons: Vec::new(),
            synapses: Vec::new(),
        }
    }

    /// Add a neuron for a new thought. Position derived from archetype + mood + orb location.
    pub fn add_thought(
        &mut self,
        orb_x: f32,
        orb_y: f32,
        mood: Mood,
        kind_idx: usize,
        t: f32,
        w: f32,
        h: f32,
    ) {
        // Spatial layout: archetype drives x-band, mood drives y-band, orb adds pull
        let kind_f = kind_idx as f32 / 13.0; // 13 archetypes
        let mood_f = match mood {
            Mood::Serene => 0.15,
            Mood::Alert => 0.35,
            Mood::Stressed => 0.55,
            Mood::Critical => 0.70,
        };
        // Base position constrained to upper crown (top ~38% of screen)
        // so the constellation reads as "thoughts hovering above" rather
        // than spilling all over the orb / center stage.
        let jx = hash_f((t * 1000.0) as u32) * 0.12 - 0.06;
        let jy = hash_f((t * 1000.0) as u32 + 777) * 0.06 - 0.03;
        let base_x = w * (0.08 + kind_f * 0.84) + jx * w;
        let base_y = h * (0.05 + mood_f * 0.28) + jy * h;
        // Blend lightly toward orb x for organic clustering, keep y locked up high
        let nx = base_x * 0.78 + orb_x * 0.22;
        let ny = base_y * 0.90 + (orb_y * 0.10).min(h * 0.40);

        let neuron = Neuron {
            x: nx.clamp(30.0, w - 30.0),
            y: ny.clamp(30.0, h * 0.42),
            vx: (hash_f((t * 1000.0) as u32 + 333) - 0.5) * 4.0,
            vy: (hash_f((t * 1000.0) as u32 + 555) - 0.5) * 3.0,
            radius: 2.5 + hash_f((t * 1000.0) as u32 + 999) * 2.0,
            birth_t: t,
            mood,
            kind_idx,
            alpha: 0.0, // fade in
            pulse: 1.0, // born firing
        };
        self.neurons.push(neuron);
        let new_idx = self.neurons.len() - 1;

        // Connect to nearest neurons sharing mood or archetype (max 3 synapses)
        let mut candidates: Vec<(usize, f32)> = Vec::new();
        for (i, n) in self.neurons.iter().enumerate() {
            if i == new_idx {
                continue;
            }
            if n.alpha < 0.05 {
                continue;
            }
            let shared = n.mood as u8 == mood as u8 || n.kind_idx == kind_idx;
            if !shared {
                continue;
            }
            let dx = n.x - nx;
            let dy = n.y - ny;
            let dist = (dx * dx + dy * dy).sqrt();
            candidates.push((i, dist));
        }
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        for &(ci, dist) in candidates.iter().take(3) {
            let strength = (1.0 - dist / (w * 0.7)).max(0.15);
            self.synapses.push(Synapse {
                a: new_idx,
                b: ci,
                strength,
                fire_t: 0.0, // start firing animation
            });
        }

        // FIFO eviction — fade out oldest
        if self.neurons.len() > SYNAPSE_MAX_NEURONS {
            self.neurons[0].alpha = -0.01; // mark for death
        }
    }

    pub fn update(&mut self, dt: f32, w: f32, h: f32) {
        // Update neurons
        for n in &mut self.neurons {
            // Fade in
            if n.alpha >= 0.0 && n.alpha < 1.0 {
                n.alpha = (n.alpha + dt * 1.2).min(1.0);
            }
            // Fade out (marked for death)
            if n.alpha < 0.0 {
                n.alpha -= dt * 0.5;
            }
            // Pulse decay
            n.pulse = (n.pulse - dt * 0.8).max(0.0);

            // Gentle drift with boundary soft-repulsion
            n.x += n.vx * dt;
            n.y += n.vy * dt;
            // Damping
            n.vx *= 1.0 - dt * 0.3;
            n.vy *= 1.0 - dt * 0.3;
            // Gentle random perturbation
            let jt = n.birth_t * 100.0 + n.x;
            n.vx += (jt * 0.7).sin() * dt * 2.0;
            n.vy += (jt * 0.5 + 1.0).cos() * dt * 1.5;
            // Soft boundary repulsion — keep neurons in the upper crown
            if n.x < 40.0 {
                n.vx += dt * 8.0;
            }
            if n.x > w - 40.0 {
                n.vx -= dt * 8.0;
            }
            if n.y < 30.0 {
                n.vy += dt * 6.0;
            }
            if n.y > h * 0.42 {
                n.vy -= dt * 10.0;
            }
        }

        // Update synapse firing animations
        for s in &mut self.synapses {
            if s.fire_t >= 0.0 && s.fire_t < 1.0 {
                s.fire_t += dt * 1.5; // travel speed
            }
        }

        // Remove dead neurons and fix synapse indices
        let mut removals: Vec<usize> = Vec::new();
        for (i, n) in self.neurons.iter().enumerate() {
            if n.alpha < -1.0 {
                removals.push(i);
            }
        }
        for &ri in removals.iter().rev() {
            self.neurons.remove(ri);
            // Fix synapse indices and remove broken ones
            self.synapses.retain(|s| s.a != ri && s.b != ri);
            for s in &mut self.synapses {
                if s.a > ri {
                    s.a -= 1;
                }
                if s.b > ri {
                    s.b -= 1;
                }
            }
        }
        // Cap synapses to avoid unbounded growth
        while self.synapses.len() > SYNAPSE_MAX_NEURONS * 4 {
            self.synapses.remove(0);
        }
    }

    pub fn draw(&self, d: &mut impl RaylibDraw, _mood: Mood, t: f32, alpha_mul: f32) {
        // Draw synapses — luminous arcs between connected neurons
        for s in &self.synapses {
            if s.a >= self.neurons.len() || s.b >= self.neurons.len() {
                continue;
            }
            let na = &self.neurons[s.a];
            let nb = &self.neurons[s.b];
            let pair_alpha = na.alpha.max(0.0) * nb.alpha.max(0.0);
            if pair_alpha < 0.02 {
                continue;
            }

            let sa = (s.strength * pair_alpha * 50.0 * alpha_mul) as u8;
            if sa < 2 {
                continue;
            }

            // Color blended from both neuron moods
            let ca = Self::mood_rgb(na.mood);
            let cb = Self::mood_rgb(nb.mood);
            let cr = ((ca.0 as u16 + cb.0 as u16) / 2) as u8;
            let cg = ((ca.1 as u16 + cb.1 as u16) / 2) as u8;
            let cb_c = ((ca.2 as u16 + cb.2 as u16) / 2) as u8;

            // Draw as a thin line (synapse arc)
            d.draw_line_ex(
                rvec2(na.x, na.y),
                rvec2(nb.x, nb.y),
                (0.5 + s.strength).min(1.5),
                Color::new(cr, cg, cb_c, sa),
            );

            // Firing pulse — bright dot traveling along the synapse
            if s.fire_t >= 0.0 && s.fire_t < 1.0 {
                let ft = s.fire_t;
                let px = na.x + (nb.x - na.x) * ft;
                let py = na.y + (nb.y - na.y) * ft;
                let fire_a = ((1.0 - (ft - 0.5).abs() * 2.0) * 200.0 * alpha_mul) as u8;
                d.draw_circle(px as i32, py as i32, 2.5, Color::new(255, 255, 240, fire_a));
            }
        }

        // Draw neurons — glowing nodes
        for n in &self.neurons {
            let a = n.alpha.max(0.0);
            if a < 0.02 {
                continue;
            }

            let c = Self::mood_rgb(n.mood);
            // Outer halo
            let halo_a = (a * (30.0 + n.pulse * 80.0) * alpha_mul) as u8;
            let halo_r = n.radius * (2.5 + n.pulse * 1.5);
            d.draw_circle(
                n.x as i32,
                n.y as i32,
                halo_r,
                Color::new(c.0, c.1, c.2, halo_a),
            );
            // Core
            let core_a = (a * (120.0 + n.pulse * 135.0) * alpha_mul).min(255.0) as u8;
            d.draw_circle(
                n.x as i32,
                n.y as i32,
                n.radius,
                Color::new(c.0, c.1, c.2, core_a),
            );
            // Hot center on birth pulse
            if n.pulse > 0.3 {
                let hot_a = (n.pulse * 180.0 * alpha_mul) as u8;
                d.draw_circle(
                    n.x as i32,
                    n.y as i32,
                    n.radius * 0.5,
                    Color::new(255, 255, 240, hot_a),
                );
            }

            // Gentle breathing sine
            let _breath = (t * 1.5 + n.birth_t * 10.0).sin() * 0.3 + 0.7;
        }
    }

    pub fn mood_rgb(mood: Mood) -> (u8, u8, u8) {
        match mood {
            Mood::Serene => (80, 220, 210),
            Mood::Alert => (220, 190, 80),
            Mood::Stressed => (240, 140, 60),
            Mood::Critical => (255, 70, 70),
        }
    }

    // ── Persistence ────────────────────────────────────────────
    // The synaptic web is the AI's accumulating record of its own
    // cognition. Persisting it across restarts gives the consciousness
    // genuine continuity: thoughts from yesterday's run remain visible
    // as faint, settled neurons among today's fresh ones.

    /// Default location for the persisted constellation:
    /// `$HOME/.aurora/synaptic_state.json` (falls back to `/tmp/...`).
    pub fn default_save_path() -> std::path::PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            std::path::PathBuf::from(home)
                .join(".aurora")
                .join("synaptic_state.json")
        } else {
            std::path::PathBuf::from("/tmp/aurora_synaptic_state.json")
        }
    }

    /// Serialize the current web (neurons + synapses) to disk as JSON.
    /// We rebase `birth_t` to 0 so freshly-loaded neurons settle visibly
    /// rather than carrying stale absolute timestamps from the previous
    /// process. Returns the number of neurons written, or io error.
    pub fn save_to_disk<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<usize> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Find the youngest birth_t so we can rebase the timeline.
        let max_birth = self
            .neurons
            .iter()
            .map(|n| n.birth_t)
            .fold(f32::NEG_INFINITY, f32::max);
        let base = if max_birth.is_finite() {
            max_birth
        } else {
            0.0
        };

        let neurons: Vec<_> = self
            .neurons
            .iter()
            .map(|n| SerNeuron {
                x: n.x,
                y: n.y,
                vx: n.vx * 0.3, // damp velocity so reloaded web doesn't fling
                vy: n.vy * 0.3,
                radius: n.radius,
                // Rebase: oldest neuron becomes most-negative, youngest = 0.
                birth_t: n.birth_t - base,
                mood: n.mood,
                kind_idx: n.kind_idx,
                // Force fully faded-in on reload; no in-flight death.
                alpha: n.alpha.clamp(0.0, 1.0).max(0.5),
                pulse: 0.0,
            })
            .collect();

        let synapses: Vec<_> = self
            .synapses
            .iter()
            .map(|s| SerSynapse {
                a: s.a,
                b: s.b,
                strength: s.strength,
            })
            .collect();

        let count = neurons.len();
        let snapshot = SerWeb { neurons, synapses };
        let json = serde_json::to_string(&snapshot)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        // Atomic write via temp + rename.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(count)
    }

    /// Restore a previously-saved constellation. Returns Some(self) only
    /// on a clean parse; any error → None (callers start fresh).
    pub fn load_from_disk<P: AsRef<std::path::Path>>(path: P) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        let snapshot: SerWeb = serde_json::from_slice(&bytes).ok()?;
        let neurons: Vec<Neuron> = snapshot
            .neurons
            .into_iter()
            .map(|s| Neuron {
                x: s.x,
                y: s.y,
                vx: s.vx,
                vy: s.vy,
                radius: s.radius,
                birth_t: s.birth_t,
                mood: s.mood,
                kind_idx: s.kind_idx,
                alpha: s.alpha,
                pulse: s.pulse,
            })
            .collect();
        // Drop any synapse referencing an out-of-range neuron, in case the
        // save file has been hand-edited or partially corrupted.
        let n_count = neurons.len();
        let synapses: Vec<Synapse> = snapshot
            .synapses
            .into_iter()
            .filter(|s| s.a < n_count && s.b < n_count && s.a != s.b)
            .map(|s| Synapse {
                a: s.a,
                b: s.b,
                strength: s.strength,
                fire_t: 1.0,
            })
            .collect();
        Some(Self { neurons, synapses })
    }
}

// Serializable mirrors of Neuron/Synapse — kept private and minimal so
// in-memory representations stay free of serde noise.
#[derive(serde::Serialize, serde::Deserialize)]
struct SerNeuron {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
    birth_t: f32,
    mood: Mood,
    kind_idx: usize,
    alpha: f32,
    pulse: f32,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct SerSynapse {
    a: usize,
    b: usize,
    strength: f32,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct SerWeb {
    neurons: Vec<SerNeuron>,
    synapses: Vec<SerSynapse>,
}

// ══════════════════════════════════════════════════════════════════
//  Thought Pulse — radial shockwave from the orb
// ══════════════════════════════════════════════════════════════════
pub struct ThoughtPulse {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub alpha: f32,
}

impl ThoughtPulse {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            radius: 0.0,
            alpha: 1.0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.radius += dt * 320.0; // expand speed
        self.alpha = (1.0 - self.radius / 900.0).max(0.0);
    }

    pub fn alive(&self) -> bool {
        self.alpha > 0.01
    }
}
