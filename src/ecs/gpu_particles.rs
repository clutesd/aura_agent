#![allow(dead_code)] // Phase 3-6 scaffolding — see /memories/repo/aura_agent_architecture.md

//! GPU Particle System — compute-shader-driven mote simulation.
//!
//! Replaces the CPU-bound OrbEmitter (80 motes, draw_circle_gradient)
//! with a 2048-particle GPU pipeline using:
//!   - SSBOs for SoA particle data
//!   - Compute shader for physics (gravity, turbulence, lifetime)
//!   - Instanced rendering with radial-gradient fragment shader
//!
//! All OpenGL calls go through `raylib::ffi` (rlgl bindings).

use std::ffi::CString;
use std::path::Path;

// rlgl constants from raylib_sys
const RL_SHADER_UNIFORM_FLOAT: i32 = 0;
const RL_SHADER_UNIFORM_VEC2: i32 = 1;
const RL_SHADER_UNIFORM_UINT: i32 = 8;
const RL_COMPUTE_SHADER: i32 = 0x91B9_u32 as i32; // GL_COMPUTE_SHADER = 37305
const RL_FLOAT: i32 = 0x1406; // GL_FLOAT
const RL_DYNAMIC_DRAW: i32 = 0x88E8; // GL_DYNAMIC_DRAW

/// Number of particles in the GPU pool.
pub const GPU_PARTICLE_COUNT: usize = 2048;

/// Number of SSBO bindings (pos_x, pos_y, vel_x, vel_y, life, max_life, size, seed, alpha).
const SSBO_COUNT: usize = 9;

pub struct GpuParticleSystem {
    /// Compute shader program ID
    compute_program: u32,
    /// Render shader program ID (vertex + fragment)
    render_program: u32,
    /// SSBO IDs: [pos_x, pos_y, vel_x, vel_y, life, max_life, size, seed, alpha]
    ssbos: [u32; SSBO_COUNT],
    /// VAO for the unit quad used in instanced rendering
    quad_vao: u32,
    /// VBO for the unit quad vertices
    quad_vbo: u32,
    /// Compute uniform locations
    cu_dt: i32,
    cu_time: i32,
    cu_orb_x: i32,
    cu_orb_y: i32,
    cu_emit_rate: i32,
    cu_speed_base: i32,
    cu_turbulence: i32,
    cu_drift: i32,
    cu_max_life: i32,
    cu_orb_radius: i32,
    cu_particle_count: i32,
    cu_spawn_offset: i32,
    /// Render uniform locations
    ru_resolution: i32,
    ru_mood: i32,
    /// Spawn ring-buffer offset (CPU-side counter)
    spawn_cursor: usize,
    /// Accumulated spawn fractional particles
    spawn_accum: f32,
    /// Whether the system initialized successfully
    ready: bool,
}

