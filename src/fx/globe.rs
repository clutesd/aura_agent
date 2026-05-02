//! Earth half-globe renderer (satellite view).
//!
//! Loads `shaders/earth_globe.{vs,fs}` and dispatches a fullscreen
//! quad. The fragment shader discards anything outside the configured
//! disk so the cost is bounded by the visible globe area.
//!
//! Designed to be drawn inline in the main pass after the background
//! / nebula but before foreground UI / orb effects.

use raylib::prelude::*;
use std::path::Path;

pub struct EarthGlobe {
    pub shader: Shader,
    loc_resolution: i32,
    loc_center: i32,
    loc_radius: i32,
    loc_time: i32,
    loc_sun_dir: i32,
    loc_alpha: i32,
    loc_utc: i32,
    loc_kenora_pulse: i32,
}

impl EarthGlobe {
    pub fn load(rl: &mut RaylibHandle, thread: &RaylibThread) -> Option<Self> {
        let vs = "shaders/earth_globe.vs";
        let fs = "shaders/earth_globe.fs";
        if !Path::new(vs).exists() || !Path::new(fs).exists() {
            eprintln!("[globe] shader files missing ({vs} / {fs})");
            return None;
        }
        let shader = rl.load_shader(thread, Some(vs), Some(fs));
        let loc_resolution = shader.get_shader_location("u_resolution");
        let loc_center = shader.get_shader_location("u_center");
        let loc_radius = shader.get_shader_location("u_radius");
        let loc_time = shader.get_shader_location("u_time");
        let loc_sun_dir = shader.get_shader_location("u_sun_dir");
        let loc_alpha = shader.get_shader_location("u_alpha");
        let loc_utc = shader.get_shader_location("u_utc");
        let loc_kenora_pulse = shader.get_shader_location("u_kenora_pulse");
        Some(Self {
            shader,
            loc_resolution,
            loc_center,
            loc_radius,
            loc_time,
            loc_sun_dir,
            loc_alpha,
            loc_utc,
            loc_kenora_pulse,
        })
    }

    /// Draw the globe. `sun_az_deg` is the celestial sun azimuth in
    /// degrees (0 = north, increasing eastward). `alpha` is the master
    /// opacity in 0..1 — useful for fade-in or night dimming.
    pub fn draw(
        &mut self,
        d: &mut RaylibDrawHandle,
        screen_w: i32,
        screen_h: i32,
        center_x: f32,
        center_y: f32,
        radius: f32,
        time: f32,
        sun_az_deg: f32,
        alpha: f32,
        utc_seconds: f32,
    ) {
        let res = [screen_w as f32, screen_h as f32];
        let cen = [center_x, center_y];
        // Convert azimuth (deg, 0=N, +E) to a unit vector in the
        // sphere's local XZ plane. The shader adds a small +Y bias so
        // the equator never goes pitch black.
        let az = sun_az_deg.to_radians();
        let sun_dir = [az.sin(), az.cos()];

        self.shader.set_shader_value(self.loc_resolution, res);
        self.shader.set_shader_value(self.loc_center, cen);
        self.shader.set_shader_value(self.loc_radius, radius);
        self.shader.set_shader_value(self.loc_time, time);
        self.shader.set_shader_value(self.loc_sun_dir, sun_dir);
        self.shader
            .set_shader_value(self.loc_alpha, alpha.clamp(0.0, 1.0));
        // Wrap UTC to a single day so float precision stays high in-shader.
        let utc_wrapped = utc_seconds.rem_euclid(86_400.0);
        self.shader.set_shader_value(self.loc_utc, utc_wrapped);
        // Gentle 1.6-second pulse for the Kenora pin (animated entirely in shader
        // to keep CPU side trivial; we only feed in the global time).
        let pulse = 0.5 + 0.5 * (time * 3.9).sin();
        self.shader.set_shader_value(self.loc_kenora_pulse, pulse);

        // Draw a fullscreen rectangle. The shader discards everything
        // outside the disk + halo, so the actual fragment work is
        // bounded to the visible globe area regardless.
        let mut sd = d.begin_shader_mode(&mut self.shader);
        sd.draw_rectangle(0, 0, screen_w, screen_h, Color::WHITE);
    }
}
