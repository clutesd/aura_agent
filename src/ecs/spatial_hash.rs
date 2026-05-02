#![allow(dead_code)] // Phase 3-6 scaffolding — see /memories/repo/aura_agent_architecture.md

//! Spatial Hash Grid — O(1) neighbor queries for particle interactions.
//!
//! Dual-layer spatial hash:
//!   - Static layer: screen-space grid for broad-phase culling
//!   - Dynamic layer: per-frame rebuilt for moving particles
//!
//! Hash function: bijective pairing with prime multiplication and XOR mixing.
//! Grid cell size ≥ 90px (orb repulsion radius) for single-cell neighbor lookup.

/// Grid cell size in pixels. Must be ≥ orb repulsion radius (90px).
pub const CELL_SIZE: f32 = 96.0;

/// Maximum entities per cell before overflow (capped to prevent unbounded alloc).
const MAX_PER_CELL: usize = 32;

/// Large prime constants for hash mixing.
const P1: u32 = 73856093;
const P2: u32 = 19349663;

/// A spatial hash grid for fast 2D neighbor queries.
pub struct SpatialHash {
    cols: u32,
    rows: u32,
    table_size: usize,
    /// Each bucket holds indices into the external particle array.
    buckets: Vec<Vec<usize>>,
}

impl SpatialHash {
    pub fn new(screen_w: f32, screen_h: f32) -> Self {
        let cols = (screen_w / CELL_SIZE).ceil() as u32 + 1;
        let rows = (screen_h / CELL_SIZE).ceil() as u32 + 1;
        let table_size = (cols * rows) as usize;
        let buckets = (0..table_size).map(|_| Vec::with_capacity(8)).collect();
        Self {
            cols,
            rows,
            table_size,
            buckets,
        }
    }

    /// Clear all buckets for a new frame.
    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
    }

    /// Hash a world-space position to a bucket index.
    fn hash(&self, x: f32, y: f32) -> usize {
        let cx = ((x / CELL_SIZE).floor() as i32).max(0) as u32;
        let cy = ((y / CELL_SIZE).floor() as i32).max(0) as u32;
        let h = (cx.wrapping_mul(P1)) ^ (cy.wrapping_mul(P2));
        (h as usize) % self.table_size
    }

    /// Insert a particle index at the given world position.
    pub fn insert(&mut self, idx: usize, x: f32, y: f32) {
        let bucket = self.hash(x, y);
        if self.buckets[bucket].len() < MAX_PER_CELL {
            self.buckets[bucket].push(idx);
        }
    }

    /// Query all particle indices in the cell containing (x, y)
    /// and its 8 neighbors (3x3 region).
    pub fn query_neighbors(&self, x: f32, y: f32) -> NeighborIter<'_> {
        let cx = ((x / CELL_SIZE).floor() as i32).max(0) as u32;
        let cy = ((y / CELL_SIZE).floor() as i32).max(0) as u32;
        NeighborIter {
            hash: self,
            cx: cx as i32,
            cy: cy as i32,
            dx: -1,
            dy: -1,
            inner_idx: 0,
        }
    }

    /// Query neighbors and call a function for each.
    /// More efficient than collecting into a Vec.
    pub fn for_each_neighbor<F>(&self, x: f32, y: f32, mut f: F)
    where
        F: FnMut(usize),
    {
        let cx = (x / CELL_SIZE).floor() as i32;
        let cy = (y / CELL_SIZE).floor() as i32;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= self.cols as i32 || ny >= self.rows as i32 {
                    continue;
                }
                let h = ((nx as u32).wrapping_mul(P1) ^ (ny as u32).wrapping_mul(P2)) as usize
                    % self.table_size;
                for &idx in &self.buckets[h] {
                    f(idx);
                }
            }
        }
    }
}

/// Iterator over neighbor cell contents.
pub struct NeighborIter<'a> {
    hash: &'a SpatialHash,
    cx: i32,
    cy: i32,
    dx: i32,
    dy: i32,
    inner_idx: usize,
}

