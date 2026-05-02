//! Context Steering System — replaces the simple weighted-random state machine
//! with radial interest/danger evaluation for smooth, collision-free trajectories.
//!
//! References:
//! - Game AI Pro 2, Ch. 18: "Context Steering — Behavior-Driven Steering at the Macro Scale"
//! - Craig Reynolds' original steering behaviors (red3d.com/cwr/steer)
//!
//! The system evaluates STEERING_RESOLUTION (16) directions around the entity.
//! Each direction gets an "interest" score (how much we want to go there) and a
//! "danger" score (how bad it would be). The final direction is the highest-interest
//! direction that is not masked by danger.

use super::components::*;
use super::flow_field::FlowField;
use crate::core::{hash_f, Mood};

// ═══════════════════════════════════════════════════════════════
//  Safe area — keeps the orb out of the HUD/spectral region.
//  Top:    keep a light margin now that the alert panel is gone.
//  Bottom: stay above the spectral analyzer baseline (~72% h).
//          Spectral lives at 0.82h with bars rising up to 0.10h.
// ═══════════════════════════════════════════════════════════════
const SAFE_TOP_FRAC: f32 = 0.06;
const SAFE_BOTTOM_FRAC: f32 = 0.70;

#[inline]
fn safe_y(h: f32) -> (f32, f32) {
    (h * SAFE_TOP_FRAC, h * SAFE_BOTTOM_FRAC)
}

/// Cubic ease-out — used for arrive so the orb glides into targets
/// instead of decelerating linearly.
#[inline]
fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Frame-rate-independent damping (half-life seconds).
#[inline]
fn damp(current: f32, target: f32, half_life: f32, dt: f32) -> f32 {
    let k = 1.0 - 0.5_f32.powf(dt / half_life.max(1e-4));
    current + (target - current) * k
}

/// Cheap pseudo-noise drift force (no dependency on noise crate);
/// produces a smoothly-varying wandering vector that keeps the orb
/// in motion even when it's near its target.
#[inline]
fn drift_force(t: f32, seed: u32) -> (f32, f32) {
    let s = (seed & 0xFFFF) as f32 * 0.0001;
    let a = (t * 0.37 + s * 13.0).sin() + (t * 0.71 + s * 7.0).cos() * 0.6;
    let b = (t * 0.43 + s * 11.0).cos() + (t * 0.83 + s * 5.0).sin() * 0.6;
    (a, b)
}

/// Precomputed unit direction vectors for each slot in the radial array.
/// Slot i points at angle (i / N) * 2π.
pub fn direction_vector(slot: usize) -> (f32, f32) {
    let angle = (slot as f32 / STEERING_RESOLUTION as f32) * std::f32::consts::TAU;
    (angle.cos(), angle.sin())
}

/// Find the slot index whose direction most closely aligns with a given vector.
pub fn vector_to_slot(dx: f32, dy: f32) -> usize {
    let angle = dy.atan2(dx);
    let normalized = if angle < 0.0 {
        angle + std::f32::consts::TAU
    } else {
        angle
    };
    let slot = (normalized / std::f32::consts::TAU * STEERING_RESOLUTION as f32) as usize;
    slot.min(STEERING_RESOLUTION - 1)
}

// ═══════════════════════════════════════════════════════════════
//  Behavior Selection (same weighted-random as before, but
//  now populates OrbStateComp instead of OrbAI)
// ═══════════════════════════════════════════════════════════════