impl GpuParticleSystem {
    /// Create and initialize the GPU particle system.
    /// Returns None if compute shaders aren't supported or shader compilation fails.
    pub fn new() -> Option<Self> {
        let cs_path = "shaders/particle_compute.glsl";
        let vs_path = "shaders/particle_vertex.glsl";
        let fs_path = "shaders/particle_fragment.glsl";

        if !Path::new(cs_path).exists()
            || !Path::new(vs_path).exists()
            || !Path::new(fs_path).exists()
        {
            eprintln!(
                "[gpu_particles] WARNING: Shader files not found, falling back to CPU particles"
            );
            return None;
        }

        unsafe {
            // ── Compile compute shader ────────────────────────────
            let cs_source = std::fs::read_to_string(cs_path).ok()?;
            let cs_cstr = CString::new(cs_source).ok()?;
            let cs_id = raylib::ffi::rlCompileShader(cs_cstr.as_ptr(), RL_COMPUTE_SHADER);
            if cs_id == 0 {
                eprintln!("[gpu_particles] ERROR: Compute shader compilation failed");
                return None;
            }
            let compute_program = raylib::ffi::rlLoadComputeShaderProgram(cs_id);
            if compute_program == 0 {
                eprintln!("[gpu_particles] ERROR: Compute shader program linking failed");
                return None;
            }
            eprintln!(
                "[gpu_particles] Compute shader compiled OK (program={})",
                compute_program
            );

            // ── Compile render shader (vertex + fragment) ─────────
            let vs_source = std::fs::read_to_string(vs_path).ok()?;
            let fs_source = std::fs::read_to_string(fs_path).ok()?;
            let vs_cstr = CString::new(vs_source).ok()?;
            let fs_cstr = CString::new(fs_source).ok()?;
            let render_program = raylib::ffi::rlLoadShaderCode(vs_cstr.as_ptr(), fs_cstr.as_ptr());
            if render_program == 0 {
                eprintln!("[gpu_particles] ERROR: Render shader compilation failed");
                return None;
            }
            eprintln!(
                "[gpu_particles] Render shader compiled OK (program={})",
                render_program
            );

            // ── Create SSBOs ──────────────────────────────────────
            let buf_size = (GPU_PARTICLE_COUNT * std::mem::size_of::<f32>()) as u32;
            let mut ssbos = [0u32; SSBO_COUNT];

            // Initialize all buffers to zero
            let zeros = vec![0.0f32; GPU_PARTICLE_COUNT];
            let zeros_ptr = zeros.as_ptr() as *const std::ffi::c_void;

            for i in 0..SSBO_COUNT {
                ssbos[i] = raylib::ffi::rlLoadShaderBuffer(buf_size, zeros_ptr, RL_DYNAMIC_DRAW);
                if ssbos[i] == 0 {
                    eprintln!("[gpu_particles] ERROR: Failed to create SSBO {}", i);
                    // Clean up already-created SSBOs
                    for j in 0..i {
                        raylib::ffi::rlUnloadShaderBuffer(ssbos[j]);
                    }
                    return None;
                }
            }

            // Initialize seed buffer with random values
            let mut seeds = vec![0.0f32; GPU_PARTICLE_COUNT];
            for i in 0..GPU_PARTICLE_COUNT {
                seeds[i] = crate::core::hash_f((i as u32).wrapping_mul(7919).wrapping_add(42));
            }
            raylib::ffi::rlUpdateShaderBuffer(
                ssbos[7], // seed SSBO
                seeds.as_ptr() as *const std::ffi::c_void,
                buf_size,
                0,
            );

            eprintln!(
                "[gpu_particles] {} SSBOs created ({} bytes each)",
                SSBO_COUNT, buf_size
            );

            // ── Create unit quad VAO/VBO for instanced rendering ──
            // Quad: 2 triangles, each vertex has (x, y, z, u, v)
            #[rustfmt::skip]
            let quad_verts: [f32; 30] = [
                // pos (x,y,z)        uv (u,v)
                -1.0, -1.0, 0.0,     0.0, 0.0,
                 1.0, -1.0, 0.0,     1.0, 0.0,
                 1.0,  1.0, 0.0,     1.0, 1.0,
                -1.0, -1.0, 0.0,     0.0, 0.0,
                 1.0,  1.0, 0.0,     1.0, 1.0,
                -1.0,  1.0, 0.0,     0.0, 1.0,
            ];

            let quad_vao = raylib::ffi::rlLoadVertexArray();
            raylib::ffi::rlEnableVertexArray(quad_vao);

            let quad_vbo = raylib::ffi::rlLoadVertexBuffer(
                quad_verts.as_ptr() as *const std::ffi::c_void,
                (quad_verts.len() * std::mem::size_of::<f32>()) as i32,
                false, // static
            );

            let stride = (5 * std::mem::size_of::<f32>()) as i32;
            // Attribute 0: position (vec3)
            raylib::ffi::rlSetVertexAttribute(0, 3, RL_FLOAT, false, stride, 0);
            raylib::ffi::rlEnableVertexAttribute(0);
            // Attribute 1: texcoord (vec2)
            raylib::ffi::rlSetVertexAttribute(1, 2, RL_FLOAT, false, stride, 3 * 4);
            raylib::ffi::rlEnableVertexAttribute(1);

            raylib::ffi::rlDisableVertexArray();

            eprintln!(
                "[gpu_particles] Unit quad VAO created (vao={}, vbo={})",
                quad_vao, quad_vbo
            );

            // ── Cache uniform locations ───────────────────────────
            let get_uniform = |prog: u32, name: &str| -> i32 {
                let cname = CString::new(name).unwrap();
                raylib::ffi::rlGetLocationUniform(prog, cname.as_ptr())
            };

            let cu_dt = get_uniform(compute_program, "u_dt");
            let cu_time = get_uniform(compute_program, "u_time");
            let cu_orb_x = get_uniform(compute_program, "u_orb_x");
            let cu_orb_y = get_uniform(compute_program, "u_orb_y");
            let cu_emit_rate = get_uniform(compute_program, "u_emit_rate");
            let cu_speed_base = get_uniform(compute_program, "u_speed_base");
            let cu_turbulence = get_uniform(compute_program, "u_turbulence");
            let cu_drift = get_uniform(compute_program, "u_drift");
            let cu_max_life = get_uniform(compute_program, "u_max_life");
            let cu_orb_radius = get_uniform(compute_program, "u_orb_radius");
            let cu_particle_count = get_uniform(compute_program, "u_particle_count");
            let cu_spawn_offset = get_uniform(compute_program, "u_spawn_offset");

            let ru_resolution = get_uniform(render_program, "u_resolution");
            let ru_mood = get_uniform(render_program, "u_mood");

            eprintln!("[gpu_particles] Uniform locations cached (compute: dt={}, time={}, particle_count={})",
                cu_dt, cu_time, cu_particle_count);

            Some(Self {
                compute_program,
                render_program,
                ssbos,
                quad_vao,
                quad_vbo,
                cu_dt,
                cu_time,
                cu_orb_x,
                cu_orb_y,
                cu_emit_rate,
                cu_speed_base,
                cu_turbulence,
                cu_drift,
                cu_max_life,
                cu_orb_radius,
                cu_particle_count,
                cu_spawn_offset,
                ru_resolution,
                ru_mood,
                spawn_cursor: 0,
                spawn_accum: 0.0,
                ready: true,
            })
        }
    }

