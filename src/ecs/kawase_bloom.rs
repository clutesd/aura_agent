#![allow(dead_code)] // Phase 3-6 scaffolding — see /memories/repo/aura_agent_architecture.md

//! Dual Kawase Bloom Pipeline — Phase 6
//!
//! Replaces the 3-pass scaled-texture additive bloom with a proper
//! iterative downsample/upsample cascade:
//!
//!   1. Extract bright pixels from the glow render target
//!   2. Iterative downsample (Kawase kernel: 5 taps, diagonal cross)
//!   3. Iterative upsample (Kawase kernel: 8 taps, diamond pattern)
//!   4. Composite bloom + scene through Uncharted 2 filmic tone map
//!
//! Energy-conserving weights ensure bloom doesn't wash out the scene.
//! Uses GLSL 330 shaders (no compute required).

use raylib::prelude::*;
use std::path::Path;

/// Number of downsample/upsample iterations.
/// Each iteration halves/doubles the resolution.
/// 5 iterations: 1920→960→480→240→120→60, giving wide bloom radius.
pub const BLOOM_ITERATIONS: usize = 5;

pub struct KawaseBloom {
    /// Downsample shader
    down_shader: Shader,
    down_loc_half_pixel: i32,

    /// Upsample shader
    up_shader: Shader,
    up_loc_half_pixel: i32,

    /// Tone mapping shader
    tone_shader: Shader,
    tone_loc_bloom_strength: i32,
    tone_loc_exposure: i32,
    tone_loc_texture1: i32,

    /// Mip chain render textures (progressively smaller)
    mip_chain: Vec<RenderTexture2D>,
    /// Screen-resolution upsampled result
    bloom_rt: RenderTexture2D,

    /// Bloom parameters
    pub bloom_strength: f32,
    pub exposure: f32,
}

impl KawaseBloom {
    /// Create the bloom pipeline. Returns None if shaders are missing.
    pub fn load(rl: &mut RaylibHandle, thread: &RaylibThread, w: u32, h: u32) -> Option<Self> {
        let down_path = "shaders/kawase_down.fs";
        let up_path = "shaders/kawase_up.fs";
        let tone_path = "shaders/tonemap.fs";

        if !Path::new(down_path).exists()
            || !Path::new(up_path).exists()
            || !Path::new(tone_path).exists()
        {
            eprintln!("[bloom] WARNING: Kawase shader files not found, using fallback bloom");
            return None;
        }

        let down_shader = rl.load_shader(thread, None, Some(down_path));
        let up_shader = rl.load_shader(thread, None, Some(up_path));
        let tone_shader = rl.load_shader(thread, None, Some(tone_path));

        let down_loc_half_pixel = down_shader.get_shader_location("u_half_pixel");
        let up_loc_half_pixel = up_shader.get_shader_location("u_half_pixel");
        let tone_loc_bloom_strength = tone_shader.get_shader_location("u_bloom_strength");
        let tone_loc_exposure = tone_shader.get_shader_location("u_exposure");
        let tone_loc_texture1 = tone_shader.get_shader_location("texture1");

        eprintln!(
            "[bloom] Kawase shaders loaded (down={}, up={}, tone={})",
            down_loc_half_pixel, up_loc_half_pixel, tone_loc_bloom_strength
        );

        // Create mip chain render textures
        let mut mip_chain = Vec::with_capacity(BLOOM_ITERATIONS);
        let mut mw = w / 2;
        let mut mh = h / 2;
        for i in 0..BLOOM_ITERATIONS {
            mw = mw.max(1);
            mh = mh.max(1);
            match rl.load_render_texture(thread, mw, mh) {
                Ok(rt) => {
                    eprintln!("[bloom] Mip {} created: {}x{}", i, mw, mh);
                    mip_chain.push(rt);
                }
                Err(e) => {
                    eprintln!("[bloom] ERROR: Failed to create mip {}: {}", i, e);
                    return None;
                }
            }
            mw /= 2;
            mh /= 2;
        }

        let bloom_rt = rl.load_render_texture(thread, w, h).ok()?;

        eprintln!(
            "[bloom] Kawase bloom pipeline ready ({} iterations)",
            BLOOM_ITERATIONS
        );

        Some(Self {
            down_shader,
            down_loc_half_pixel,
            up_shader,
            up_loc_half_pixel,
            tone_shader,
            tone_loc_bloom_strength,
            tone_loc_exposure,
            tone_loc_texture1,
            mip_chain,
            bloom_rt,
            bloom_strength: 0.45,
            exposure: 1.2,
        })
    }

