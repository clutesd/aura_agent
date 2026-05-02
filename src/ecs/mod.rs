#![allow(dead_code)] // Phase 3-6 scaffolding (World lifecycle API) — see /memories/repo/aura_agent_architecture.md

//! Lightweight, bespoke Entity Component System (ECS).
//!
//! Design goals:
//! - Entities are plain integer IDs (generational for safe reuse).
//! - Components are stored in contiguous arrays (Structure of Arrays layout)
//!   for maximum L1/L2 cache utilization during system iteration.
//! - Systems are plain functions that iterate over component slices.
//! - No heap-allocated trait objects per entity — zero-cost abstraction.

pub mod components;
pub mod flow_field;
pub mod gpu_particles;
pub mod kawase_bloom;
pub mod sdf_renderer;
pub mod spatial_hash;
pub mod spectral_fft;
pub mod steering;

// ═══════════════════════════════════════════════════════════════
//  Entity — generational index for safe ID reuse
// ═══════════════════════════════════════════════════════════════

/// A generational entity handle. The `generation` field prevents
/// use-after-free: if an entity is despawned and its slot recycled,
/// stale handles pointing to the old generation are detected.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Entity {
    pub id: u32,
    pub generation: u32,
}

// ═══════════════════════════════════════════════════════════════
//  World — the central store of all entities and components
// ═══════════════════════════════════════════════════════════════

/// The World owns all entity metadata and all component storage.
/// Systems read/write component arrays via `&mut World`.
pub struct World {
    // Entity bookkeeping
    generations: Vec<u32>,
    alive: Vec<bool>,
    free_ids: Vec<u32>,
    next_id: u32,

    // ── SoA Component Storage ─────────────────────────────────
    // Each Vec is indexed by entity id (slot).
    // Components use Option<T> so not every entity needs every component.
    /// Position (x, y) in screen-space pixels.
    pub position: Vec<Option<components::Position>>,
    /// Velocity (vx, vy) in px/s.
    pub velocity: Vec<Option<components::Velocity>>,
    /// Mood component — current emotional state.
    pub mood: Vec<Option<components::MoodComp>>,
    /// Context steering state (radial interest/danger arrays + chosen direction).
    pub steering: Vec<Option<components::SteeringComp>>,
    /// Orb-specific state: behavior timer, orbit params, rest anchor, etc.
    pub orb_state: Vec<Option<components::OrbStateComp>>,
    /// Render flag — what visual role this entity plays.
    pub render_tag: Vec<Option<components::RenderTag>>,
    /// Trail data — phosphor wake ring buffer.
    pub trail: Vec<Option<components::TrailComp>>,
    /// Emitter data — particle emission parameters.
    pub emitter: Vec<Option<components::EmitterComp>>,
    /// SDF render parameters pushed to the GPU as uniforms.
    pub sdf_params: Vec<Option<components::SdfParams>>,
}

impl World {
    pub fn new() -> Self {
        let initial_cap = 64;
        Self {
            generations: Vec::with_capacity(initial_cap),
            alive: Vec::with_capacity(initial_cap),
            free_ids: Vec::new(),
            next_id: 0,
            position: Vec::with_capacity(initial_cap),
            velocity: Vec::with_capacity(initial_cap),
            mood: Vec::with_capacity(initial_cap),
            steering: Vec::with_capacity(initial_cap),
            orb_state: Vec::with_capacity(initial_cap),
            render_tag: Vec::with_capacity(initial_cap),
            trail: Vec::with_capacity(initial_cap),
            emitter: Vec::with_capacity(initial_cap),
            sdf_params: Vec::with_capacity(initial_cap),
        }
    }

    /// Spawn a new entity. Reuses freed slots when available.
    pub fn spawn(&mut self) -> Entity {
        if let Some(id) = self.free_ids.pop() {
            let idx = id as usize;
            self.generations[idx] += 1;
            self.alive[idx] = true;
            // Clear all component slots for the recycled entity
            self.position[idx] = None;
            self.velocity[idx] = None;
            self.mood[idx] = None;
            self.steering[idx] = None;
            self.orb_state[idx] = None;
            self.render_tag[idx] = None;
            self.trail[idx] = None;
            self.emitter[idx] = None;
            self.sdf_params[idx] = None;
            Entity {
                id,
                generation: self.generations[idx],
            }
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.generations.push(0);
            self.alive.push(true);
            self.position.push(None);
            self.velocity.push(None);
            self.mood.push(None);
            self.steering.push(None);
            self.orb_state.push(None);
            self.render_tag.push(None);
            self.trail.push(None);
            self.emitter.push(None);
            self.sdf_params.push(None);
            Entity { id, generation: 0 }
        }
    }

    /// Despawn an entity, freeing its slot for reuse.
    pub fn despawn(&mut self, e: Entity) {
        let idx = e.id as usize;
        if idx < self.alive.len() && self.alive[idx] && self.generations[idx] == e.generation {
            self.alive[idx] = false;
            self.free_ids.push(e.id);
            // Components remain until slot is recycled (spawn clears them)
        }
    }

    /// Check if an entity handle is still valid.
    pub fn is_alive(&self, e: Entity) -> bool {
        let idx = e.id as usize;
        idx < self.alive.len() && self.alive[idx] && self.generations[idx] == e.generation
    }

    /// Total number of allocated slots (including dead ones).
    pub fn capacity(&self) -> usize {
        self.next_id as usize
    }

    /// Number of currently alive entities.
    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }
}