    /// Spawn new particles from the orb position.
    /// Called each frame to emit particles according to the current rate.
    pub fn spawn(
        &mut self,
        dt: f32,
        orb_x: f32,
        orb_y: f32,
        rate: f32,
        speed: f32,
        _turbulence: f32,
        lifetime: f32,
        mood: crate::core::Mood,
    ) {
        if !self.ready {
            return;
        }

        self.spawn_accum += rate * dt;
        let count = self.spawn_accum as usize;
        if count == 0 {
            return;
        }
        self.spawn_accum -= count as f32;

        // Prepare spawn data for new particles
        let mut px = vec![0.0f32; count];
        let mut py = vec![0.0f32; count];
        let mut vx = vec![0.0f32; count];
        let mut vy = vec![0.0f32; count];
        let mut life = vec![0.0f32; count];
        let mut max_life = vec![0.0f32; count];
        let mut size = vec![0.0f32; count];
        let mut alpha = vec![0.0f32; count];

        let mood_mul = match mood {
            crate::core::Mood::Serene => 0.8f32,
            crate::core::Mood::Alert => 1.0,
            crate::core::Mood::Stressed => 1.3,
            crate::core::Mood::Critical => 1.8,
        };

        for i in 0..count {
            let cursor = (self.spawn_cursor + i) % GPU_PARTICLE_COUNT;
            let seed =
                crate::core::hash_f((cursor as u32).wrapping_mul(4999).wrapping_add(i as u32));
            let angle = seed * std::f32::consts::TAU;
            let spd = speed * (0.5 + seed * 0.5) * mood_mul;

            px[i] = orb_x + angle.cos() * 8.0;
            py[i] = orb_y + angle.sin() * 8.0;
            vx[i] = angle.cos() * spd;
            vy[i] = angle.sin() * spd - 10.0; // slight upward bias
            life[i] = lifetime * (0.6 + seed * 0.4);
            max_life[i] = life[i];
            size[i] = (3.0 + seed * 6.0) * mood_mul;
            alpha[i] = 0.0; // compute shader will ramp up
        }

        // Upload to SSBOs at the ring-buffer offset
        unsafe {
            let float_size = std::mem::size_of::<f32>() as u32;
            let offset = (self.spawn_cursor * std::mem::size_of::<f32>()) as u32;
            let data_size = (count * std::mem::size_of::<f32>()) as u32;

            // Handle ring-buffer wrap-around
            let remaining = GPU_PARTICLE_COUNT - self.spawn_cursor;
            if count <= remaining {
                // No wrap: single upload
                Self::upload_ssbo(self.ssbos[0], &px, offset, data_size);
                Self::upload_ssbo(self.ssbos[1], &py, offset, data_size);
                Self::upload_ssbo(self.ssbos[2], &vx, offset, data_size);
                Self::upload_ssbo(self.ssbos[3], &vy, offset, data_size);
                Self::upload_ssbo(self.ssbos[4], &life, offset, data_size);
                Self::upload_ssbo(self.ssbos[5], &max_life, offset, data_size);
                Self::upload_ssbo(self.ssbos[6], &size, offset, data_size);
                Self::upload_ssbo(self.ssbos[8], &alpha, offset, data_size);
            } else {
                // Wrap: upload in two parts
                let first_count = remaining;
                let second_count = count - first_count;
                let first_size = (first_count as u32) * float_size;
                let second_size = (second_count as u32) * float_size;

                macro_rules! upload_wrapped {
                    ($idx:expr, $data:expr) => {
                        Self::upload_ssbo(
                            self.ssbos[$idx],
                            &$data[..first_count],
                            offset,
                            first_size,
                        );
                        Self::upload_ssbo(self.ssbos[$idx], &$data[first_count..], 0, second_size);
                    };
                }
                upload_wrapped!(0, px);
                upload_wrapped!(1, py);
                upload_wrapped!(2, vx);
                upload_wrapped!(3, vy);
                upload_wrapped!(4, life);
                upload_wrapped!(5, max_life);
                upload_wrapped!(6, size);
                upload_wrapped!(8, alpha);
            }
        }

        self.spawn_cursor = (self.spawn_cursor + count) % GPU_PARTICLE_COUNT;
    }