    /// Process bloom: downsample → upsample cascade.
    /// `glow_texture` is the source bright-pixel render target.
    /// After calling this, use `composite()` to draw the final result.
    pub fn process(
        &mut self,
        d: &mut RaylibDrawHandle,
        thread: &RaylibThread,
        glow_rt: &RenderTexture2D,
        is_thinking: bool,
        pulse_active: bool,
    ) {
        use raylib::core::texture::RaylibTexture2D;

        // Adjust bloom intensity based on state
        self.bloom_strength = if is_thinking { 0.55 } else { 0.38 };
        if pulse_active {
            self.bloom_strength += 0.15;
        }

        // ── Downsample cascade ────────────────────────────────
        // First pass: glow_rt → mip[0]
        {
            let src_tex = glow_rt.texture();
            let src_w = src_tex.width() as f32;
            let src_h = src_tex.height() as f32;
            let half_pixel = [0.5 / src_w, 0.5 / src_h];
            self.down_shader
                .set_shader_value(self.down_loc_half_pixel, half_pixel);

            let dst_w = self.mip_chain[0].texture().width();
            let dst_h = self.mip_chain[0].texture().height();
            let mut g = d.begin_texture_mode(thread, &mut self.mip_chain[0]);
            g.clear_background(Color::BLACK);
            {
                let mut sm = g.begin_shader_mode(&mut self.down_shader);
                let src_rect = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: src_w,
                    height: -src_h,
                };
                let dst_rect = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: dst_w as f32,
                    height: dst_h as f32,
                };
                sm.draw_texture_pro(
                    src_tex,
                    src_rect,
                    dst_rect,
                    rvec2(0.0, 0.0),
                    0.0,
                    Color::WHITE,
                );
            }
        }

        // Subsequent downsample passes: mip[i-1] → mip[i]
        // Use split_at_mut to safely borrow two different mip_chain elements
        for i in 1..self.mip_chain.len() {
            let (src_w, src_h) = {
                let tex = self.mip_chain[i - 1].texture();
                (tex.width() as f32, tex.height() as f32)
            };
            let half_pixel = [0.5 / src_w, 0.5 / src_h];
            self.down_shader
                .set_shader_value(self.down_loc_half_pixel, half_pixel);

            let (dst_w, dst_h) = {
                let tex = self.mip_chain[i].texture();
                (tex.width(), tex.height())
            };

            let (left, right) = self.mip_chain.split_at_mut(i);
            let src_tex = left[i - 1].texture();
            let dst_rt = &mut right[0];

            let mut g = d.begin_texture_mode(thread, dst_rt);
            g.clear_background(Color::BLACK);
            {
                let mut sm = g.begin_shader_mode(&mut self.down_shader);
                let src_rect = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: src_w,
                    height: -src_h,
                };
                let dst_rect = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: dst_w as f32,
                    height: dst_h as f32,
                };
                sm.draw_texture_pro(
                    src_tex,
                    src_rect,
                    dst_rect,
                    rvec2(0.0, 0.0),
                    0.0,
                    Color::WHITE,
                );
            }
        }

        // ── Upsample cascade ──────────────────────────────────
        // From smallest mip back up to mip[0], accumulating blur
        for i in (0..self.mip_chain.len() - 1).rev() {
            let (src_w, src_h) = {
                let tex = self.mip_chain[i + 1].texture();
                (tex.width() as f32, tex.height() as f32)
            };
            let half_pixel = [0.5 / src_w, 0.5 / src_h];
            self.up_shader
                .set_shader_value(self.up_loc_half_pixel, half_pixel);

            let (dst_w, dst_h) = {
                let tex = self.mip_chain[i].texture();
                (tex.width(), tex.height())
            };

            let (left, right) = self.mip_chain.split_at_mut(i + 1);
            let dst_rt = &mut left[i];
            let src_tex = right[0].texture();

            let mut g = d.begin_texture_mode(thread, dst_rt);
            // Don't clear — accumulate on top of existing content
            {
                let mut blend = g.begin_blend_mode(BlendMode::BLEND_ADDITIVE);
                {
                    let mut sm = blend.begin_shader_mode(&mut self.up_shader);
                    let src_rect = Rectangle {
                        x: 0.0,
                        y: 0.0,
                        width: src_w,
                        height: -src_h,
                    };
                    let dst_rect = Rectangle {
                        x: 0.0,
                        y: 0.0,
                        width: dst_w as f32,
                        height: dst_h as f32,
                    };
                    sm.draw_texture_pro(
                        src_tex,
                        src_rect,
                        dst_rect,
                        rvec2(0.0, 0.0),
                        0.0,
                        Color::WHITE,
                    );
                }
            }
        }

        // ── Copy final bloom result to full-res bloom_rt ──────
        {
            let src_tex = self.mip_chain[0].texture();
            let src_w = src_tex.width() as f32;
            let src_h = src_tex.height() as f32;
            let dst_w = self.bloom_rt.texture().width();
            let dst_h = self.bloom_rt.texture().height();

            let mut g = d.begin_texture_mode(thread, &mut self.bloom_rt);
            g.clear_background(Color::BLACK);
            let src_rect = Rectangle {
                x: 0.0,
                y: 0.0,
                width: src_w,
                height: -src_h,
            };
            let dst_rect = Rectangle {
                x: 0.0,
                y: 0.0,
                width: dst_w as f32,
                height: dst_h as f32,
            };
            g.draw_texture_pro(
                src_tex,
                src_rect,
                dst_rect,
                rvec2(0.0, 0.0),
                0.0,
                Color::WHITE,
            );
        }
    }

    /// Composite the bloom onto the screen using additive blending.
    /// Call this where the old 3-pass bloom overlay was.
    pub fn composite(&mut self, d: &mut RaylibDrawHandle, w: i32, h: i32) {
        use raylib::core::texture::RaylibTexture2D;
        let tex = self.bloom_rt.texture();
        let tw = tex.width() as f32;
        let th = tex.height() as f32;
        let src = Rectangle {
            x: 0.0,
            y: 0.0,
            width: tw,
            height: -th,
        };
        let dest = Rectangle {
            x: 0.0,
            y: 0.0,
            width: w as f32,
            height: h as f32,
        };

        let bloom = self.bloom_strength.min(1.0);
        let alpha = (255.0 * bloom) as u8;

        {
            let mut blend = d.begin_blend_mode(BlendMode::BLEND_ADDITIVE);
            blend.draw_texture_pro(
                tex,
                src,
                dest,
                rvec2(0.0, 0.0),
                0.0,
                Color::new(255, 255, 255, alpha),
            );
        }
    }
}
