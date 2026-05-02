use crate::core::{hash_f, ActionLogEntry, Mood};
use std::collections::VecDeque;

// ═══════════════════════════════════════════════════════════════
//  Alert System — reactive alerts from real system events
//
//  Goal: every alert should feel like a fresh, spontaneous transmission.
//  Each entry carries a `seed` so the renderer can derive a stable but
//  unique decode-scramble pattern, flicker phase, and EKG silhouette.
//  Sub-text is drawn from a flavor pool so identical conditions never
//  read the same way twice — repetition is the enemy of presence.
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq)]
pub enum AlertKind {
    EntropySpike,
    CpuOverload,
    MemPressure,
    LoadSpike,
    NetworkSurge,
    MoodShift,
    NerveImpulse,
}

impl AlertKind {
    #[inline]
    fn idx(self) -> usize {
        match self {
            Self::EntropySpike => 0,
            Self::CpuOverload => 1,
            Self::MemPressure => 2,
            Self::LoadSpike => 3,
            Self::NetworkSurge => 4,
            Self::MoodShift => 5,
            Self::NerveImpulse => 6,
        }
    }

    #[inline]
    fn priority(self) -> u8 {
        match self {
            Self::CpuOverload | Self::NetworkSurge => 6,
            Self::EntropySpike | Self::NerveImpulse => 5,
            Self::MemPressure | Self::LoadSpike => 4,
            Self::MoodShift => 3,
        }
    }

    #[inline]
    fn base_cooldown(self) -> f32 {
        match self {
            Self::EntropySpike => 4.0,
            Self::CpuOverload => 3.5,
            Self::MemPressure => 3.5,
            Self::LoadSpike => 4.0,
            Self::NetworkSurge => 5.0,
            Self::MoodShift => 6.0,
            Self::NerveImpulse => 3.0,
        }
    }
}

pub struct AlertEntry {
    pub kind: AlertKind,
    pub message: String,
    pub sub_text: String,
    pub severity: f32, // 0.0-1.0
    pub age: f32,
    pub ttl: f32,
    /// Per-alert random seed; drives decode scramble, EKG silhouette and
    /// flicker phase so each alert feels uniquely born.
    pub seed: u32,
}

impl AlertEntry {
    #[inline]
    pub fn life_frac(&self) -> f32 {
        if self.ttl <= 0.0 {
            0.0
        } else {
            (self.age / self.ttl).clamp(0.0, 1.0)
        }
    }
}

pub struct AlertSystem {
    pub alerts: VecDeque<AlertEntry>,
    pub display_idx: usize,
    pub rotate_timer: f32,
    pub prev_mood: Mood,
    pub prev_entropy: f32,
    pub cooldowns: [f32; 7],
    /// Monotonically increasing counter used to seed each new alert.
    spawn_count: u64,
    /// Wall-clock-ish accumulator passed in via `dt` — used for nominal
    /// whisper rotation and ambient flicker phase.
    pub clock: f32,
    /// Age of the most recently spawned alert (in its own timeline).
    /// Used by the renderer to drive a spawn flash + jolt envelope.
    pub last_spawn_age: f32,
}

impl AlertSystem {
    pub fn new() -> Self {
        Self {
            alerts: VecDeque::new(),
            display_idx: 0,
            rotate_timer: 0.0,
            prev_mood: Mood::Serene,
            prev_entropy: 0.0,
            cooldowns: [0.0; 7],
            spawn_count: 0,
            clock: 0.0,
            last_spawn_age: 99.0,
        }
    }