impl<'a> Iterator for NeighborIter<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        loop {
            if self.dy > 1 {
                return None;
            }

            let nx = self.cx + self.dx;
            let ny = self.cy + self.dy;

            if nx >= 0 && ny >= 0 && nx < self.hash.cols as i32 && ny < self.hash.rows as i32 {
                let h = ((nx as u32).wrapping_mul(P1) ^ (ny as u32).wrapping_mul(P2)) as usize
                    % self.hash.table_size;
                let bucket = &self.hash.buckets[h];
                if self.inner_idx < bucket.len() {
                    let val = bucket[self.inner_idx];
                    self.inner_idx += 1;
                    return Some(val);
                }
            }

            // Advance to next cell
            self.inner_idx = 0;
            self.dx += 1;
            if self.dx > 1 {
                self.dx = -1;
                self.dy += 1;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  SPH-inspired density-pressure particle repulsion
// ═══════════════════════════════════════════════════════════════

/// Smoothing kernel radius (same as orb repulsion radius).
pub const SPH_RADIUS: f32 = 90.0;

/// Cubic spline kernel W(r, h) for 2D.
/// Returns normalized kernel value.
#[inline]
pub fn kernel_cubic(dist: f32, h: f32) -> f32 {
    let q = dist / h;
    if q > 1.0 {
        return 0.0;
    }
    let norm = 10.0 / (7.0 * std::f32::consts::PI * h * h);
    if q <= 0.5 {
        norm * (6.0 * q * q * q - 6.0 * q * q + 1.0)
    } else {
        let t = 1.0 - q;
        norm * 2.0 * t * t * t
    }
}

/// Gradient of cubic spline kernel for 2D.
/// Returns (gx, gy) — the gradient vector.
#[inline]
pub fn kernel_cubic_gradient(dx: f32, dy: f32, dist: f32, h: f32) -> (f32, f32) {
    if dist < 0.001 || dist > h {
        return (0.0, 0.0);
    }
    let q = dist / h;
    let norm = 10.0 / (7.0 * std::f32::consts::PI * h * h * h);
    let grad_mag = if q <= 0.5 {
        norm * q * (18.0 * q - 12.0)
    } else {
        let t = 1.0 - q;
        norm * (-6.0 * t * t)
    };
    let nx = dx / dist;
    let ny = dy / dist;
    (grad_mag * nx, grad_mag * ny)
}

/// Apply SPH-style density-pressure repulsion forces between weather particles.
/// Uses the spatial hash for O(n) total rather than O(n²).
pub fn apply_particle_repulsion(
    px: &mut [f32],
    py: &mut [f32],
    vx: &mut [f32],
    vy: &mut [f32],
    count: usize,
    dt: f32,
    hash: &SpatialHash,
    rest_density: f32,
    stiffness: f32,
) {
    // Phase 1: Compute densities
    let mut densities = vec![0.0f32; count];
    for i in 0..count {
        let mut rho = 0.0f32;
        hash.for_each_neighbor(px[i], py[i], |j| {
            if j >= count {
                return;
            }
            let ddx = px[i] - px[j];
            let ddy = py[i] - py[j];
            let dist = (ddx * ddx + ddy * ddy).sqrt();
            rho += kernel_cubic(dist, SPH_RADIUS);
        });
        densities[i] = rho;
    }

    // Phase 2: Compute pressure forces
    for i in 0..count {
        let pressure_i = stiffness * (densities[i] - rest_density);
        let mut fx = 0.0f32;
        let mut fy = 0.0f32;

        hash.for_each_neighbor(px[i], py[i], |j| {
            if j >= count || j == i {
                return;
            }
            let ddx = px[i] - px[j];
            let ddy = py[i] - py[j];
            let dist = (ddx * ddx + ddy * ddy).sqrt();
            if dist > SPH_RADIUS || dist < 0.001 {
                return;
            }

            let pressure_j = stiffness * (densities[j] - rest_density);
            let avg_pressure = (pressure_i + pressure_j) * 0.5;

            let (gx, gy) = kernel_cubic_gradient(ddx, ddy, dist, SPH_RADIUS);
            fx -= avg_pressure * gx;
            fy -= avg_pressure * gy;
        });

        vx[i] += fx * dt;
        vy[i] += fy * dt;
    }
}