    unsafe fn upload_ssbo(ssbo: u32, data: &[f32], offset: u32, size: u32) {
        raylib::ffi::rlUpdateShaderBuffer(
            ssbo,
            data.as_ptr() as *const std::ffi::c_void,
            size,
            offset,
        );
    }

    /// Dispatch the compute shader to advance particle physics.
    pub fn update(&mut self, dt: f32, time: f32, orb_x: f32, orb_y: f32, orb_radius: f32) {
        if !self.ready {
            return;
        }

        unsafe {
            // Flush raylib's internal batch before raw GL
            raylib::ffi::rlDrawRenderBatchActive();

            // Bind SSBOs
            for i in 0..SSBO_COUNT {
                raylib::ffi::rlBindShaderBuffer(self.ssbos[i], i as u32);
            }

            // Enable compute shader
            raylib::ffi::rlEnableShader(self.compute_program);

            // Set uniforms
            let particle_count = GPU_PARTICLE_COUNT as u32;
            raylib::ffi::rlSetUniform(
                self.cu_dt,
                &dt as *const f32 as *const _,
                RL_SHADER_UNIFORM_FLOAT,
                1,
            );
            raylib::ffi::rlSetUniform(
                self.cu_time,
                &time as *const f32 as *const _,
                RL_SHADER_UNIFORM_FLOAT,
                1,
            );
            raylib::ffi::rlSetUniform(
                self.cu_orb_x,
                &orb_x as *const f32 as *const _,
                RL_SHADER_UNIFORM_FLOAT,
                1,
            );
            raylib::ffi::rlSetUniform(
                self.cu_orb_y,
                &orb_y as *const f32 as *const _,
                RL_SHADER_UNIFORM_FLOAT,
                1,
            );
            raylib::ffi::rlSetUniform(
                self.cu_orb_radius,
                &orb_radius as *const f32 as *const _,
                RL_SHADER_UNIFORM_FLOAT,
                1,
            );
            raylib::ffi::rlSetUniform(
                self.cu_particle_count,
                &particle_count as *const u32 as *const _,
                RL_SHADER_UNIFORM_UINT,
                1,
            );

            // Dispatch: ceil(GPU_PARTICLE_COUNT / 256) work groups
            let groups = ((GPU_PARTICLE_COUNT + 255) / 256) as u32;
            raylib::ffi::rlComputeShaderDispatch(groups, 1, 1);

            raylib::ffi::rlDisableShader();
        }
    }

    /// Render all alive particles via GPU instancing.
    pub fn draw(&self, screen_w: f32, screen_h: f32, mood_float: f32) {
        if !self.ready {
            return;
        }

        unsafe {
            // Flush raylib's internal batch before raw GL
            raylib::ffi::rlDrawRenderBatchActive();

            // Bind SSBOs for vertex shader reads
            for i in 0..SSBO_COUNT {
                raylib::ffi::rlBindShaderBuffer(self.ssbos[i], i as u32);
            }

            // Enable render shader
            raylib::ffi::rlEnableShader(self.render_program);

            // Set render uniforms
            let resolution = [screen_w, screen_h];
            raylib::ffi::rlSetUniform(
                self.ru_resolution,
                resolution.as_ptr() as *const _,
                RL_SHADER_UNIFORM_VEC2,
                1,
            );
            raylib::ffi::rlSetUniform(
                self.ru_mood,
                &mood_float as *const f32 as *const _,
                RL_SHADER_UNIFORM_FLOAT,
                1,
            );

            // Bind quad VAO and draw instanced
            raylib::ffi::rlEnableVertexArray(self.quad_vao);
            raylib::ffi::rlDrawVertexArrayInstanced(0, 6, GPU_PARTICLE_COUNT as i32);
            raylib::ffi::rlDisableVertexArray();

            raylib::ffi::rlDisableShader();
        }
    }

    /// Returns true if the GPU particle system initialized successfully.
    pub fn is_ready(&self) -> bool {
        self.ready
    }
}

impl Drop for GpuParticleSystem {
    fn drop(&mut self) {
        unsafe {
            for ssbo in &self.ssbos {
                if *ssbo != 0 {
                    raylib::ffi::rlUnloadShaderBuffer(*ssbo);
                }
            }
            if self.quad_vao != 0 {
                raylib::ffi::rlUnloadVertexArray(self.quad_vao);
            }
            if self.quad_vbo != 0 {
                raylib::ffi::rlUnloadVertexBuffer(self.quad_vbo);
            }
            if self.render_program != 0 {
                raylib::ffi::rlUnloadShaderProgram(self.render_program);
            }
            // Compute program cleanup is via rlUnloadShaderProgram too
            if self.compute_program != 0 {
                raylib::ffi::rlUnloadShaderProgram(self.compute_program);
            }
        }
    }
}