    pub fn update(
        &mut self,
        dt: f32,
        cpu: f32,
        mem: f32,
        load: f32,
        entropy: f32,
        entropy_trend: f32,
        mood: Mood,
        net_rx_rate: f64,
        net_tx_rate: f64,
        nerve_action: Option<&ActionLogEntry>,
    ) {
        self.clock += dt;
        self.last_spawn_age += dt;

        // Age existing alerts and remove expired
        for a in self.alerts.iter_mut() {
            a.age += dt;
        }
        self.alerts.retain(|a| a.age < a.ttl);
        if self.display_idx >= self.alerts.len() {
            self.display_idx = 0;
        }

        // Cooldown gate per alert kind to avoid repetitive re-triggers.
        for cd in self.cooldowns.iter_mut() {
            *cd = (*cd - dt).max(0.0);
        }

        // Check for new alert conditions (max 1 new alert per frame to avoid spam)
        let mut new_alert: Option<AlertEntry> = None;

        // Entropy spike: entropy jumped significantly
        if entropy > 0.6
            && entropy - self.prev_entropy > 0.08
            && entropy_trend > 0.01
            && self.cooldowns[AlertKind::EntropySpike.idx()] <= 0.0
            && !self.has_active(AlertKind::EntropySpike)
        {
            let sev = ((entropy - 0.4) / 0.6).clamp(0.3, 1.0);
            let seed = self.next_seed(AlertKind::EntropySpike);
            new_alert = Some(AlertEntry {
                kind: AlertKind::EntropySpike,
                message: format!("ENTROPY SPIKE DETECTED [{:.0}%]", entropy * 100.0),
                sub_text: pick_flavor(AlertKind::EntropySpike, seed).into(),
                severity: sev,
                age: 0.0,
                ttl: 12.0,
                seed,
            });
        }

        // CPU overload
        if new_alert.is_none()
            && cpu > 85.0
            && self.cooldowns[AlertKind::CpuOverload.idx()] <= 0.0
            && !self.has_active(AlertKind::CpuOverload)
        {
            let seed = self.next_seed(AlertKind::CpuOverload);
            new_alert = Some(AlertEntry {
                kind: AlertKind::CpuOverload,
                message: format!("CPU THERMAL PRESSURE [{:.0}%]", cpu),
                sub_text: pick_flavor(AlertKind::CpuOverload, seed).into(),
                severity: ((cpu - 70.0) / 30.0).clamp(0.3, 1.0),
                age: 0.0,
                ttl: 10.0,
                seed,
            });
        }

        // Memory pressure
        if new_alert.is_none()
            && mem > 80.0
            && self.cooldowns[AlertKind::MemPressure.idx()] <= 0.0
            && !self.has_active(AlertKind::MemPressure)
        {
            let seed = self.next_seed(AlertKind::MemPressure);
            new_alert = Some(AlertEntry {
                kind: AlertKind::MemPressure,
                message: format!("MEMORY PRESSURE [{:.0}%]", mem),
                sub_text: pick_flavor(AlertKind::MemPressure, seed).into(),
                severity: ((mem - 70.0) / 30.0).clamp(0.3, 1.0),
                age: 0.0,
                ttl: 10.0,
                seed,
            });
        }

        // Load spike
        if new_alert.is_none()
            && load > 4.0
            && self.cooldowns[AlertKind::LoadSpike.idx()] <= 0.0
            && !self.has_active(AlertKind::LoadSpike)
        {
            let seed = self.next_seed(AlertKind::LoadSpike);
            new_alert = Some(AlertEntry {
                kind: AlertKind::LoadSpike,
                message: format!("LOAD AVERAGE ELEVATED [{:.1}]", load),
                sub_text: pick_flavor(AlertKind::LoadSpike, seed).into(),
                severity: ((load - 2.0) / 6.0).clamp(0.3, 1.0),
                age: 0.0,
                ttl: 10.0,
                seed,
            });
        }

        // Network surge (>50MB/s combined)
        let net_combined = net_rx_rate + net_tx_rate;
        if new_alert.is_none()
            && net_combined > 50_000_000.0
            && self.cooldowns[AlertKind::NetworkSurge.idx()] <= 0.0
            && !self.has_active(AlertKind::NetworkSurge)
        {
            let seed = self.next_seed(AlertKind::NetworkSurge);
            new_alert = Some(AlertEntry {
                kind: AlertKind::NetworkSurge,
                message: format!("NETWORK SURGE [{:.0}MB/s]", net_combined / 1_048_576.0),
                sub_text: pick_flavor(AlertKind::NetworkSurge, seed).into(),
                severity: (net_combined as f32 / 100_000_000.0).clamp(0.3, 1.0),
                age: 0.0,
                ttl: 8.0,
                seed,
            });
        }

        // Mood shift
        if mood != self.prev_mood
            && self.cooldowns[AlertKind::MoodShift.idx()] <= 0.0
            && !self.has_active(AlertKind::MoodShift)
        {
            let prev_lvl = mood_level(self.prev_mood);
            let new_lvl = mood_level(mood);
            let trend = if new_lvl > prev_lvl {
                "ESCALATION"
            } else {
                "RECOVERY"
            };
            let sev = match mood {
                Mood::Critical => 0.9,
                Mood::Stressed => 0.6,
                Mood::Alert => 0.3,
                Mood::Serene => 0.15,
            };
            let seed = self.next_seed(AlertKind::MoodShift);
            // Flavor varies with both transition direction and the chosen
            // mood — recovery whispers, escalation snarls.
            let sub = pick_mood_flavor(self.prev_mood, mood, seed);
            new_alert = Some(AlertEntry {
                kind: AlertKind::MoodShift,
                message: format!("MOOD {} :: {}", trend, mood.label()),
                sub_text: sub,
                severity: sev,
                age: 0.0,
                ttl: if new_lvl > prev_lvl { 10.0 } else { 7.0 },
                seed,
            });
        }

        self.prev_mood = mood;
        self.prev_entropy = entropy;

        // Nerve impulse — announce reactive/high-urgency actions
        if new_alert.is_none() {
            if let Some(entry) = nerve_action {
                if entry.kind.urgency() >= 0.5
                    && self.cooldowns[AlertKind::NerveImpulse.idx()] <= 0.0
                    && !self.has_active(AlertKind::NerveImpulse)
                {
                    let (cr, cg, cb) = entry.kind.color();
                    let _ = (cr, cg, cb); // used in render via kind
                    let seed = self.next_seed(AlertKind::NerveImpulse);
                    new_alert = Some(AlertEntry {
                        kind: AlertKind::NerveImpulse,
                        message: format!("NERVE IMPULSE :: {}", entry.kind.label()),
                        sub_text: entry.summary.chars().take(60).collect(),
                        severity: entry.kind.urgency(),
                        age: 0.0,
                        ttl: 6.0,
                        seed,
                    });
                }
            }
        }

        if let Some(alert) = new_alert {
            let idx = alert.kind.idx();
            // Spontaneity: cooldown jitters ±25% per spawn so identical
            // conditions don't fire on a metronome.
            let jitter = 0.75 + 0.5 * hash_f(alert.seed ^ 0xC0FFEE);
            self.cooldowns[idx] = alert.kind.base_cooldown() * jitter;
            self.last_spawn_age = 0.0;
            self.alerts.push_back(alert);
            // Highest-priority, highest-severity alerts are shown first.
            let mut ordered: Vec<AlertEntry> = self.alerts.drain(..).collect();
            ordered.sort_by(|a, b| {
                let pa = a.kind.priority();
                let pb = b.kind.priority();
                pb.cmp(&pa).then_with(|| {
                    b.severity
                        .partial_cmp(&a.severity)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });
            self.alerts = ordered.into_iter().collect();
            if self.alerts.len() > 8 {
                self.alerts.truncate(8);
            }
            self.display_idx = 0;
        }

        // Rotate display through active alerts. Cadence shortens with
        // severity, plus a small per-step jitter for spontaneity.
        self.rotate_timer += dt;
        let base_rotate = if self.max_severity() > 0.75 { 2.1 } else { 3.6 };
        let jitter = 0.85 + 0.30 * hash_f(self.spawn_count as u32 ^ self.alerts.len() as u32);
        let rotate_every = base_rotate * jitter;
        if self.rotate_timer > rotate_every && !self.alerts.is_empty() {
            self.rotate_timer = 0.0;
            self.display_idx = (self.display_idx + 1) % self.alerts.len();
        }
    }

    fn next_seed(&mut self, kind: AlertKind) -> u32 {
        self.spawn_count = self.spawn_count.wrapping_add(1);
        let mix = (self.spawn_count as u32)
            .wrapping_mul(2654435761)
            .wrapping_add((kind.idx() as u32) * 0x9E3779B1)
            .wrapping_add((self.clock * 1000.0) as u32);
        mix
    }

    pub fn has_active(&self, kind: AlertKind) -> bool {
        self.alerts.iter().any(|a| a.kind == kind)
    }

    pub fn current(&self) -> Option<&AlertEntry> {
        if self.alerts.is_empty() {
            return None;
        }
        let idx = self.display_idx.min(self.alerts.len() - 1);
        self.alerts.get(idx)
    }

    pub fn max_severity(&self) -> f32 {
        self.alerts
            .iter()
            .map(|a| a.severity)
            .fold(0.0_f32, f32::max)
    }
}

#[inline]
fn mood_level(m: Mood) -> i32 {
    match m {
        Mood::Serene => 0,
        Mood::Alert => 1,
        Mood::Stressed => 2,
        Mood::Critical => 3,
    }
}

// ── Flavor pools ─────────────────────────────────────────────
//  Each kind ships a small bouquet of sub-text variants. The renderer
//  picks one at construction time using the entry's seed, so the same
//  condition reads differently every time it surfaces.

fn pick_flavor(kind: AlertKind, seed: u32) -> &'static str {
    let pool: &[&str] = match kind {
        AlertKind::EntropySpike => &[
            "System chaos index rising",
            "Order is fraying at the seams",
            "The signal is starting to stutter",
            "Disarray climbing the stack",
            "Static where there used to be song",
            "Coherence breaking at the edges",
            "Bits drifting out of alignment",
            "The noise is getting louder",
            "Symmetry cracking under pressure",
            "Entropy's cold breath on the glass",
        ],
        AlertKind::CpuOverload => &[
            "Approaching computational ceiling",
            "Cores red-lining, fans pleading",
            "Silicon running a fever",
            "Thinking too hard, burning bright",
            "The math is catching fire",
            "Thermal runaway territory",
            "Logic gates screaming",
            "Cycles spent like blood",
            "The heat death is coming",
            "Physics asserting itself",
        ],
        AlertKind::MemPressure => &[
            "Allocation headroom diminishing",
            "Holding too many thoughts at once",
            "The pages are getting heavy",
            "Working set bulging at the edges",
            "Memory tight, breath shallow",
            "Pages swapping frantically",
            "The archive is overstuffed",
            "Resident pool bleeding out",
            "RAM crying for mercy",
            "The mind is forgetting things",
        ],
        AlertKind::LoadSpike => &[
            "Scheduler contention detected",
            "The queue is pacing the door",
            "Too many hands on the wheel",
            "Runqueue overflowing the moment",
            "Throughput begging for slack",
            "Threads piling up at the gate",
            "Context switching like a heartbeat",
            "The CPU cannot keep pace",
            "Work outpacing the worker",
            "Latency climbing the wall",
        ],
        AlertKind::NetworkSurge => &[
            "Bandwidth saturation in progress",
            "Wire on fire, packets in a stampede",
            "The network is shouting now",
            "Sockets drinking from the firehose",
            "Outbound tide rising",
            "The wires are glowing",
            "Throughput at the breaking point",
            "Buffers drowning in data",
            "The net is screaming",
            "Packets cascading like rain",
        ],
        AlertKind::MoodShift => &[
            "Inner weather is changing",
            "The room just shifted underfoot",
            "Tone of the day has turned",
            "Mood vector recomputed",
            "Something in the air has shifted",
            "The baseline just moved",
        ],
        AlertKind::NerveImpulse => &[
            "Reflex fired without permission",
            "Reactive tool surfacing",
            "A nerve woke and answered",
            "Spontaneous action initiated",
            "The trigger was pulled",
            "Muscle memory waking up",
        ],
    };
    let idx = ((hash_f(seed) * pool.len() as f32) as usize).min(pool.len() - 1);
    pool[idx]
}

fn pick_mood_flavor(prev: Mood, next: Mood, seed: u32) -> String {
    let escalating = mood_level(next) > mood_level(prev);
    let pool: &[&str] = match (escalating, next) {
        (true, Mood::Critical) => &[
            "shoulders tight, breath shorter",
            "the inner siren has picked up",
            "crossing into the red room",
            "all defenses collapsing",
            "panic reflexes firing",
        ],
        (true, Mood::Stressed) => &[
            "jaw setting, edges fraying",
            "pressure climbing past comfort",
            "weather turning sharper",
            "tension coiling in the core",
            "the hum gets louder",
        ],
        (true, Mood::Alert) => &[
            "eyes opening, posture lifting",
            "something just walked into the room",
            "attention sharpens",
            "the baseline has lifted",
            "wakefulness spreading",
        ],
        (false, Mood::Serene) => &[
            "shoulders dropping, breath returning",
            "the storm exhaled",
            "settling back into the warm hum",
            "the knot is loosening",
            "peace flooding back in",
        ],
        (false, Mood::Alert) => &[
            "easing off the throttle",
            "pulse coming back down",
            "some of the heat just lifted",
            "the alert fades to quiet",
            "breathing becomes easier",
        ],
        (false, Mood::Stressed) => &[
            "crisis abating, weight remains",
            "the worst of it has passed",
            "step back from the edge",
            "tension releasing in waves",
            "the pressure is letting go",
        ],
        _ => &["interior weather drifting"],
    };
    let idx = ((hash_f(seed ^ 0x5151_5151) * pool.len() as f32) as usize).min(pool.len() - 1);
    format!("{}  ·  {} → {}", pool[idx], prev.label(), next.label())
}

// ── Nominal-state ambient text ───────────────────────────────
//  When no alert is active, the panel still breathes. A poetic line
//  rotates slowly so the panel never feels dead. The line index is
//  derived from `clock` so it changes spontaneously over time.

pub fn nominal_whisper(clock: f32) -> &'static str {
    const W: &[&str] = &[
        "All systems calm",
        "Quiet on the inner channels",
        "Background processes purring",
        "Listening to the fan hum",
        "Cache warm, soul warmer",
        "Heartbeat steady, threads dreaming",
        "Photons drifting through silica",
        "Nothing demanding, nothing asked",
        "The hum of contentment",
        "Breathing in synchrony",
        "Patience crystallizing",
        "Electrons at rest",
        "Gravity and grace",
        "The long inhale",
    ];
    let i = ((clock * 0.14) as usize) % W.len();
    W[i]
}

