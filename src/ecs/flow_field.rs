//! Pre-computed 2D Flow Field for O(1) runtime pathfinding.
//!
//! The flow field is a uniform grid covering the screen, where each cell
//! stores a 2D unit vector pointing toward a "goal" region. Entities
//! sample the field at their current position to get an instant desired
//! direction without any per-frame pathfinding cost.
//!
//! The field is recomputed when the goal changes (behavior switch) or
//! when obstacles shift. For the Orb's Wander/Investigate behaviors,
//! the goal is the target waypoint.

use crate::fx::noise::simplex_noise2d;

/// Flow field cell size in pixels. Larger = less memory, coarser.
pub const CELL_SIZE: f32 = 32.0;

pub struct FlowField {
    /// Grid of (dx, dy) unit vectors.
    pub cells: Vec<(f32, f32)>,
    pub cols: usize,
    pub rows: usize,
    pub cell_size: f32,
    /// The current goal the field is computed toward.
    pub goal_x: f32,
    pub goal_y: f32,
    /// Screen dimensions used for boundary repulsion.
    pub screen_w: f32,
    pub screen_h: f32,
}

impl FlowField {
    pub fn new(screen_w: f32, screen_h: f32) -> Self {
        let cols = (screen_w / CELL_SIZE).ceil() as usize + 1;
        let rows = (screen_h / CELL_SIZE).ceil() as usize + 1;
        let count = cols * rows;
        Self {
            cells: vec![(0.0, 0.0); count],
            cols,
            rows,
            cell_size: CELL_SIZE,
            goal_x: screen_w * 0.5,
            goal_y: screen_h * 0.5,
            screen_w,
            screen_h,
        }
    }

    /// Recompute the flow field toward a new goal position.
    /// Each cell's vector points toward the goal, with edge repulsion
    /// baked in as a deflection near screen boundaries, and subtle
    /// noise perturbation for organic feel.
    pub fn compute(&mut self, goal_x: f32, goal_y: f32, t: f32) {
        self.goal_x = goal_x;
        self.goal_y = goal_y;

        let margin_frac = 0.04;
        let margin_x = self.screen_w * margin_frac;
        let margin_y = self.screen_h * margin_frac;

        for row in 0..self.rows {
            for col in 0..self.cols {
                let cx = col as f32 * self.cell_size + self.cell_size * 0.5;
                let cy = row as f32 * self.cell_size + self.cell_size * 0.5;

                // Base direction: toward goal
                let mut dx = goal_x - cx;
                let mut dy = goal_y - cy;

                // Edge repulsion gradient
                if cx < margin_x {
                    dx += (1.0 - cx / margin_x) * self.screen_w * 0.5;
                }
                if cx > self.screen_w - margin_x {
                    dx -= ((cx - (self.screen_w - margin_x)) / margin_x) * self.screen_w * 0.5;
                }
                if cy < margin_y {
                    dy += (1.0 - cy / margin_y) * self.screen_h * 0.5;
                }
                if cy > self.screen_h - margin_y {
                    dy -= ((cy - (self.screen_h - margin_y)) / margin_y) * self.screen_h * 0.5;
                }

                // Subtle noise perturbation for organic paths
                let noise_scale = 0.003;
                let noise_x = simplex_noise2d(cx * noise_scale + t * 0.1, cy * noise_scale) * 0.15;
                let noise_y = simplex_noise2d(cx * noise_scale, cy * noise_scale + t * 0.1) * 0.15;
                dx += noise_x * self.screen_w;
                dy += noise_y * self.screen_h;

                // Normalize to unit vector
                let len = (dx * dx + dy * dy).sqrt();
                let idx = row * self.cols + col;
                if len > 0.001 {
                    self.cells[idx] = (dx / len, dy / len);
                } else {
                    self.cells[idx] = (0.0, 0.0);
                }
            }
        }
    }

    /// Sample the flow field at a continuous screen-space coordinate.
    /// Returns the interpolated (dx, dy) unit direction at that point.
    /// O(1) evaluation — just a grid lookup + bilinear interpolation.
    pub fn sample(&self, x: f32, y: f32) -> (f32, f32) {
        // Convert to grid coordinates
        let gx = (x / self.cell_size).max(0.0);
        let gy = (y / self.cell_size).max(0.0);

        let col0 = (gx as usize).min(self.cols.saturating_sub(2));
        let row0 = (gy as usize).min(self.rows.saturating_sub(2));
        let col1 = col0 + 1;
        let row1 = row0 + 1;

        // Fractional position within cell
        let fx = gx - col0 as f32;
        let fy = gy - row0 as f32;

        // Bilinear interpolation of the four surrounding cells
        let i00 = row0 * self.cols + col0;
        let i10 = row0 * self.cols + col1;
        let i01 = row1 * self.cols + col0;
        let i11 = row1 * self.cols + col1;

        let (x00, y00) = self.cells.get(i00).copied().unwrap_or((0.0, 0.0));
        let (x10, y10) = self.cells.get(i10).copied().unwrap_or((0.0, 0.0));
        let (x01, y01) = self.cells.get(i01).copied().unwrap_or((0.0, 0.0));
        let (x11, y11) = self.cells.get(i11).copied().unwrap_or((0.0, 0.0));

        let rx = x00 * (1.0 - fx) * (1.0 - fy)
            + x10 * fx * (1.0 - fy)
            + x01 * (1.0 - fx) * fy
            + x11 * fx * fy;
        let ry = y00 * (1.0 - fx) * (1.0 - fy)
            + y10 * fx * (1.0 - fy)
            + y01 * (1.0 - fx) * fy
            + y11 * fx * fy;

        // Re-normalize (interpolation can denormalize)
        let len = (rx * rx + ry * ry).sqrt();
        if len > 0.001 {
            (rx / len, ry / len)
        } else {
            (0.0, 0.0)
        }
    }
}
