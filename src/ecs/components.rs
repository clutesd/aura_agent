#![allow(dead_code)] // Phase 3-6 scaffolding — see /memories/repo/aura_agent_architecture.md

//! ECS Component definitions — pure data, no logic.
//!
//! All components are stored in Structure-of-Arrays (SoA) layout
//! inside `World` for cache-friendly system iteration.

use crate::core::Mood;

// ═══════════════════════════════════════════════════════════════
//  Transform Components
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, Default)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Velocity {
    pub vx: f32,
    pub vy: f32,
}

// ═══════════════════════════════════════════════════════════════
//  Mood Component
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug)]
pub struct MoodComp {
    pub mood: Mood,
}

// ═══════════════════════════════════════════════════════════════
//  Context Steering Component
// ═══════════════════════════════════════════════════════════════

/// Number of radial directions evaluated by the context steering system.
/// 16 directions gives 22.5° resolution — good balance of precision and cost.
pub const STEERING_RESOLUTION: usize = 16;

#[derive(Clone, Debug)]
pub struct SteeringComp {
    /// Interest map: how desirable each direction is (0.0 = ignore, 1.0 = max).
    pub interest: [f32; STEERING_RESOLUTION],
    /// Danger map: how dangerous each direction is (0.0 = safe, 1.0 = fatal).
    pub danger: [f32; STEERING_RESOLUTION],
    /// Final chosen direction after masking and weighting (unit vector).
    pub chosen_dir: (f32, f32),
    /// Maximum speed the entity is currently allowed (px/s).
    pub max_speed: f32,
    /// Maximum steering force (px/s²).
    pub steer_force: f32,
}

impl Default for SteeringComp {
    fn default() -> Self {
        Self {
            interest: [0.0; STEERING_RESOLUTION],
            danger: [0.0; STEERING_RESOLUTION],
            chosen_dir: (0.0, 0.0),
            max_speed: 150.0,
            steer_force: 220.0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Orb State Component
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrbBehavior {
    Wander,
    Investigate,
    Orbit,
    Rest,
}

#[derive(Clone, Debug)]
pub struct OrbStateComp {
    pub behavior: OrbBehavior,
    pub behavior_timer: f32,
    pub seed: u32,
    // Wander / Investigate target
    pub target_x: f32,
    pub target_y: f32,
    // Orbit params
    pub orbit_angle: f32,
    pub orbit_cx: f32,
    pub orbit_cy: f32,
    // Rest anchor
    pub rest_anchor_x: f32,
    pub rest_anchor_y: f32,
}

impl OrbStateComp {
    pub fn new(cx: f32, cy: f32) -> Self {
        Self {
            behavior: OrbBehavior::Wander,
            behavior_timer: 0.5,
            seed: 42,
            target_x: cx,
            target_y: cy,
            orbit_angle: 0.0,
            orbit_cx: cx,
            orbit_cy: cy,
            rest_anchor_x: cx,
            rest_anchor_y: cy,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Render Tag
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderTag {
    /// The main orb entity — rendered via SDF shader.
    Orb,
    /// A mote particle — rendered via GPU instancing.
    Mote,
    /// A trail point — rendered via ribbon mesh.
    TrailPoint,
    /// A weather particle.
    WeatherParticle,
}

// ═══════════════════════════════════════════════════════════════
//  Trail Component
// ═══════════════════════════════════════════════════════════════

pub const TRAIL_LEN: usize = 48;

#[derive(Clone, Copy, Debug, Default)]
pub struct TrailSample {
    pub x: f32,
    pub y: f32,
    pub age: f32,
    pub speed: f32,
}

#[derive(Clone, Debug)]
pub struct TrailComp {
    pub points: [TrailSample; TRAIL_LEN],
    pub head: usize,
    pub timer: f32,
    pub sample_interval: f32,
}

impl Default for TrailComp {
    fn default() -> Self {
        Self {
            points: [TrailSample::default(); TRAIL_LEN],
            head: 0,
            timer: 0.0,
            sample_interval: 0.03,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Emitter Component
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct EmitterComp {
    pub rate: f32,        // particles per second
    pub base_speed: f32,  // px/s
    pub turbulence: f32,  // 0.0–1.0
    pub lifetime: f32,    // seconds
    pub spawn_timer: f32, // accumulator
}

impl Default for EmitterComp {
    fn default() -> Self {
        Self {
            rate: 3.0,
            base_speed: 12.0,
            turbulence: 0.1,
            lifetime: 2.8,
            spawn_timer: 0.0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  SDF Render Parameters (sent to GPU as uniforms)
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug)]
pub struct SdfParams {
    /// Base radius of the SDF sphere.
    pub radius: f32,
    /// Breath multiplier (pulsing oscillation).
    pub breath: f32,
    /// CPU load percentage (drives color mapping).
    pub cpu_load: f32,
    /// Mood index (0=Serene, 1=Alert, 2=Stressed, 3=Critical).
    pub mood_index: f32,
    /// Temperature in Celsius (weather palette injection).
    pub temperature: f32,
    /// Weather condition flags packed as bits.
    pub weather_flags: u32,
}

impl Default for SdfParams {
    fn default() -> Self {
        Self {
            radius: 55.0,
            breath: 1.0,
            cpu_load: 0.0,
            mood_index: 0.0,
            temperature: 20.0,
            weather_flags: 0,
        }
    }
}
