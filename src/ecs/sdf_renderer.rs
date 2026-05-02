#![allow(dead_code)] // Phase 3-6 scaffolding — see /memories/repo/aura_agent_architecture.md

//! SDF Orb Renderer — manages the GLSL shader pipeline for
//! volumetric orb rendering via Signed Distance Fields.
//!
//! Replaces all CPU-side draw_circle_gradient calls with a single
//! fullscreen shader dispatch that evaluates the SDF per-fragment.

use raylib::prelude::*;
use std::path::Path;

/// Holds the loaded shader and its uniform locations.
pub struct SdfRenderer {
    pub shader: Shader,
    // Uniform locations (cached at load time for zero-cost per-frame access)
    loc_resolution: i32,
    loc_orb_pos: i32,
    loc_time: i32,
    loc_radius: i32,
    loc_cpu: i32,
    loc_mood: i32,
    loc_temperature: i32,
    loc_weather_flags: i32,
}

impl SdfRenderer {
    /// Load the SDF shader from disk. Returns None if files are missing.
    pub fn load(rl: &mut RaylibHandle, thread: &RaylibThread) -> Option<Self> {
        let vs_path = "shaders/orb_sdf.vs";
        let fs_path = "shaders/orb_sdf.fs";

        if !Path::new(vs_path).exists() || !Path::new(fs_path).exists() {
            eprintln!("[sdf] WARNING: Shader files not found at {vs_path} / {fs_path}");
            eprintln!("[sdf] Falling back to CPU rendering.");
            return None;
        }

        let shader = rl.load_shader(thread, Some(vs_path), Some(fs_path));

        let loc_resolution = shader.get_shader_location("u_resolution");
        let loc_orb_pos = shader.get_shader_location("u_orb_pos");
        let loc_time = shader.get_shader_location("u_time");
        let loc_radius = shader.get_shader_location("u_radius");
        let loc_cpu = shader.get_shader_location("u_cpu");
        let loc_mood = shader.get_shader_location("u_mood");
        let loc_temperature = shader.get_shader_location("u_temperature");
        let loc_weather_flags = shader.get_shader_location("u_weather_flags");

        Some(Self {
            shader,
            loc_resolution,
            loc_orb_pos,
            loc_time,
            loc_radius,
            loc_cpu,
            loc_mood,
            loc_temperature,
            loc_weather_flags,
        })
    }

    /// Update all shader uniforms for this frame.
    pub fn set_uniforms(
        &mut self,
        _rl: &mut RaylibHandle,
        orb_x: f32,
        orb_y: f32,
        screen_w: f32,
        screen_h: f32,
        time: f32,
        radius: f32,
        cpu: f32,
        mood_index: f32,
        temperature: f32,
        weather_flags: f32,
    ) {
        let resolution = [screen_w, screen_h];
        let orb_pos = [orb_x, orb_y];

        self.shader
            .set_shader_value(self.loc_resolution, resolution);
        self.shader.set_shader_value(self.loc_orb_pos, orb_pos);
        self.shader.set_shader_value(self.loc_time, time);
        self.shader.set_shader_value(self.loc_radius, radius);
        self.shader.set_shader_value(self.loc_cpu, cpu);
        self.shader.set_shader_value(self.loc_mood, mood_index);
        self.shader
            .set_shader_value(self.loc_temperature, temperature);
        self.shader
            .set_shader_value(self.loc_weather_flags, weather_flags);
    }

    /// Draw the orb using the SDF shader on a fullscreen quad.
    /// The shader's early-discard optimization ensures only fragments
    /// near the orb are actually computed.
    ///
    /// Call this inside a `begin_shader_mode()` / `end_shader_mode()` block,
    /// or use the convenience `draw()` method which handles that.
    pub fn draw(
        &mut self,
        d: &mut RaylibDrawHandle,
        orb_x: f32,
        orb_y: f32,
        screen_w: i32,
        screen_h: i32,
        time: f32,
        radius: f32,
        cpu: f32,
        mood_index: f32,
        temperature: f32,
        weather_flags: f32,
    ) {
        // Update uniforms
        let resolution = [screen_w as f32, screen_h as f32];
        let orb_pos = [orb_x, orb_y];

        self.shader
            .set_shader_value(self.loc_resolution, resolution);
        self.shader.set_shader_value(self.loc_orb_pos, orb_pos);
        self.shader.set_shader_value(self.loc_time, time);
        self.shader.set_shader_value(self.loc_radius, radius);
        self.shader.set_shader_value(self.loc_cpu, cpu);
        self.shader.set_shader_value(self.loc_mood, mood_index);
        self.shader
            .set_shader_value(self.loc_temperature, temperature);
        self.shader
            .set_shader_value(self.loc_weather_flags, weather_flags);

        // Draw a fullscreen rectangle with the shader active.
        // The fragment shader evaluates the SDF for each pixel,
        // with early-discard for fragments far from the orb.
        {
            let mut shader_draw = d.begin_shader_mode(&mut self.shader);
            shader_draw.draw_rectangle(0, 0, screen_w, screen_h, Color::WHITE);
        }
    }
}

/// Compute the mood index as a float for the shader uniform.
pub fn mood_to_float(mood: crate::core::Mood) -> f32 {
    match mood {
        crate::core::Mood::Serene => 0.0,
        crate::core::Mood::Alert => 1.0,
        crate::core::Mood::Stressed => 2.0,
        crate::core::Mood::Critical => 3.0,
    }
}

/// Pack weather classification booleans into a single float for the shader.
pub fn pack_weather_flags(
    is_clear: bool,
    is_cloudy: bool,
    is_fog: bool,
    is_rain: bool,
    is_snow: bool,
    is_storm: bool,
) -> f32 {
    let mut flags = 0u32;
    if is_clear {
        flags |= 1;
    }
    if is_cloudy {
        flags |= 2;
    }
    if is_fog {
        flags |= 4;
    }
    if is_rain {
        flags |= 8;
    }
    if is_snow {
        flags |= 16;
    }
    if is_storm {
        flags |= 32;
    }
    flags as f32
}