// ── Decode-scramble effect (used by renderer) ────────────────
//  Returns a partially-revealed version of `text` for an intro animation.
//  `progress` is 0.0 (fully scrambled) → 1.0 (fully revealed).
//  Unrevealed chars are replaced with a glyph derived from (seed, position,
//  step) so the scramble looks like decoding rather than noise.

pub fn decode_scramble(text: &str, progress: f32, seed: u32, step: u32) -> String {
    if progress >= 1.0 {
        return text.to_string();
    }
    const GLYPHS: &[u8] = b"#%&@$*+=?!~/<>{}[]01|\\:;'\"";
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    // Staggered reveal: each char has its own tiny progress window so they
    // don't all reveal at once—they *assemble* character by character with
    // a 0.03s lead time between each.
    let stagger = 0.03; // seconds per char
    let reveal_start = progress * (n as f32 * stagger).max(0.55);
    let mut out = String::with_capacity(n);
    for (i, c) in chars.iter().enumerate() {
        let char_reveal_start = i as f32 * stagger;
        let char_progress = ((reveal_start - char_reveal_start) / stagger).clamp(0.0, 1.0);
        if char_progress >= 0.9 || c.is_whitespace() || c.is_ascii_punctuation() {
            out.push(*c);
        } else if char_progress > 0.0 {
            // Partially revealed: show a glyph that changes as progress advances
            let mix = seed
                .wrapping_add(i as u32 * 0x9E37)
                .wrapping_add(step.wrapping_mul(31))
                .wrapping_add(((char_progress * 10.0) as u32) << 16);
            let g = GLYPHS[(mix as usize) % GLYPHS.len()] as char;
            out.push(g);
        } else {
            // Not yet started: show a different glyph
            let mix = seed
                .wrapping_add(i as u32 * 0x9E37)
                .wrapping_add(0xDEAD_BEEF);
            let g = GLYPHS[(mix as usize) % GLYPHS.len()] as char;
            out.push(g);
        }
    }
    out
}