pub fn pick_behavior(orb: &mut OrbStateComp, mood: Mood, t: f32, w: f32, h: f32) {
    orb.seed = orb.seed.wrapping_add((t * 1000.0) as u32);
    let roll = hash_f(orb.seed);

    orb.behavior = match mood {
        Mood::Serene => {
            // Less rest, more motion — Serene should still feel alive.
            if roll < 0.12 {
                OrbBehavior::Rest
            } else if roll < 0.55 {
                OrbBehavior::Wander
            } else if roll < 0.80 {
                OrbBehavior::Orbit
            } else {
                OrbBehavior::Investigate
            }
        }
        Mood::Alert => {
            if roll < 0.05 {
                OrbBehavior::Rest
            } else if roll < 0.40 {
                OrbBehavior::Wander
            } else if roll < 0.70 {
                OrbBehavior::Investigate
            } else {
                OrbBehavior::Orbit
            }
        }
        Mood::Stressed => {
            if roll < 0.40 {
                OrbBehavior::Wander
            } else if roll < 0.80 {
                OrbBehavior::Investigate
            } else {
                OrbBehavior::Orbit
            }
        }
        Mood::Critical => {
            if roll < 0.50 {
                OrbBehavior::Wander
            } else if roll < 0.80 {
                OrbBehavior::Investigate
            } else {
                OrbBehavior::Orbit
            }
        }
    };

    // Shorter behaviors → more frequent goal changes → feels more alive.
    let base_dur = match orb.behavior {
        OrbBehavior::Wander => 2.6,
        OrbBehavior::Investigate => 2.4,
        OrbBehavior::Orbit => 4.0,
        OrbBehavior::Rest => 2.5,
    };
    let mood_scale = match mood {
        Mood::Serene => 1.2,
        Mood::Alert => 0.9,
        Mood::Stressed => 0.65,
        Mood::Critical => 0.45,
    };
    orb.behavior_timer = base_dur * mood_scale + hash_f(orb.seed.wrapping_add(1)) * 1.6;

    let margin_x = w * 0.04;
    let (top_y, bot_y) = safe_y(h);
    let usable_y = (bot_y - top_y).max(1.0);
    match orb.behavior {
        OrbBehavior::Wander => {
            orb.target_x = margin_x + hash_f(orb.seed.wrapping_add(2)) * (w - margin_x * 2.0);
            orb.target_y = top_y + hash_f(orb.seed.wrapping_add(3)) * usable_y;
        }
        OrbBehavior::Investigate => {
            // 6 inspection points: left/right edges + 4 corners of the SAFE area
            // (no top/bottom edges — they collide with the HUD/spectral).
            let edge = (hash_f(orb.seed.wrapping_add(4)) * 6.0) as u32;
            match edge {
                0 => {
                    orb.target_x = margin_x;
                    orb.target_y = top_y + hash_f(orb.seed.wrapping_add(5)) * usable_y;
                }
                1 => {
                    orb.target_x = w - margin_x;
                    orb.target_y = top_y + hash_f(orb.seed.wrapping_add(5)) * usable_y;
                }
                2 => {
                    orb.target_x = margin_x;
                    orb.target_y = top_y;
                }
                3 => {
                    orb.target_x = w - margin_x;
                    orb.target_y = top_y;
                }
                4 => {
                    orb.target_x = margin_x;
                    orb.target_y = bot_y;
                }
                _ => {
                    orb.target_x = w - margin_x;
                    orb.target_y = bot_y;
                }
            }
        }
        OrbBehavior::Orbit => {
            orb.orbit_cx = w * 0.15 + hash_f(orb.seed.wrapping_add(6)) * w * 0.70;
            // Constrain orbit centre well inside the safe area so the
            // circle doesn't dip into the spectral region.
            orb.orbit_cy =
                top_y + usable_y * 0.25 + hash_f(orb.seed.wrapping_add(7)) * usable_y * 0.5;
            orb.orbit_angle = hash_f(orb.seed.wrapping_add(8)) * std::f32::consts::TAU;
        }
        OrbBehavior::Rest => {
            // rest_anchor set from current position by the caller
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Context Steering Evaluation
// ═══════════════════════════════════════════════════════════════

/// Populate the interest map based on the current behavior and flow field.
///
/// For Wander/Investigate: interest is highest toward the target position,
///   boosted by the flow field sample at the entity's current location.
/// For Orbit: interest curves around the orbit anchor.
/// For Rest: interest gently points toward the sway position.
pub fn evaluate_interest(
    steering: &mut SteeringComp,
    pos: &Position,
    orb: &OrbStateComp,
    mood: Mood,
    t: f32,
    flow_field: &FlowField,
) {
    // Zero out
    steering.interest = [0.0; STEERING_RESOLUTION];

    match orb.behavior {
        OrbBehavior::Wander | OrbBehavior::Investigate => {
            let tx = orb.target_x;
            let ty = orb.target_y;
            let dx = tx - pos.x;
            let dy = ty - pos.y;
            let _dist = (dx * dx + dy * dy).sqrt().max(0.001);

            // Primary interest: direction toward target
            let target_slot = vector_to_slot(dx, dy);
            for i in 0..STEERING_RESOLUTION {
                // Interest falls off with angular distance from target direction
                let slot_diff = angular_slot_distance(i, target_slot);
                let angular_factor = 1.0 - (slot_diff as f32 / (STEERING_RESOLUTION as f32 * 0.5));
                steering.interest[i] = angular_factor.max(0.0);
            }

            // Flow field boost: sample the pre-computed vector field at current pos
            let (flow_dx, flow_dy) = flow_field.sample(pos.x, pos.y);
            if flow_dx.abs() > 0.001 || flow_dy.abs() > 0.001 {
                let flow_slot = vector_to_slot(flow_dx, flow_dy);
                for i in 0..STEERING_RESOLUTION {
                    let slot_diff = angular_slot_distance(i, flow_slot);
                    let flow_factor = 1.0 - (slot_diff as f32 / (STEERING_RESOLUTION as f32 * 0.5));
                    // Blend flow field influence (30% weight)
                    steering.interest[i] += flow_factor.max(0.0) * 0.3;
                }
            }
        }
        OrbBehavior::Orbit => {
            let orbit_r = match mood {
                Mood::Serene => 110.0,
                Mood::Alert => 150.0,
                Mood::Stressed => 190.0,
                Mood::Critical => 230.0,
            };
            let _orbit_speed = match mood {
                Mood::Serene => 0.4,
                Mood::Alert => 0.6,
                Mood::Stressed => 0.9,
                Mood::Critical => 1.4,
            };
            // Desired position on the orbit circle
            let goal_x = orb.orbit_cx + orb.orbit_angle.cos() * orbit_r;
            let goal_y = orb.orbit_cy + orb.orbit_angle.sin() * orbit_r;
            let dx = goal_x - pos.x;
            let dy = goal_y - pos.y;
            let target_slot = vector_to_slot(dx, dy);
            for i in 0..STEERING_RESOLUTION {
                let slot_diff = angular_slot_distance(i, target_slot);
                let angular_factor = 1.0 - (slot_diff as f32 / (STEERING_RESOLUTION as f32 * 0.5));
                steering.interest[i] = angular_factor.max(0.0);
            }
        }
        OrbBehavior::Rest => {
            let sway_x = orb.rest_anchor_x + (t * 0.4).sin() * 40.0 + (t * 0.17).cos() * 25.0;
            let sway_y = orb.rest_anchor_y + (t * 0.3 + 1.0).cos() * 35.0 + (t * 0.23).sin() * 20.0;
            let dx = sway_x - pos.x;
            let dy = sway_y - pos.y;
            let target_slot = vector_to_slot(dx, dy);
            for i in 0..STEERING_RESOLUTION {
                let slot_diff = angular_slot_distance(i, target_slot);
                let angular_factor = 1.0 - (slot_diff as f32 / (STEERING_RESOLUTION as f32 * 0.5));
                steering.interest[i] = angular_factor.max(0.0) * 0.5; // gentler for rest
            }
        }
    }
}

/// Populate the danger map from screen boundaries and the HUD safe area.
/// The bottom danger zone is widened so the orb is actively pushed up
/// away from the spectral analyzer / cognitive stream.
pub fn evaluate_danger(steering: &mut SteeringComp, pos: &Position, w: f32, h: f32) {
    steering.danger = [0.0; STEERING_RESOLUTION];

    let margin_x = w * 0.04;
    let (top_y, bot_y) = safe_y(h);
    // Soft falloff zones — danger ramps up across this many pixels.
    let zone_top = (h * 0.05).max(40.0); // generous upper buffer
    let zone_bottom = (h * 0.10).max(60.0); // larger — keep above spectral

    for i in 0..STEERING_RESOLUTION {
        let (dir_x, dir_y) = direction_vector(i);
        let mut d = 0.0_f32;

        // Left edge
        if dir_x < 0.0 && pos.x < margin_x {
            d += (1.0 - pos.x / margin_x) * (-dir_x);
        }
        // Right edge
        if dir_x > 0.0 && pos.x > w - margin_x {
            d += ((pos.x - (w - margin_x)) / margin_x) * dir_x;
        }
        // Top safe-area edge: ramp begins above top_y
        if dir_y < 0.0 && pos.y < top_y {
            let depth = ((top_y - pos.y) / zone_top).clamp(0.0, 1.0);
            d += depth * (-dir_y);
        }
        // Bottom safe-area edge: STRONG ramp before the spectral region.
        if dir_y > 0.0 && pos.y > bot_y - zone_bottom {
            let depth = ((pos.y - (bot_y - zone_bottom)) / zone_bottom).clamp(0.0, 1.0);
            // Square the depth so danger spikes hard near the bound.
            d += depth * depth * dir_y * 1.4;
        }

        steering.danger[i] = d.clamp(0.0, 1.0);
    }
}

/// Resolve the final steering direction by masking danger from interest.
/// Returns the chosen direction as a unit vector in `steering.chosen_dir`.
pub fn resolve_steering(steering: &mut SteeringComp) {
    let mut best_score = -1.0_f32;
    let mut best_slot = 0_usize;

    for i in 0..STEERING_RESOLUTION {
        // Mask: if danger exceeds a threshold, zero out interest
        let effective = if steering.danger[i] > 0.3 {
            steering.interest[i] * (1.0 - steering.danger[i])
        } else {
            steering.interest[i]
        };

        if effective > best_score {
            best_score = effective;
            best_slot = i;
        }
    }

    // Smooth the chosen direction by blending the best slot with its neighbors
    let prev = if best_slot == 0 {
        STEERING_RESOLUTION - 1
    } else {
        best_slot - 1
    };
    let next = (best_slot + 1) % STEERING_RESOLUTION;

    let (bx, by) = direction_vector(best_slot);
    let (px, py) = direction_vector(prev);
    let (nx, ny) = direction_vector(next);

    let w_best = steering.interest[best_slot];
    let w_prev = steering.interest[prev] * 0.5;
    let w_next = steering.interest[next] * 0.5;
    let w_total = w_best + w_prev + w_next;

    if w_total > 0.001 {
        let dx = (bx * w_best + px * w_prev + nx * w_next) / w_total;
        let dy = (by * w_best + py * w_prev + ny * w_next) / w_total;
        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.001 {
            steering.chosen_dir = (dx / len, dy / len);
        } else {
            steering.chosen_dir = direction_vector(best_slot);
        }
    } else {
        // No viable direction — maintain current heading (will decelerate via drag)
        steering.chosen_dir = (0.0, 0.0);
    }
}

// ═══════════════════════════════════════════════════════════════
//  Full Context Steering System — called once per frame
// ═══════════════════════════════════════════════════════════════

/// The main context steering system. For each entity with Position,
/// Velocity, SteeringComp, and OrbStateComp, evaluate interest/danger,
/// resolve the best direction, and apply steering forces.
pub fn context_steering_system(
    positions: &mut [Option<Position>],
    velocities: &mut [Option<Velocity>],
    steerings: &mut [Option<SteeringComp>],
    orb_states: &mut [Option<OrbStateComp>],
    moods: &[Option<MoodComp>],
    flow_field: &FlowField,
    dt: f32,
    t: f32,
    w: f32,
    h: f32,
) {
    let count = positions
        .len()
        .min(velocities.len())
        .min(steerings.len())
        .min(orb_states.len())
        .min(moods.len());

    for i in 0..count {
        // Only process entities that have all required components
        let (pos, vel, steer, orb, mood_c) = match (
            &mut positions[i],
            &mut velocities[i],
            &mut steerings[i],
            &mut orb_states[i],
            &moods[i],
        ) {
            (Some(p), Some(v), Some(s), Some(o), Some(m)) => (p, v, s, o, m),
            _ => continue,
        };

        let mood = mood_c.mood;

        // Update behavior timer
        orb.behavior_timer -= dt;
        if orb.behavior_timer <= 0.0 {
            // Save position for rest anchor before picking new behavior
            orb.rest_anchor_x = pos.x;
            orb.rest_anchor_y = pos.y;
            pick_behavior(orb, mood, t, w, h);
        }

        // Advance orbit angle
        if orb.behavior == OrbBehavior::Orbit {
            let orbit_speed = match mood {
                Mood::Serene => 0.5,
                Mood::Alert => 0.8,
                Mood::Stressed => 1.1,
                Mood::Critical => 1.6,
            };
            orb.orbit_angle += orbit_speed * dt;
        }

        // ── Speed/force tuned upward — orb should always feel in motion ──
        let (max_speed, steer_force) = match mood {
            Mood::Serene => (130.0, 180.0),
            Mood::Alert => (200.0, 280.0),
            Mood::Stressed => (310.0, 420.0),
            Mood::Critical => (440.0, 560.0),
        };
        steer.max_speed = max_speed;
        steer.steer_force = steer_force;

        // ── Re-roll target early if we've arrived (Wander/Investigate) ──
        // Without this, the orb sits at the goal until behavior_timer
        // expires, which is what made it feel "stuck".
        if matches!(orb.behavior, OrbBehavior::Wander | OrbBehavior::Investigate) {
            let dx = orb.target_x - pos.x;
            let dy = orb.target_y - pos.y;
            if dx * dx + dy * dy < 35.0 * 35.0 {
                let margin_x = w * 0.04;
                let (top_y, bot_y) = safe_y(h);
                orb.seed = orb.seed.wrapping_add(7919);
                orb.target_x = margin_x + hash_f(orb.seed) * (w - margin_x * 2.0);
                orb.target_y = top_y + hash_f(orb.seed.wrapping_add(31)) * (bot_y - top_y);
            }
        }

        // ── Evaluate interest and danger ──────────────────────
        evaluate_interest(steer, pos, orb, mood, t, flow_field);
        evaluate_danger(steer, pos, w, h);

        // ── Resolve final direction ───────────────────────────
        resolve_steering(steer);

        // ── Apply steering force ──────────────────────────────
        let (dir_x, dir_y) = steer.chosen_dir;
        let desired_speed = if steer.chosen_dir == (0.0, 0.0) {
            0.0
        } else {
            // Cubic arrive — orb stays at full speed longer, then
            // glides smoothly into the target instead of crawling.
            let dx = orb.target_x - pos.x;
            let dy = orb.target_y - pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            let arrive_radius = 110.0;
            if dist < arrive_radius {
                max_speed * (0.25 + 0.75 * ease_out_cubic(dist / arrive_radius))
            } else {
                max_speed
            }
        };

        let desired_vx = dir_x * desired_speed;
        let desired_vy = dir_y * desired_speed;

        // Steering = desired - current, clamped by force
        let mut sx = desired_vx - vel.vx;
        let mut sy = desired_vy - vel.vy;
        let sm = (sx * sx + sy * sy).sqrt();
        if sm > steer_force {
            sx = sx / sm * steer_force;
            sy = sy / sm * steer_force;
        }

        // Apply as acceleration
        vel.vx += sx * dt * 8.0;
        vel.vy += sy * dt * 8.0;

        // ── Continuous drift force — keeps the orb breathing even at rest ──
        let (drift_amp, half_life) = match mood {
            Mood::Serene => (24.0, 1.4),
            Mood::Alert => (38.0, 1.0),
            Mood::Stressed => (60.0, 0.7),
            Mood::Critical => (90.0, 0.5),
        };
        let (dxn, dyn_) = drift_force(t, orb.seed);
        vel.vx += dxn * drift_amp * dt;
        vel.vy += dyn_ * drift_amp * dt;

        // Clamp speed
        let spd = (vel.vx * vel.vx + vel.vy * vel.vy).sqrt();
        if spd > max_speed {
            vel.vx = vel.vx / spd * max_speed;
            vel.vy = vel.vy / spd * max_speed;
        }

        // Frame-rate-independent damping (replaces (0.4).powf(dt) drag).
        // Slightly longer half-life keeps motion flowing.
        vel.vx = damp(vel.vx, 0.0, half_life, dt);
        vel.vy = damp(vel.vy, 0.0, half_life, dt);

        // Integrate position
        pos.x += vel.vx * dt;
        pos.y += vel.vy * dt;

        // Hard clamp safety net — X full, Y constrained to safe area.
        pos.x = pos.x.clamp(10.0, w - 10.0);
        let (top_y, bot_y) = safe_y(h);
        if pos.y < top_y {
            pos.y = top_y;
            if vel.vy < 0.0 {
                vel.vy *= -0.3;
            } // soft bounce
        }
        if pos.y > bot_y {
            pos.y = bot_y;
            if vel.vy > 0.0 {
                vel.vy *= -0.3;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Trail Update System
// ═══════════════════════════════════════════════════════════════

#[allow(dead_code)] // Phase 3-6 scaffolding — not yet driven each frame
pub fn trail_system(
    positions: &[Option<Position>],
    velocities: &[Option<Velocity>],
    trails: &mut [Option<TrailComp>],
    dt: f32,
) {
    let count = positions.len().min(velocities.len()).min(trails.len());
    for i in 0..count {
        let (pos, vel, trail) = match (&positions[i], &velocities[i], &mut trails[i]) {
            (Some(p), Some(v), Some(t)) => (p, v, t),
            _ => continue,
        };

        // Age all points
        for p in trail.points.iter_mut() {
            p.age += dt;
        }

        // Sample at fixed interval
        trail.timer += dt;
        if trail.timer >= trail.sample_interval {
            trail.timer = 0.0;
            let speed = (vel.vx * vel.vx + vel.vy * vel.vy).sqrt();
            trail.points[trail.head] = TrailSample {
                x: pos.x,
                y: pos.y,
                age: 0.0,
                speed,
            };
            trail.head = (trail.head + 1) % TRAIL_LEN;
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════

/// Circular distance between two slots on the steering ring.
fn angular_slot_distance(a: usize, b: usize) -> usize {
    let diff = if a > b { a - b } else { b - a };
    diff.min(STEERING_RESOLUTION - diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_vectors_unit_length() {
        for i in 0..STEERING_RESOLUTION {
            let (x, y) = direction_vector(i);
            let len = (x * x + y * y).sqrt();
            assert!((len - 1.0).abs() < 0.001, "Slot {} has length {}", i, len);
        }
    }

    #[test]
    fn test_angular_slot_distance_wraps() {
        assert_eq!(angular_slot_distance(0, STEERING_RESOLUTION - 1), 1);
        assert_eq!(angular_slot_distance(0, 0), 0);
        assert_eq!(
            angular_slot_distance(0, STEERING_RESOLUTION / 2),
            STEERING_RESOLUTION / 2
        );
    }

    #[test]
    fn test_vector_to_slot_roundtrip() {
        for i in 0..STEERING_RESOLUTION {
            let (dx, dy) = direction_vector(i);
            let slot = vector_to_slot(dx, dy);
            assert_eq!(slot, i, "Roundtrip failed for slot {}", i);
        }
    }
}