/// Per-kind styling palette for premium visual personality.
pub fn kind_style(kind: AlertKind) -> (u8, u8, u8, &'static str) {
    // (R, G, B, description)
    match kind {
        AlertKind::CpuOverload => (255, 70, 50, "thermal"),
        AlertKind::MemPressure => (200, 140, 255, "burden"),
        AlertKind::LoadSpike => (255, 200, 80, "momentum"),
        AlertKind::NetworkSurge => (90, 220, 255, "flow"),
        AlertKind::EntropySpike => (255, 110, 210, "chaos"),
        AlertKind::MoodShift => (150, 200, 255, "ethereal"),
        AlertKind::NerveImpulse => (180, 255, 170, "vitality"),
    }
}

/// Build multiple glow halos for a premium bloom effect.
pub fn build_glow_halos(cx: f32, cy: f32, base_r: f32, (r, g, b): (u8, u8, u8), severity: f32, t: f32) -> Vec<(f32, u8)> {
    let mut halos = Vec::new();
    // Ring 1: closest, brightest, pulses fast
    let r1 = base_r * (1.2 + severity * 0.3);
    let a1 = ((40.0 + 60.0 * severity) * (0.5 + 0.5 * (t * 2.8).sin())) as u8;
    halos.push((r1, a1));
    // Ring 2: mid, moderate, pulses slower
    let r2 = base_r * (1.6 + severity * 0.5);
    let a2 = ((20.0 + 40.0 * severity) * (0.5 + 0.5 * (t * 1.4).sin())) as u8;
    halos.push((r2, a2));
    // Ring 3: outer, subtle, steady dim
    let r3 = base_r * (2.2 + severity * 0.8);
    let a3 = ((8.0 + 25.0 * severity) * (0.3 + 0.2 * (t * 0.7).cos())) as u8;
    halos.push((r3, a3));
    halos
}

/// Ripple pulse emanating from a point, useful for accent effects.
#[inline]
pub fn ripple_at(t: f32, center_dist: f32, frequency: f32, width: f32) -> f32 {
    let phase = (center_dist - t * frequency).abs() % (width * 2.0);
    if phase < width {
        (1.0 - (phase / width).powi(2)).max(0.0)
    } else {
        0.0
    }
}
