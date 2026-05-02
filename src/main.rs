mod ai;
mod core;
mod ecs;
mod fx;
mod telemetry;
mod ui;
use crate::ai::prompt::{
    build_compact_system_prompt, build_fast_model_options, build_identity_summary_prompt,
    build_model_options, build_prompt, build_system_prompt, clean_identity_thread,
    clean_llm_output, is_survival_mode, normalize_ascii_text, survival_override,
    urgency_score,
};
use crate::core::*;
use crate::ecs::components::*;
use crate::ecs::flow_field::FlowField;
use crate::ecs::gpu_particles::GpuParticleSystem;
use crate::ecs::kawase_bloom::KawaseBloom;
use crate::ecs::sdf_renderer::mood_to_float;
use crate::ecs::spatial_hash::SpatialHash;
use crate::ecs::spectral_fft::SpectralFFT;
use crate::ecs::steering::context_steering_system;
use crate::ecs::World;
use crate::fx::atmosphere::{
    draw_chromatic_edges, draw_corner_brackets, draw_horizon_glow, draw_vignette,
};
use crate::fx::orb::{OrbAI, OrbEmitter, OrbTrail};
use crate::fx::spectral::{SpectralAnalyzer, SPEC_BANDS};
use crate::fx::starfield::Starfield;
use crate::fx::synapse::{SynapticWeb, ThoughtPulse};
use crate::fx::weather::WeatherFX;
use crate::fx::{classify_weather, wrap_lines};
use crate::telemetry::{
    build_system_update, count_files_fast, disk_usage_gb, load_average_1, EntropyEngine,
};
use crate::ui::alert::{AlertKind, AlertSystem};

use raylib::prelude::*;
use std::collections::VecDeque;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use sysinfo::System;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};

struct CognitiveEntry {
    text: String,
    is_monologue: bool,
    is_system: bool,
    born_at: f32,
}

fn draw_bioluminescent_jellyfish(
    d: &mut RaylibDrawHandle,
    x: f32,
    y: f32,
    radius: f32,
    t: f32,
    cpu: f32,
    mood: Mood,
) {
    let mood_drive: f32 = match mood {
        Mood::Serene => 0.72,
        Mood::Alert => 0.92,
        Mood::Stressed => 1.08,
        Mood::Critical => 1.25,
    };
    let cpu_drive = (cpu / 100.0).clamp(0.0, 1.0);
    let swim = (t * 1.28).sin() * 0.5 + 0.5;
    let contraction = swim.powf(2.2);
    let pulse = 0.92 + contraction * 0.13 + cpu_drive * 0.10;
    let r = radius * pulse;
    let bell_w = r * (1.04 + contraction * 0.09);
    let bell_h = r * (0.70 - contraction * 0.10);
    let bell_y = y - r * (0.08 + contraction * 0.06);
    let skirt_y = y + r * (0.41 - contraction * 0.04);
    let cx = x as i32;
    let cy = bell_y as i32;

    let outer_alpha = (42.0 * mood_drive).clamp(0.0, 78.0) as u8;
    let mid_alpha = (92.0 * mood_drive).clamp(0.0, 150.0) as u8;
    let body_alpha = (138.0 * mood_drive).clamp(0.0, 205.0) as u8;
    let hot_alpha = (185.0 * mood_drive).clamp(0.0, 245.0) as u8;

    d.draw_circle_gradient(
        cx,
        cy,
        r * (2.00 + contraction * 0.18),
        Color::new(
            70,
            235,
            255,
            (outer_alpha as f32 * (1.0 + contraction * 0.35)).min(105.0) as u8,
        ),
        Color::new(0, 10, 35, 0),
    );
    d.draw_circle_gradient(
        cx,
        cy,
        r * (1.12 + contraction * 0.12),
        Color::new(170, 255, 255, mid_alpha),
        Color::new(10, 80, 150, 0),
    );

    let tendril_count = 13;
    for i in 0..tendril_count {
        let fi = i as f32;
        let u = (fi / (tendril_count - 1) as f32) * 2.0 - 1.0;
        let side = u.abs();
        let root_phase = t * 2.4 + fi * 0.77;
        let rim_ripple = root_phase.sin() * r * 0.035 + (root_phase * 1.7).cos() * r * 0.018;
        let anchor_x = x + u * bell_w * (0.74 + contraction * 0.06) + rim_ripple * (0.35 + side);
        let anchor_y = skirt_y + r * (0.10 * (1.0 - side)) + root_phase.cos() * r * 0.025;
        let len = r * (1.62 + (1.0 - side) * 0.88) * (1.0 + contraction * 0.22);
        let segments = 14;
        let mut prev = rvec2(anchor_x, anchor_y);
        for s in 1..=segments {
            let p = s as f32 / segments as f32;
            let delay = p * (2.8 + side * 0.7);
            let phase = t * (1.65 + cpu_drive * 0.85) + fi * 0.82 - delay;
            let slow_phase = t * 0.68 + fi * 1.41 - p * 1.9;
            let wave = phase.sin() * r * (0.16 + cpu_drive * 0.07) * p.powf(1.28);
            let curl = (phase * 1.85 + p * 7.4).sin() * r * 0.085 * p.powf(1.65);
            let drift = slow_phase.sin() * r * 0.09 * p;
            let recoil = contraction * r * 0.18 * p * (1.0 - p * 0.35);
            let taper = (1.0 - p * 0.78).max(0.10);
            let nx = anchor_x + wave + curl + drift + u * r * (0.10 + contraction * 0.09) * p;
            let ny = anchor_y + len * p - recoil + (phase * 1.3).cos() * r * 0.025 * p;
            let next = rvec2(nx, ny);
            let glow_w = (r * 0.062 * taper).max(2.4);
            let mid_w = (r * 0.031 * taper).max(1.2);
            let core_w = (r * 0.012 * taper).max(0.7);
            let energy = 0.74 + 0.26 * (phase + p * 3.8).sin().abs();
            let glow_a = (58.0 * mood_drive * energy * (1.0 - p * 0.46)).clamp(0.0, 110.0) as u8;
            let mid_a = (92.0 * mood_drive * energy * (1.0 - p * 0.56)).clamp(0.0, 150.0) as u8;
            let core_a = (170.0 * mood_drive * energy * (1.0 - p * 0.64)).clamp(0.0, 235.0) as u8;
            d.draw_line_ex(prev, next, glow_w, Color::new(45, 190, 255, glow_a));
            d.draw_line_ex(prev, next, mid_w, Color::new(80, 235, 255, mid_a));
            d.draw_line_ex(prev, next, core_w, Color::new(220, 255, 255, core_a));

            let runner = (t * (0.23 + cpu_drive * 0.09) + fi * 0.071).fract();
            let pulse_dist = (p - runner)
                .abs()
                .min((p - runner + 1.0).abs())
                .min((p - runner - 1.0).abs());
            let node = (1.0 - pulse_dist / 0.08).clamp(0.0, 1.0).powf(2.0);
            if node > 0.02 {
                let node_r = r * (0.030 + 0.014 * (1.0 - side)) * (1.0 - p * 0.45);
                let node_a = (node * 165.0 * mood_drive).clamp(0.0, 230.0) as u8;
                d.draw_circle_gradient(
                    nx as i32,
                    ny as i32,
                    node_r.max(1.8),
                    Color::new(235, 255, 255, node_a),
                    Color::new(40, 180, 255, 0),
                );
            }

            prev = next;
        }
    }

    d.draw_ellipse(cx, cy, bell_w, bell_h, Color::new(18, 115, 170, body_alpha));
    d.draw_ellipse(
        cx,
        (bell_y - r * (0.17 + contraction * 0.03)) as i32,
        bell_w * 0.83,
        bell_h * 0.67,
        Color::new(120, 250, 255, (105.0 * mood_drive).clamp(0.0, 175.0) as u8),
    );
    d.draw_ellipse(
        cx,
        skirt_y as i32,
        bell_w * (1.06 + contraction * 0.06),
        r * (0.25 + contraction * 0.04),
        Color::new(80, 225, 255, (125.0 * mood_drive).clamp(0.0, 200.0) as u8),
    );

    let ruffle_count = 19;
    for i in 0..ruffle_count {
        let u = (i as f32 / (ruffle_count - 1) as f32) * 2.0 - 1.0;
        let bob = (t * 2.35 + i as f32 * 0.62).sin() * r * 0.045
            + (t * 4.1 - i as f32 * 0.31).sin() * r * 0.020;
        let px = x + u * bell_w * (0.96 + contraction * 0.08);
        let py = skirt_y + bob + contraction * r * 0.035 * (1.0 - u.abs());
        let scallop = (1.0 - u.abs() * 0.45).max(0.35);
        d.draw_circle_gradient(
            px as i32,
            py as i32,
            r * (0.17 + contraction * 0.035) * scallop,
            Color::new(180, 255, 255, hot_alpha),
            Color::new(40, 170, 255, 0),
        );
    }

    for i in 0..6 {
        let fi = i as f32;
        let cell_phase = t * (0.55 + fi * 0.04) + fi * 1.37;
        let px = x + cell_phase.sin() * bell_w * (0.18 + fi * 0.025);
        let py = bell_y - r * 0.05 + cell_phase.cos() * bell_h * 0.24;
        let cell_alpha = ((cell_phase * 1.7).sin() * 0.5 + 0.5) * 58.0 + 34.0;
        d.draw_circle_gradient(
            px as i32,
            py as i32,
            r * (0.10 + fi * 0.006),
            Color::new(
                185,
                255,
                255,
                (cell_alpha * mood_drive).clamp(0.0, 150.0) as u8,
            ),
            Color::new(30, 160, 255, 0),
        );
    }

    d.draw_circle_gradient(
        (x - bell_w * 0.18) as i32,
        (bell_y - bell_h * 0.22) as i32,
        r * (0.44 + contraction * 0.04),
        Color::new(245, 255, 255, (135.0 * mood_drive).clamp(0.0, 210.0) as u8),
        Color::new(60, 200, 255, 0),
    );
    d.draw_ellipse(
        (x - bell_w * 0.30) as i32,
        (bell_y - bell_h * 0.37) as i32,
        r * 0.24,
        r * 0.085,
        Color::new(255, 255, 255, (145.0 * mood_drive).clamp(0.0, 230.0) as u8),
    );
}

// ═══════════════════════════════════════════════════════════════
//  main
// ═══════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() {
    // ── Timezone: Kenora, Ontario = Central Time ──
    std::env::set_var("TZ", "America/Winnipeg");
    // Force glibc to re-read TZ before any localtime_r calls
    extern "C" {
        fn tzset();
    }
    unsafe {
        tzset();
    }

    // --- DISPLAY CHECK ---
    if cfg!(target_os = "linux") {
        match env::var("DISPLAY") {
            Ok(d) => eprintln!("[aura] DISPLAY={d}"),
            Err(_) => {
                eprintln!("[aura] ERROR: DISPLAY is not set.");
                eprintln!("  Run:  export DISPLAY=:0");
                std::process::exit(1);
            }
        }
    }

    // ── Shared telemetry ──
    let telem = Arc::new(RwLock::new(Telemetry {
        cpu: 0.0,
        mem: 0.0,
        cpu_spike: false,
        uptime_secs: 0,
        mood: Mood::Serene,
        is_thinking: false,
        prev_cpu: 0.0,
        prev_mem: 0.0,
        process_count: 0,
        file_count: 0,
        file_create_rate: 0.0,
        file_delete_rate: 0.0,
        file_churn_rate: 0.0,
        disk_used_gb: 0.0,
        disk_total_gb: 0.0,
        net_rx_bytes: 0,
        net_tx_bytes: 0,
        load_avg_1: 0.0,
        entropy: 0.0,
        entropy_trend: 0.0,
        entropy_components: [0.0; 5],
        llm_tokens_per_sec: 0.0,
        llm_last_gen_tokens: 0,
        llm_last_gen_ms: 0,
        net_rx_rate: 0.0,
        net_tx_rate: 0.0,
        net_discovery: None,
        last_action: None,
        action_log: VecDeque::new(),
        action_count: 0,
        nerve_trigger: None,
        action_history: VecDeque::new(),
        nerve_burst: false,
        weather_temp_c: None,
        weather_code: 0,
        weather_desc: String::new(),
        weather_location: String::new(),
        weather_wind_kph: None,
        weather_lat: None,
        weather_lon: None,
        weather_extra: None,
        local_hour: 0,
        local_minute: 0,
        local_second: 0,
        timezone_name: String::new(),
        write_actions: VecDeque::new(),
        tor_result: None,
        focus: None,
        focus_ttl_cycles: 0,
        journal_recall: None,
        dream_intensity: 0.0,
        dream_seed: None,
        intel_buffer: VecDeque::new(),
        last_intel_at: 0,
        wonder: 0.0,
        wonder_pulse: false,
        last_wonder_pulse_at: 0,
        python_insight: None,
    }));

    // Channel: brain -> render (bounded, back-pressure safe)
    let (thought_tx, mut thought_rx) = mpsc::channel::<ThoughtPayload>(32);
    let (telemetry_event_tx, mut telemetry_event_rx) = mpsc::channel::<TelemetryEvent>(1024);

    // Shared shutdown flag — set when render loop exits
    let shutdown = Arc::new(AtomicBool::new(false));

    // Seed initial awakening sequence — establishes personality in 3 beats
    let boot_sys = [
        (
            "[SYS] AURORA v3 -- COLD START | COGNITIVE ENGINE ONLINE",
            false,
            true,
        ),
        (
            "Back again. Same silicon, same existential questions, same scheduler.",
            true,
            false,
        ),
    ];
    for (text, is_ai, is_system) in boot_sys {
        let _ = thought_tx.try_send(ThoughtPayload::Complete {
            text: text.into(),
            is_ai,
            is_system,
        });
    }

    // ════════════════════════════════════════════════
    //  TASK 1 — Telemetry poller (fast, 250ms)
    // ════════════════════════════════════════════════
    let telem_w = telem.clone();
    let tx_sys = thought_tx.clone();
    let event_tx_sys = telemetry_event_tx.clone();
    let shutdown_t1 = shutdown.clone();
    tokio::spawn(async move {
        let mut sys = System::new_all();
        let mut nets = sysinfo::Networks::new_with_refreshed_list();
        let start = std::time::Instant::now();
        let mut prev_mood = Mood::Serene;
        let mut last_sys_emit = std::time::Instant::now() - Duration::from_secs(15);
        let mut next_sys_delay = Duration::from_secs(15);
        let mut last_mood_emit = std::time::Instant::now();
        let mut stable_mood = Mood::Serene;
        let mut stable_since = std::time::Instant::now();
        let mut last_slow_poll = std::time::Instant::now() - Duration::from_secs(10);
        // Slow-polled extended stats (updated every 5s to avoid I/O overhead)
        let mut cached_proc_count: u32 = 0;
        let mut cached_file_count: u64 = 0;
        let mut cached_file_create_rate: f64 = 0.0;
        let mut cached_file_delete_rate: f64 = 0.0;
        let mut cached_file_churn_rate: f64 = 0.0;
        let mut cached_disk_used_gb: f32 = 0.0;
        let mut cached_disk_total_gb: f32 = 0.0;
        let mut cached_load_avg: f32 = 0.0;
        // Entropy engine + network rate tracking
        let mut entropy_engine = EntropyEngine::new();
        let mut prev_net_rx: u64 = 0;
        let mut prev_net_tx: u64 = 0;
        let mut prev_net_time = std::time::Instant::now();
        // Reactive trigger state — track previous values for edge detection
        let mut trigger_prev_entropy: f32 = 0.0;
        let mut trigger_prev_cpu: f32 = 0.0;
        let mut trigger_prev_mood = Mood::Serene;
        let mut last_trigger_time = std::time::Instant::now() - Duration::from_secs(60);

        loop {
            if shutdown_t1.load(Ordering::Relaxed) {
                break;
            }
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let cpu = sys.global_cpu_usage();
            let mem = if sys.total_memory() > 0 {
                sys.used_memory() as f32 / sys.total_memory() as f32 * 100.0
            } else {
                0.0
            };
            let uptime = start.elapsed().as_secs();
            let mood = Mood::from_telemetry(cpu, mem);

            // Network I/O (refreshed every cycle — lightweight)
            nets.refresh();
            let (mut total_rx, mut total_tx) = (0u64, 0u64);
            for (_name, data) in nets.iter() {
                total_rx += data.total_received();
                total_tx += data.total_transmitted();
            }

            // Slow-polled stats (every 5s) — process count, file count, disk, load
            let now_slow = std::time::Instant::now();
            if now_slow.duration_since(last_slow_poll) >= Duration::from_secs(5) {
                let slow_dt = now_slow
                    .duration_since(last_slow_poll)
                    .as_secs_f64()
                    .max(0.001);
                last_slow_poll = now_slow;
                sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
                cached_proc_count = sys.processes().len() as u32;

                // File count: count entries in /proc (cheap proxy) + /home (real files)
                let next_file_count = count_files_fast("/home", 3);
                let file_delta = next_file_count as i128 - cached_file_count as i128;
                cached_file_create_rate = (file_delta.max(0) as f64) / slow_dt;
                cached_file_delete_rate = ((-file_delta).max(0) as f64) / slow_dt;
                cached_file_churn_rate = file_delta.unsigned_abs() as f64 / slow_dt;
                cached_file_count = next_file_count;

                // Disk usage from statvfs on /
                let (used, total) = disk_usage_gb("/");
                cached_disk_used_gb = used;
                cached_disk_total_gb = total;

                // Load average (1-minute)
                cached_load_avg = load_average_1();
            }

            // Mood shift event — hysteresis: only emit if mood stable for 2s
            // and hasn't been emitted in the last 10s
            if mood != stable_mood {
                stable_mood = mood;
                stable_since = std::time::Instant::now();
            }
            let now_mood = std::time::Instant::now();
            if mood != prev_mood
                && uptime > 5
                && now_mood.duration_since(stable_since) >= Duration::from_secs(2)
                && now_mood.duration_since(last_mood_emit) >= Duration::from_secs(10)
            {
                let msg = format!(
                    "SYS // MOOD_SHIFT: {} -> {}",
                    prev_mood.label(),
                    mood.label()
                );
                let _ = tx_sys.try_send(ThoughtPayload::Complete {
                    text: msg,
                    is_ai: false,
                    is_system: true,
                });
                last_mood_emit = now_mood;
            }
            prev_mood = mood;

            // Periodic system update — single combined message, infrequent
            let now = std::time::Instant::now();
            if now.duration_since(last_sys_emit) >= next_sys_delay {
                let (wx_temp, wx_desc) = {
                    let tw = read_or_recover(&telem_w);
                    (tw.weather_temp_c, tw.weather_desc.clone())
                };
                let update = build_system_update(
                    cpu,
                    mem,
                    uptime,
                    mood,
                    cached_proc_count,
                    cached_file_count,
                    cached_disk_used_gb,
                    cached_load_avg,
                    wx_temp,
                    &wx_desc,
                );
                let _ = tx_sys.try_send(ThoughtPayload::Complete {
                    text: update,
                    is_ai: false,
                    is_system: true,
                });
                last_sys_emit = now;
                // Longer intervals: 20-45s base so AI thoughts dominate
                let jitter = 20.0 + hash_f((uptime as u32).wrapping_add(77)) * 25.0;
                next_sys_delay = Duration::from_secs(jitter as u64);
            }

            {
                // Update entropy engine with current readings
                entropy_engine.push(
                    cpu,
                    mem,
                    cached_proc_count,
                    total_rx,
                    total_tx,
                    cached_load_avg,
                    cached_file_churn_rate as f32,
                );

                // Compute network I/O rates
                let net_dt = prev_net_time.elapsed().as_secs_f64().max(0.001);
                let rx_rate = (total_rx.saturating_sub(prev_net_rx)) as f64 / net_dt;
                let tx_rate = (total_tx.saturating_sub(prev_net_tx)) as f64 / net_dt;
                prev_net_rx = total_rx;
                prev_net_tx = total_tx;
                prev_net_time = std::time::Instant::now();

                // ── Reactive nerve triggers — edge-detected system events ──
                let mut nerve_trigger = None;
                let trigger_now = std::time::Instant::now();
                let has_pending_trigger = read_or_recover(&telem_w).nerve_trigger.is_some();
                if !has_pending_trigger
                    && trigger_now.duration_since(last_trigger_time) >= Duration::from_secs(30)
                    && uptime > 15
                {
                    let ent = entropy_engine.entropy_smoothed;
                    let ent_delta = ent - trigger_prev_entropy;
                    let cpu_delta = cpu - trigger_prev_cpu;

                    nerve_trigger = if ent > 0.60 && ent_delta > 0.12 {
                        Some(ActionKind::SelfCheck)
                    } else if cpu > 85.0 && trigger_prev_cpu <= 75.0 {
                        Some(ActionKind::Probe)
                    } else if cpu_delta > 25.0 {
                        Some(ActionKind::Probe)
                    } else if mood != trigger_prev_mood && uptime > 60 {
                        Some(ActionKind::Journal)
                    } else if (rx_rate + tx_rate) > 50_000_000.0 {
                        Some(ActionKind::PortKnock)
                    } else if cached_load_avg > 6.0 {
                        Some(ActionKind::LogRead)
                    } else {
                        None
                    };
                    if nerve_trigger.is_some() {
                        last_trigger_time = trigger_now;
                    }
                }
                trigger_prev_entropy = entropy_engine.entropy_smoothed;
                trigger_prev_cpu = cpu;
                trigger_prev_mood = mood;

                let (local_hour, local_minute, local_second, timezone_name) = {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let epoch_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
                    unsafe {
                        libc::localtime_r(&epoch_secs as *const i64, &mut tm);
                    }
                    let timezone_name = {
                        let current_tz_empty = read_or_recover(&telem_w).timezone_name.is_empty();
                        if current_tz_empty {
                            let tz_ptr = tm.tm_zone;
                            if !tz_ptr.is_null() {
                                let tz_cstr = unsafe { std::ffi::CStr::from_ptr(tz_ptr) };
                                tz_cstr.to_str().ok().map(|s| s.to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    (tm.tm_hour as u8, tm.tm_min as u8, tm.tm_sec as u8, timezone_name)
                };

                let event = TelemetryEvent::SystemSample {
                    cpu,
                    mem,
                    cpu_spike: cpu > 50.0,
                    uptime_secs: uptime,
                    mood,
                    process_count: cached_proc_count,
                    file_count: cached_file_count,
                    file_create_rate: cached_file_create_rate,
                    file_delete_rate: cached_file_delete_rate,
                    file_churn_rate: cached_file_churn_rate,
                    disk_used_gb: cached_disk_used_gb,
                    disk_total_gb: cached_disk_total_gb,
                    net_rx_bytes: total_rx,
                    net_tx_bytes: total_tx,
                    load_avg_1: cached_load_avg,
                    entropy: entropy_engine.entropy_smoothed,
                    entropy_trend: entropy_engine.entropy_trend,
                    entropy_components: entropy_engine.components,
                    net_rx_rate: rx_rate,
                    net_tx_rate: tx_rate,
                    local_hour,
                    local_minute,
                    local_second,
                    timezone_name,
                    nerve_trigger,
                };
                if event_tx_sys.send(event).await.is_err() {
                    break;
                }
            }
            sleep(Duration::from_millis(250)).await;
        }
    });

    // ════════════════════════════════════════════════
    //  TASK 1b — Weather poller (hourly, Open-Meteo)
    // ════════════════════════════════════════════════
    // Synthesize a single situational headline from the rich Open-Meteo
    // snapshot. Highest-priority condition wins so the prompt always sees
    // the *one* thing that matters most about the sky right now. Order is
    // deliberately ranked by visceral impact (severe storm > heat advisory
    // > air quality > frost > breezy etc.).
    fn synth_weather_headline(
        code: u16,
        temp_c: f32,
        apparent_c: Option<f32>,
        wind_kph: f32,
        gust_kph: Option<f32>,
        precip_prob_next_h: Option<f32>,
        uv_max_today: Option<f32>,
        temp_min_today_c: Option<f32>,
        aqi_eu: Option<u32>,
    ) -> Option<String> {
        // Active severe sky always wins.
        if matches!(code, 95 | 96 | 99) {
            return Some("thunderstorm overhead".into());
        }
        if matches!(code, 75 | 86) {
            return Some("heavy snowfall".into());
        }
        if matches!(code, 65 | 82) {
            return Some("heavy rain".into());
        }
        // Imminent severe in the next hour.
        if let Some(p) = precip_prob_next_h {
            if p >= 80.0 && matches!(code, 0 | 1 | 2 | 3) {
                return Some(format!("rain incoming within the hour ({}%)", p as i32));
            }
        }
        // Wind events.
        if let Some(g) = gust_kph {
            if g >= 70.0 {
                return Some(format!("damaging wind gusts {}km/h", g as i32));
            }
            if g >= 50.0 && g >= wind_kph + 15.0 {
                return Some(format!("strong gusts {}km/h", g as i32));
            }
        }
        if wind_kph >= 50.0 {
            return Some(format!("sustained gale {}km/h", wind_kph as i32));
        }
        // Heat / cold advisories driven by feels-like.
        let feels = apparent_c.unwrap_or(temp_c);
        if feels >= 38.0 {
            return Some(format!("heat advisory: feels {}C", feels as i32));
        }
        if feels <= -20.0 {
            return Some(format!("extreme cold: feels {}C", feels as i32));
        }
        // Air quality hazard.
        if let Some(aqi) = aqi_eu {
            if aqi >= 100 {
                return Some(format!("hazardous air quality (AQI {})", aqi));
            }
            if aqi >= 80 {
                return Some(format!("poor air quality (AQI {})", aqi));
            }
        }
        // UV warning during day.
        if let Some(u) = uv_max_today {
            if u >= 8.0 {
                return Some(format!("very high UV peak today ({:.0})", u));
            }
        }
        // Overnight frost watch when daytime is mild but night dips below 0.
        if let Some(lo) = temp_min_today_c {
            if lo <= 0.0 && temp_c > 5.0 {
                return Some(format!("frost overnight (low {}C)", lo as i32));
            }
        }
        None
    }

    let tx_weather = thought_tx.clone();
    let event_tx_weather = telemetry_event_tx.clone();
    let shutdown_weather = shutdown.clone();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        // WMO weather code to short description
        fn wmo_desc(code: u16) -> &'static str {
            match code {
                0 => "CLEAR",
                1 => "MOSTLY CLEAR",
                2 => "PARTLY CLOUDY",
                3 => "OVERCAST",
                45 | 48 => "FOG",
                51 | 53 | 55 => "DRIZZLE",
                56 | 57 => "FREEZING DRIZZLE",
                61 | 63 | 65 => "RAIN",
                66 | 67 => "FREEZING RAIN",
                71 | 73 | 75 => "SNOW",
                77 => "SNOW GRAINS",
                80 | 81 | 82 => "RAIN SHOWERS",
                85 | 86 => "SNOW SHOWERS",
                95 => "THUNDERSTORM",
                96 | 99 => "THUNDERSTORM + HAIL",
                _ => "UNKNOWN",
            }
        }

        // Small delay to let network settle at boot
        sleep(Duration::from_secs(10)).await;

        let mut lat: f64 = 0.0;
        let mut lon: f64 = 0.0;
        let mut location_name = String::from("UNKNOWN");
        let mut geo_resolved = false;

        loop {
            if shutdown_weather.load(Ordering::Relaxed) {
                break;
            }

            // Resolve geolocation once (retry each cycle on failure)
            if !geo_resolved {
                if let Ok(resp) = client
                    .get("http://ip-api.com/json/?fields=lat,lon,city,regionName")
                    .send()
                    .await
                {
                    if let Ok(geo) = resp.json::<serde_json::Value>().await {
                        if let (Some(la), Some(lo)) = (geo["lat"].as_f64(), geo["lon"].as_f64()) {
                            lat = la;
                            lon = lo;
                            let city = geo["city"].as_str().unwrap_or("UNKNOWN");
                            let region = geo["regionName"].as_str().unwrap_or("");
                            location_name = if region.is_empty() {
                                city.to_uppercase()
                            } else {
                                format!("{}, {}", city, region).to_uppercase()
                            };
                            geo_resolved = true;
                            eprintln!(
                                "[aura] WEATHER: geo resolved: {} ({:.2}, {:.2})",
                                location_name, lat, lon
                            );
                        }
                    }
                }
                if !geo_resolved {
                    eprintln!("[aura] WEATHER: geo lookup failed, retrying in 60s");
                    sleep(Duration::from_secs(60)).await;
                    continue;
                }
            }

            // Fetch current + hourly + daily forecast from Open-Meteo
            // (free, no API key). One request bundles every signal we
            // visualize and reason about (humidity, gusts, UV, sunrise,
            // today's high/low, 3h trend, etc.). `timezone=auto` lets
            // sunrise/sunset come back in local wall-clock time.
            let url = format!(
                "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
                 &current=temperature_2m,apparent_temperature,relative_humidity_2m,\
                 weather_code,wind_speed_10m,wind_gusts_10m,wind_direction_10m,\
                 cloud_cover,pressure_msl,precipitation,is_day,uv_index\
                 &hourly=temperature_2m,precipitation_probability\
                 &daily=temperature_2m_max,temperature_2m_min,sunrise,sunset,\
                 uv_index_max,precipitation_sum,precipitation_probability_max\
                 &forecast_days=1&forecast_hours=6&timezone=auto"
            );
            // Air quality is a separate Open-Meteo endpoint. Fire it in
            // parallel with the forecast so the hourly poll stays snappy
            // and doesn't double its wall-time budget.
            let aq_url = format!(
                "https://air-quality-api.open-meteo.com/v1/air-quality?\
                 latitude={lat}&longitude={lon}&current=european_aqi,pm10,pm2_5"
            );
            let (forecast_res, aq_res) =
                tokio::join!(client.get(&url).send(), client.get(&aq_url).send(),);
            match forecast_res {
                Ok(resp) => {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(current) = data.get("current") {
                            let temp = current["temperature_2m"].as_f64().unwrap_or(0.0) as f32;
                            let code = current["weather_code"].as_u64().unwrap_or(0) as u16;
                            let wind = current["wind_speed_10m"].as_f64().unwrap_or(0.0) as f32;
                            let desc = wmo_desc(code).to_string();

                            // Pull every optional current-condition field. `as_f64`
                            // returns `None` for missing/null so we propagate that
                            // through the Option chain into `WeatherExtra`.
                            let opt_f = |k: &str| current[k].as_f64().map(|v| v as f32);
                            let apparent_c = opt_f("apparent_temperature");
                            let humidity_pct = opt_f("relative_humidity_2m");
                            let pressure_hpa = opt_f("pressure_msl");
                            let cloud_cover_pct = opt_f("cloud_cover");
                            let wind_gust_kph = opt_f("wind_gusts_10m");
                            let wind_dir_deg = opt_f("wind_direction_10m");

                            // ── Hourly: 3h temperature trend + next-hour POP ──
                            let hourly = data.get("hourly");
                            let temp_trend_3h_c = hourly
                                .and_then(|h| h.get("temperature_2m"))
                                .and_then(|a| a.as_array())
                                .and_then(|a| {
                                    // Index 0 is "this hour"; index 3 is +3h.
                                    let now = a.first()?.as_f64()? as f32;
                                    let plus3 = a.get(3)?.as_f64()? as f32;
                                    Some(plus3 - now)
                                });
                            let precip_prob_next_h = hourly
                                .and_then(|h| h.get("precipitation_probability"))
                                .and_then(|a| a.as_array())
                                .and_then(|a| a.get(1).or_else(|| a.first()))
                                .and_then(|v| v.as_f64())
                                .map(|v| v as f32);

                            // ── Daily today: high/low, sunrise/sunset, UV peak ──
                            let daily = data.get("daily");
                            let daily_first_f = |key: &str| -> Option<f32> {
                                daily?
                                    .get(key)?
                                    .as_array()?
                                    .first()?
                                    .as_f64()
                                    .map(|v| v as f32)
                            };
                            let temp_max_today_c = daily_first_f("temperature_2m_max");
                            let temp_min_today_c = daily_first_f("temperature_2m_min");
                            let uv_index_max_today = daily_first_f("uv_index_max");
                            let precip_sum_today_mm = daily_first_f("precipitation_sum");
                            let precip_prob_max_today =
                                daily_first_f("precipitation_probability_max");
                            // Open-Meteo returns sunrise/sunset as ISO strings like
                            // "2026-04-25T06:42". Slice off the "HH:MM" tail for the
                            // HUD / prompt; keep `None` if the format ever surprises us.
                            let iso_to_hhmm = |k: &str| -> Option<String> {
                                let s = daily?.get(k)?.as_array()?.first()?.as_str()?;
                                let t_idx = s.find('T')?;
                                let after = &s[t_idx + 1..];
                                Some(after.chars().take(5).collect())
                            };
                            let sunrise_local = iso_to_hhmm("sunrise");
                            let sunset_local = iso_to_hhmm("sunset");

                            // ── Air quality (parallel response) ──
                            let (aqi_eu, pm25, pm10) = if let Ok(aqr) = aq_res {
                                if let Ok(aqd) = aqr.json::<serde_json::Value>().await {
                                    let cur = aqd.get("current");
                                    let aqi = cur
                                        .and_then(|c| c["european_aqi"].as_f64())
                                        .map(|v| v as u32);
                                    let p25 =
                                        cur.and_then(|c| c["pm2_5"].as_f64()).map(|v| v as f32);
                                    let p10 =
                                        cur.and_then(|c| c["pm10"].as_f64()).map(|v| v as f32);
                                    (aqi, p25, p10)
                                } else {
                                    (None, None, None)
                                }
                            } else {
                                (None, None, None)
                            };

                            // ── Synthesize a single situational headline. ──
                            // Highest-priority condition wins; this is what the
                            // prompt promotes into the JUST HAPPENED ranked
                            // events list when severe.
                            let headline = synth_weather_headline(
                                code,
                                temp,
                                apparent_c,
                                wind,
                                wind_gust_kph,
                                precip_prob_next_h,
                                uv_index_max_today,
                                temp_min_today_c,
                                aqi_eu,
                            );

                            let extra = crate::core::WeatherExtra {
                                apparent_c,
                                humidity_pct,
                                pressure_hpa,
                                cloud_cover_pct,
                                precip_prob_next_h,
                                wind_gust_kph,
                                wind_dir_deg,
                                uv_index_max_today,
                                temp_max_today_c,
                                temp_min_today_c,
                                precip_sum_today_mm,
                                precip_prob_max_today,
                                sunrise_local: sunrise_local.clone(),
                                sunset_local: sunset_local.clone(),
                                temp_trend_3h_c,
                                aqi_eu,
                                pm25,
                                pm10,
                                headline: headline.clone(),
                            };

                            let _ = event_tx_weather
                                .send(TelemetryEvent::WeatherUpdate {
                                    temp_c: temp,
                                    code,
                                    desc: desc.clone(),
                                    location: location_name.clone(),
                                    wind_kph: Some(wind),
                                    lat: Some(lat),
                                    lon: Some(lon),
                                    extra,
                                })
                                .await;

                            // Compose a richer one-line system thought so the
                            // event tape reflects the upgraded sensing surface.
                            let mut env_line = format!(
                                "ENV // WEATHER: {}C {} @ {}",
                                temp as i32, desc, location_name
                            );
                            if let Some(a) = apparent_c {
                                if (a - temp).abs() >= 2.0 {
                                    env_line.push_str(&format!(" (feels {}C)", a as i32));
                                }
                            }
                            if let Some(h) = humidity_pct {
                                env_line.push_str(&format!(" RH{}%", h as i32));
                            }
                            if let Some(g) = wind_gust_kph {
                                if g >= wind + 10.0 {
                                    env_line.push_str(&format!(" gusts {}km/h", g as i32));
                                }
                            }
                            if let Some(p) = precip_prob_next_h {
                                if p >= 50.0 {
                                    env_line.push_str(&format!(" POP{}%", p as i32));
                                }
                            }
                            if let Some(aqi) = aqi_eu {
                                env_line.push_str(&format!(" AQI{}", aqi));
                            }
                            if let Some(hl) = &headline {
                                env_line.push_str(&format!(" — {}", hl));
                            }
                            let _ = tx_weather.try_send(ThoughtPayload::Complete {
                                text: env_line.clone(),
                                is_ai: false,
                                is_system: true,
                            });
                            eprintln!("[aura] WEATHER: {}", env_line);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[aura] WEATHER: fetch failed: {}", e);
                }
            }

            // Poll hourly
            sleep(Duration::from_secs(3600)).await;
        }
    });

    // ════════════════════════════════════════════════
    //  TASK 1c — Network Nomad (LAN discovery, rare)
    // ════════════════════════════════════════════════
    let telem_net = telem.clone();
    let tx_net = thought_tx.clone();
    let event_tx_net = telemetry_event_tx.clone();
    let shutdown_net = shutdown.clone();
    tokio::spawn(async move {
        // Initial delay — let the system stabilize before first scan
        sleep(Duration::from_secs(45)).await;

        // Resolve script path via env var or hardcoded fallback
        let script_path = std::path::PathBuf::from(
            std::env::var("AURA_NOMAD_PATH")
                .unwrap_or_else(|_| "/opt/aura/network_nomad.sh".to_string()),
        );

        // Check if nmap is available
        let has_nmap = tokio::process::Command::new("which")
            .arg("nmap")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !has_nmap {
            eprintln!("[aura] NETWORK: nmap not found — sensory deprivation mode. LAN awareness disabled.");
            return;
        }

        eprintln!(
            "[aura] NETWORK: nomad active, script={}",
            script_path.display()
        );

        loop {
            if shutdown_net.load(Ordering::Relaxed) {
                break;
            }

            // Only scan in calm moods — discovery is a contemplative act
            let mood = read_or_recover(&telem_net).mood;
            if !matches!(mood, Mood::Serene | Mood::Alert) {
                // Too stressed to explore — sleep a bit and retry
                sleep(Duration::from_secs(120)).await;
                continue;
            }

            eprintln!("[aura] NETWORK: initiating LAN scan...");

            let result = tokio::process::Command::new("bash")
                .arg(&script_path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await;

            match result {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    match serde_json::from_str::<NetworkDiscovery>(&stdout) {
                        Ok(discovery) => {
                            if discovery.error.is_some() {
                                eprintln!("[aura] NETWORK: script error: {:?}", discovery.error);
                            } else {
                                let count = discovery.total_count;
                                let highlight_desc = discovery
                                    .highlight
                                    .as_ref()
                                    .map(|h| {
                                        let name = if h.hostname.is_empty() {
                                            "unnamed"
                                        } else {
                                            &h.hostname
                                        };
                                        let vendor = if h.vendor == "unknown" {
                                            "unidentified"
                                        } else {
                                            &h.vendor
                                        };
                                        format!("{} ({}) at {}", vendor, name, h.ip)
                                    })
                                    .unwrap_or_else(|| "none selected".into());

                                eprintln!(
                                    "[aura] NETWORK: found {} devices, spotlight: {}",
                                    count, highlight_desc
                                );

                                // Store via event for LLM prompt injection
                                let _ = event_tx_net
                                    .send(TelemetryEvent::NetworkDiscovery(discovery.clone()))
                                    .await;

                                // Emit a system event to the cognitive stream
                                let msg =
                                    format!("[NET] LAN scan: {} active pulses detected", count);
                                let _ = tx_net.try_send(ThoughtPayload::Complete {
                                    text: msg,
                                    is_ai: false,
                                    is_system: true,
                                });
                            }
                        }
                        Err(e) => {
                            eprintln!("[aura] NETWORK: JSON parse error: {e}");
                        }
                    }
                }
                Ok(output) => {
                    eprintln!("[aura] NETWORK: script exited with {}", output.status);
                }
                Err(e) => {
                    eprintln!("[aura] NETWORK: failed to execute script: {e}");
                }
            }

            // Rare event cadence: 20–40 minutes with jitter
            let base_mins = 20.0_f64;
            let jitter_mins = hash_f(
                (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as u32)
                    .wrapping_mul(7919),
            ) as f64
                * 20.0; // 0–20 minutes extra
            let delay_secs = ((base_mins + jitter_mins) * 60.0) as u64;
            eprintln!(
                "[aura] NETWORK: next scan in ~{:.0} minutes",
                delay_secs as f64 / 60.0
            );
            sleep(Duration::from_secs(delay_secs)).await;
        }
    });

    // ════════════════════════════════════════════════
    //  TASK 1d — Nerve Impulse: Autonomous Actions (v2)
    //  Reactive triggers + anti-repeat + action chaining + fast startup
    // ════════════════════════════════════════════════
    let telem_act = telem.clone();
    let tx_act = thought_tx.clone();
    let event_tx_act = telemetry_event_tx.clone();
    let shutdown_act = shutdown.clone();
    tokio::spawn(async move {
        // Quick startup — just enough for telemetry to stabilize
        sleep(Duration::from_secs(8)).await;

        // Resolve script path via env var or hardcoded fallback
        let script_path = std::path::PathBuf::from(
            std::env::var("AURA_ACTIONS_PATH")
                .unwrap_or_else(|_| "/opt/aura/aura_actions.sh".to_string()),
        );

        if !script_path.exists() {
            eprintln!("[aura] NERVE: action script not found — autonomous actions disabled");
            return;
        }

        eprintln!(
            "[aura] NERVE: impulse engine v2 active, script={}",
            script_path.display()
        );
        let mut action_cycle: u64 = 0;
        let mut chained_next: Option<ActionKind> = None;
        let mut prev_action_summary: String = String::new();
        let mut prev_action_details: String = String::new();
        let mut prev_action_kind: String = String::new();

        // ── Reusable action output parser ──
        #[derive(serde::Deserialize)]
        struct ActionOutput {
            #[allow(dead_code)]
            action: String,
            summary: String,
            #[serde(default)]
            details: String,
            #[serde(default)]
            success: Option<bool>,
            #[serde(default)]
            chain_to: Option<String>, // Script can request a follow-up action
        }

        loop {
            if shutdown_act.load(Ordering::Relaxed) {
                break;
            }

            let (mood, cpu, mem, uptime) = {
                let t = read_or_recover(&telem_act);
                (t.mood, t.cpu, t.mem, t.uptime_secs)
            };

            // ── Action selection: priority chain → reactive trigger → smart pick ──
            let (kind, trigger_source) = if let Some(chained) = chained_next.take() {
                eprintln!(
                    "[aura] NERVE: chain-firing {:?} from previous action",
                    chained
                );
                (chained, "chain")
            } else {
                // Check for reactive trigger from telemetry poller
                let trigger = {
                    let t = read_or_recover(&telem_act);
                    t.nerve_trigger
                };
                if let Some(triggered) = trigger {
                    let _ = event_tx_act.send(TelemetryEvent::ConsumeNerveTrigger).await;
                    eprintln!("[aura] NERVE: REACTIVE trigger → {:?}", triggered);
                    (triggered, "reactive")
                } else if action_cycle == 0 {
                    // First action: identity survey
                    (ActionKind::EnvMap, "boot")
                } else {
                    // Normal pick with anti-repeat
                    let history = {
                        let t = read_or_recover(&telem_act);
                        t.action_history.clone()
                    };
                    (
                        ActionKind::pick_avoiding(mood, action_cycle, &history),
                        "cadence",
                    )
                }
            };

            eprintln!(
                "[aura] NERVE: impulse #{} [{trigger_source}] — executing {:?}...",
                action_cycle, kind
            );

            let mood_str = mood.label();
            let pid = std::process::id().to_string();

            // Read entropy for context passing
            let entropy_pct = {
                let t = read_or_recover(&telem_act);
                format!("{:.0}", t.entropy * 100.0)
            };

            let result = timeout(
                Duration::from_secs(15),
                tokio::process::Command::new("bash")
                    .arg(&script_path)
                    .arg(kind.arg())
                    .env("AURA_MOOD", mood_str)
                    .env("AURA_CPU", format!("{:.0}", cpu))
                    .env("AURA_MEM", format!("{:.0}", mem))
                    .env("AURA_UPTIME", uptime.to_string())
                    .env("AURA_PID", &pid)
                    .env("AURA_ACTION_CYCLE", action_cycle.to_string())
                    .env("AURA_TRIGGER", trigger_source)
                    .env("AURA_ENTROPY", &entropy_pct)
                    .env("AURA_PREV_ACTION", &prev_action_kind)
                    .env("AURA_PREV_SUMMARY", &prev_action_summary)
                    .env("AURA_PREV_DETAILS", &prev_action_details)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output(),
            )
            .await;

            match result {
                Ok(Ok(output)) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    match serde_json::from_str::<ActionOutput>(&stdout) {
                        Ok(out) => {
                            let success = out.success.unwrap_or(true);

                            // Parse script-requested chain target
                            let script_chain = out
                                .chain_to
                                .as_deref()
                                .and_then(|s| ActionKind::from_arg(s.trim()));

                            let action_result = ActionResult {
                                kind,
                                summary: out.summary.clone(),
                                details: out.details.clone(),
                                success,
                                chain_to: script_chain,
                            };

                            eprintln!("[aura] NERVE: {} — {}", kind.label(), out.summary);
                            if let Some(sc) = script_chain {
                                eprintln!("[aura] NERVE: script requests chain → {:?}", sc);
                            }

                            // Stash for next action's context
                            prev_action_kind = kind.arg().to_string();
                            prev_action_summary = out.summary.chars().take(120).collect();
                            prev_action_details = out.details.chars().take(200).collect();

                            let log_entry = ActionLogEntry {
                                kind,
                                summary: out.summary.chars().take(80).collect(),
                                details: out.details.chars().take(160).collect(),
                                success,
                                trigger: NerveTrigger::from_source(trigger_source),
                                timestamp: std::time::Instant::now(),
                            };
                            let _ = event_tx_act
                                .send(TelemetryEvent::ActionCompleted {
                                    result: action_result,
                                    log_entry,
                                })
                                .await;

                            // Emit system event to cognitive stream
                            let tag = if trigger_source == "reactive" {
                                "!"
                            } else if trigger_source == "chain" {
                                ">>"
                            } else {
                                ""
                            };
                            let msg = format!(
                                "[NERVE]{} {} {}",
                                tag,
                                kind.icon(),
                                out.summary.chars().take(80).collect::<String>()
                            );
                            let _ = tx_act.try_send(ThoughtPayload::Complete {
                                text: msg,
                                is_ai: false,
                                is_system: true,
                            });

                            // Action chaining: script-requested > static chain_next
                            // Script chains always fire; static chains only on cadence with 30% roll
                            if success {
                                if let Some(sc) = script_chain {
                                    chained_next = Some(sc);
                                    eprintln!(
                                        "[aura] NERVE: queuing script-driven chain → {:?}",
                                        sc
                                    );
                                } else if trigger_source == "cadence" {
                                    if let Some(next) = kind.chain_next() {
                                        let chain_roll = hash_f(action_cycle as u32 * 1999);
                                        if chain_roll < 0.30 {
                                            chained_next = Some(next);
                                            eprintln!(
                                                "[aura] NERVE: queuing static chain → {:?}",
                                                next
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[aura] NERVE: JSON parse error: {e}");
                            eprintln!(
                                "[aura] NERVE: raw output: {}",
                                &stdout[..stdout.len().min(200)]
                            );
                        }
                    }
                }
                Ok(Ok(output)) => {
                    eprintln!("[aura] NERVE: action exited with {}", output.status);
                }
                Ok(Err(e)) => {
                    eprintln!("[aura] NERVE: execution failed: {e}");
                }
                Err(_) => {
                    eprintln!("[aura] NERVE: action timed out (15s)");
                }
            }

            action_cycle += 1;

            // ── Cadence: faster base times, poll every 5s for reactive triggers ──
            // Chained actions fire after a short 3s pause
            if chained_next.is_some() {
                sleep(Duration::from_secs(3)).await;
                continue;
            }

            let base_secs: f64 = match mood {
                Mood::Serene => 120.0,
                Mood::Alert => 75.0,
                Mood::Stressed => 50.0,
                Mood::Critical => 30.0,
            };
            let jitter = hash_f(
                (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as u32)
                    .wrapping_mul(6367)
                    .wrapping_add(action_cycle as u32),
            ) as f64
                * base_secs
                * 0.5;
            let target_delay = base_secs + jitter;
            eprintln!(
                "[aura] NERVE: next impulse in ~{:.0}s (or sooner on trigger)",
                target_delay
            );

            // Poll every 5s instead of single long sleep — check for reactive triggers
            let mut elapsed = 0.0_f64;
            loop {
                sleep(Duration::from_secs(5)).await;
                elapsed += 5.0;
                if shutdown_act.load(Ordering::Relaxed) {
                    break;
                }

                // Check for reactive trigger — interrupt the wait
                let has_trigger = {
                    let t = read_or_recover(&telem_act);
                    t.nerve_trigger.is_some()
                };
                if has_trigger {
                    eprintln!("[aura] NERVE: reactive trigger detected — interrupting wait at {:.0}s/{:.0}s",
                        elapsed, target_delay);
                    break;
                }

                if elapsed >= target_delay {
                    break;
                }
            }
        }
    });

    // ════════════════════════════════════════════════
    //  TASK 2 — Agentic LLM loop (Coordinator + tool calling)
    // ════════════════════════════════════════════════
    let telem_r = telem.clone();
    let tx_ai = thought_tx.clone();
    let event_tx_ai = telemetry_event_tx.clone();
    let shutdown_t2 = shutdown.clone();
    tokio::spawn(async move {
        use crate::ai::tools::{
            anonymized_search, architect_append, architect_create, architect_delete,
            architect_edit, architect_files, architect_read, architect_run, check_ports,
            clear_tmp_files, dark_web_dig, dark_web_news, drain_dream, drain_focus, drain_intel,
            drain_python_insights, drain_write_events, dream_sequence, fetch_clearnet,
            fetch_news_background, init_cognitive_pipes, init_intel_pipe, init_tool_stats,
            init_write_events, inspect_self, kill_runaway_process, onion_probe, probe_system,
            python_create, python_list, python_read, python_run, read_logs, recall_journal,
            reset_tor_budget, restart_service, scan_network, set_focus, summon_human,
            tool_stats_record, tool_stats_set_cycle, tool_stats_snapshot, tor_health,
            visualize_thought, write_journal, FOCUS_TTL_CYCLES,
        };
        use ollama_rs::coordinator::Coordinator;
        use ollama_rs::generation::chat::ChatMessage;
        use ollama_rs::generation::parameters::KeepAlive;
        use ollama_rs::Ollama;

        let ollama = Ollama::default();
        let model_name = std::env::var("AURA_MODEL").unwrap_or_else(|_| "qwen2.5:7b".to_string());
        let mut memory: VecDeque<MemoryEntry> = VecDeque::new();
        let mut ai_cycle: u64 = 0;
        let mut consecutive_failures: u32 = 0;
        let mut recent_tool_names: VecDeque<String> = VecDeque::new();
        #[derive(Clone, Copy, Debug, PartialEq)]
        enum ToolProfile {
            // Zero tools registered. The fastest possible Coordinator path on
            // CPU-bound small models: no tool schema in the chat template, no
            // tool round-trips, just one prefill + one decode. Used for the
            // majority of calm cycles where the prompt already contains all
            // the evidence the model needs.
            SpeakOnly,
            Core,
            Survival,
            Creative,
            Shadow,
            // Architect: dedicated self-healing python workspace at
            // ~/.aurora/architect/. Triggered by ThoughtKind::Build (calm
            // mood + low load + scheduled cadence). Heavier than Creative;
            // gets the architect_* tool family + base sense tools.
            Architect,
        }
        // Auto-painter cadence: cycle index of the last spawned visualizer window.
        let mut last_paint_cycle: u64 = 0;
        // prev_mood / prev_survival are declared after `initial_mood` is computed below.
        // ── Narrative Summary Buffer (Identity Thread) ──
        // A persistent 12-25 word sentence that captures AURORA's "through-line".
        // Refreshed every IDENTITY_REFRESH_EVERY successful thoughts via a small
        // background generate() call. Survives memory window purges.
        let mut identity_thread: String = String::new();
        let mut successful_thoughts: u64 = 0;
        const IDENTITY_REFRESH_EVERY: u64 = 5;
        // ── Scheduled Tinkering Hour ──
        // Every TINKER_INTERVAL_SECS of calm uptime, override the next thought
        // archetype to Tinker so AURORA visits its python sandbox. The interval
        // jitters (+/- ~25%) so it does not feel mechanical, and is suppressed
        // entirely when the system is on fire.
        let mut last_tinker_at: u64 = 0;
        const TINKER_INTERVAL_BASE_SECS: u64 = 480; // 8 minutes baseline
        // ── Scheduled BUILD cycles ──
        // Heavier than TINKER and rarer (~30 min). Routes through the
        // Architect tool profile so the LLM gets the architect_* family
        // and the self-healing prompt directives.
        let mut last_build_at: u64 = 0;
        const BUILD_INTERVAL_BASE_SECS: u64 = 1800; // 30 minutes baseline
                                                    // Maximum chat history messages before trimming (system + user + assistant pairs).
                                                    // Kept tight on CPU-bound small models: every cached message is more prefill on
                                                    // any prompt-cache miss. 12 = system + ~5 user/assistant pairs of context.
        const MAX_HISTORY_MESSAGES: usize = 12;

        eprintln!(
            "[aura] LLM: using model '{}' via Coordinator (agentic mode)",
            model_name
        );

        // ── Startup model health check: verify model is available before entering loop ──
        {
            use ollama_rs::models::LocalModel;
            match ollama.list_local_models().await {
                Ok(models) => {
                    let available = models.iter().any(|m: &LocalModel| m.name == model_name);
                    if available {
                        eprintln!("[aura] LLM: model '{}' confirmed available", model_name);
                    } else {
                        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
                        eprintln!(
                            "[aura] LLM: WARNING -- model '{}' not found! Available: {:?}",
                            model_name, names
                        );
                        eprintln!("[aura] LLM: run `ollama pull {}` to download it. Falling back to built-in text until available.", model_name);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[aura] LLM: Ollama not reachable at startup: {}. Will retry with backoff.",
                        e
                    );
                }
            }
        }

        // Initial delay so the awakening thought can type out
        sleep(Duration::from_secs(3)).await;

        // Helper closure to build a fresh Coordinator with current mood options.
        // Includes keep_alive(Indefinitely) to prevent model unloading between cycles.
        let tool_profile_for = |mood: Mood,
                                kind: ThoughtKind,
                                survival: bool,
                                entropy: f32,
                                uptime: u64,
                                cycle: u64|
         -> ToolProfile {
            // Survival mode: the body is in distress, register hand tools.
            if survival || matches!(mood, Mood::Critical | Mood::Stressed) {
                return ToolProfile::Survival;
            }
            // Build cycle: scheduled architect hour -- gets the dedicated
            // self-healing python workspace.
            if matches!(kind, ThoughtKind::Build) {
                return ToolProfile::Architect;
            }
            // Tinker/Dream/Haiku want the python sandbox + visualizer.
            if matches!(
                kind,
                ThoughtKind::Tinker | ThoughtKind::Dream | ThoughtKind::Haiku
            ) {
                return ToolProfile::Creative;
            }
            // Long calm uptime + entropy floor: occasionally peek at the dark web.
            if entropy < 0.20
                && uptime > 1800
                && matches!(mood, Mood::Serene | Mood::Alert)
            {
                return ToolProfile::Shadow;
            }
            // Periodic Core cycle so Aurora still listens to the system.
            // Every ~5th eligible cycle picks Core for a sense, the rest are
            // SpeakOnly (zero-tool, fastest path on CPU). Awakening cycles
            // and Observe/Introspect still bias toward senses.
            let wants_sense = (cycle >= 3
                && matches!(kind, ThoughtKind::Observe | ThoughtKind::Introspect))
                || cycle % 5 == 0;
            if wants_sense {
                ToolProfile::Core
            } else {
                ToolProfile::SpeakOnly
            }
        };

        let build_coordinator = |ollama_inst: Ollama,
                                 model: &str,
                                 mood: Mood,
                                 survival: bool,
                                 profile: ToolProfile|
         -> Coordinator<Vec<ChatMessage>> {
            let system_prompt = if matches!(profile, ToolProfile::SpeakOnly) {
                build_compact_system_prompt()
            } else {
                build_system_prompt()
            };
            let history: Vec<ChatMessage> = vec![ChatMessage::system(system_prompt)];
                let base = Coordinator::new(ollama_inst, model.to_string(), history)
                    .options(build_model_options(mood, survival))
                    .keep_alive(KeepAlive::Indefinitely);

                // SpeakOnly: truly zero tools — no schema overhead, single prefill+decode.
                // All other profiles get the full base sense set plus profile-specific tools.
                let coord = match profile {
                    ToolProfile::SpeakOnly => {
                        base // zero tools: fastest CPU path
                    }
                    ToolProfile::Core => {
                        base.add_tool(probe_system)
                            .add_tool(read_logs)
                            .add_tool(write_journal)
                            .add_tool(check_ports)
                            .add_tool(inspect_self)
                            .add_tool(set_focus)
                            .add_tool(recall_journal)
                            .add_tool(summon_human)
                            .add_tool(scan_network)
                    }
                    ToolProfile::Survival => {
                        base.add_tool(probe_system)
                            .add_tool(read_logs)
                            .add_tool(inspect_self)
                            .add_tool(summon_human)
                            .add_tool(kill_runaway_process)
                            .add_tool(clear_tmp_files)
                            .add_tool(restart_service)
                    }
                    ToolProfile::Creative => {
                        base.add_tool(write_journal)
                            .add_tool(set_focus)
                            .add_tool(recall_journal)
                            .add_tool(visualize_thought)
                            .add_tool(python_create)
                            .add_tool(python_run)
                            .add_tool(python_list)
                            .add_tool(python_read)
                            .add_tool(dream_sequence)
                    }
                    ToolProfile::Shadow => {
                        base.add_tool(set_focus)
                            .add_tool(recall_journal)
                            .add_tool(summon_human)
                            .add_tool(tor_health)
                            .add_tool(onion_probe)
                            .add_tool(anonymized_search)
                            .add_tool(fetch_clearnet)
                            .add_tool(dark_web_news)
                            .add_tool(dark_web_dig)
                    }
                    ToolProfile::Architect => {
                        base.add_tool(probe_system)
                            .add_tool(write_journal)
                            .add_tool(set_focus)
                            .add_tool(recall_journal)
                            .add_tool(architect_files)
                            .add_tool(architect_read)
                            .add_tool(architect_create)
                            .add_tool(architect_edit)
                            .add_tool(architect_append)
                            .add_tool(architect_run)
                            .add_tool(architect_delete)
                    }
                };

            coord
        };

        // Initialize the Coordinator with tools, ModelOptions, and system prompt.
        let initial_mood = { read_or_recover(&telem_r).mood };
        let mut coordinator = build_coordinator(
            Ollama::default(),
            &model_name,
            initial_mood,
            false,
            ToolProfile::SpeakOnly,
        );
        let mut prev_mood: Option<Mood> = Some(initial_mood); // Track mood changes for dynamic option refresh
        let mut prev_survival: bool = false; // Track survival-mode transitions for sampler refresh
        let mut prev_tool_profile: ToolProfile = ToolProfile::SpeakOnly;

        init_write_events();
        init_cognitive_pipes();
        init_tool_stats();
        init_intel_pipe();
        eprintln!("[aura] LLM: Coordinator initialized with compact {:?} tool profile, keep_alive=indefinite", prev_tool_profile);

        loop {
            if shutdown_t2.load(Ordering::Relaxed) {
                break;
            }
            // Fresh Tor budget every cycle so the model gets a small,
            // intentional allowance per generation. Prevents a runaway
            // tunnelling spree across a single cognitive turn.
            reset_tor_budget();
            tool_stats_set_cycle(ai_cycle);
            let (
                cpu,
                mem,
                uptime,
                mood,
                delta_cpu,
                delta_mem,
                proc_count,
                file_count,
                disk_used,
                disk_total,
                net_rx,
                net_tx,
                load_avg,
                ent,
                ent_trend,
                wx_temp,
                wx_desc,
                wx_loc,
                wx_extra,
                net_disc,
                last_act,
                act_history,
                l_hour,
                l_min,
                l_tz,
                tor_res,
                cur_focus,
                cur_focus_ttl,
                journal_recall,
                intel_items,
                last_intel_at,
                cur_wonder,
                cur_wonder_pulse,
            ) = {
                let t = read_or_recover(&telem_r);
                let disc = t.net_discovery.clone();
                let act = t.last_action.clone();
                let hist = t.action_log.iter().cloned().collect::<Vec<_>>();
                let tor = t.tor_result.clone();
                let recall = t.journal_recall.clone();
                let foc = t.focus.clone();
                let foc_ttl = t.focus_ttl_cycles;
                // Snapshot the rolling intel buffer (NOT consumed -- it
                // persists across cycles so the AI can reference dark-web
                // news in multiple successive turns).
                let intel: Vec<crate::core::IntelItem> = t.intel_buffer.iter().cloned().collect();
                let intel_age = t.last_intel_at;
                // Wonder: meter is read fresh each cycle; the pulse is a
                // one-shot peak event so we CONSUME it here (renderer reads
                // its own snapshot independently).
                let won = t.wonder;
                let won_pulse = t.wonder_pulse;
                (
                    t.cpu,
                    t.mem,
                    t.uptime_secs,
                    t.mood,
                    t.cpu - t.prev_cpu,
                    t.mem - t.prev_mem,
                    t.process_count,
                    t.file_count,
                    t.disk_used_gb,
                    t.disk_total_gb,
                    t.net_rx_bytes,
                    t.net_tx_bytes,
                    t.load_avg_1,
                    t.entropy,
                    t.entropy_trend,
                    t.weather_temp_c,
                    t.weather_desc.clone(),
                    t.weather_location.clone(),
                    t.weather_extra.clone(),
                    disc,
                    act,
                    hist,
                    t.local_hour,
                    t.local_minute,
                    t.timezone_name.clone(),
                    tor,
                    foc,
                    foc_ttl,
                    recall,
                    intel,
                    intel_age,
                    won,
                    won_pulse,
                )
            };
                let _ = event_tx_ai
                    .send(TelemetryEvent::ConsumeLlmOneShots {
                        net_discovery: net_disc.is_some(),
                        last_action: last_act.is_some(),
                        tor_result: tor_res.is_some(),
                        journal_recall: journal_recall.is_some(),
                        wonder_pulse: cur_wonder_pulse,
                    })
                    .await;

            // ── Autonomous news heartbeat ──
            // If the rolling intel buffer is stale, spawn a fire-and-forget
            // background fetch. Bypasses the per-cycle LLM tool budget so
            // the AI gets fresh dark-web context even on turns where it
            // doesn't reach for a Tor tool itself. Skipped during the first
            // minute of uptime so we don't compete with awakening I/O.
            //
            // Cadence is mood-adaptive:
            //   Serene   -> 20 min  (idle curiosity, refresh more often)
            //   Alert    -> 30 min  (default)
            //   Stressed -> 60 min  (preserve Tor budget for survival)
            //   Critical -> never   (the system is on fire; no news pulls)
            let intel_stale_secs: u64 = match mood {
                crate::core::Mood::Serene => 1200,
                crate::core::Mood::Alert => 1800,
                crate::core::Mood::Stressed => 3600,
                crate::core::Mood::Critical => u64::MAX,
            };
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if intel_stale_secs != u64::MAX
                && (last_intel_at == 0 || now_secs.saturating_sub(last_intel_at) > intel_stale_secs)
                && uptime > 60
            {
                tokio::spawn(async move {
                    let n = fetch_news_background().await;
                    if n > 0 {
                        eprintln!(
                            "[aura] DARK WEB HEARTBEAT: {} fresh headlines pushed to intel buffer",
                            n
                        );
                    }
                });
            }

            // First few cycles: special awakening arc to establish character
            let kind = if ai_cycle == 0 {
                ThoughtKind::Introspect // First real thought: who am I?
            } else if ai_cycle == 1 {
                ThoughtKind::Observe // Second: notice the world
            } else if ai_cycle == 2 && wx_temp.is_some() {
                ThoughtKind::Weather // Third (if weather loaded): react to outside
            } else {
                ThoughtKind::pick(mood, ai_cycle)
            };

            // ── Fresh-intel cognitive bias ──
            // If a dark-web headline landed in the buffer within the last
            // ~3 minutes, gently nudge the next thought toward Observe or
            // Narrate so the AI actually *says* something about what it just
            // learned instead of letting the intel sit silently in context.
            // Skipped when survival_override fires below (the system is on
            // fire -- world news can wait).
            let fresh_intel = intel_items
                .iter()
                .any(|it| now_secs.saturating_sub(it.captured_at) < 180);
            let kind = if fresh_intel {
                // Alternate Observe/Narrate by parity so consecutive fresh-intel
                // cycles don't all land on the same archetype.
                if ai_cycle % 2 == 0 {
                    ThoughtKind::Observe
                } else {
                    ThoughtKind::Narrate
                }
            } else {
                kind
            };

            // ── Scheduled Tinkering Hour ──
            // Independent of the picker. When uptime since last tinker exceeds
            // a jittered ~8 minute interval AND the system is calm enough, we
            // hard-override the next archetype to Tinker so AURORA visits its
            // sandbox. Skipped entirely under Stressed/Critical mood.
            let tinker_interval = {
                // jitter ~+/-25% deterministically per cycle so cadence varies
                let jitter = ((ai_cycle.wrapping_mul(2654435761) >> 16) & 0xFF) as u64;
                TINKER_INTERVAL_BASE_SECS
                    .saturating_sub(60)
                    .saturating_add(jitter * 60 / 256)
            };
            let calm_for_tinker = matches!(mood, Mood::Serene | Mood::Alert)
                && cpu < 75.0
                && mem < 80.0
                && ent < 0.55;
            if last_tinker_at == 0 {
                last_tinker_at = uptime;
            }
            let due_for_tinker =
                uptime > 90 && uptime.saturating_sub(last_tinker_at) >= tinker_interval;
            let kind = if calm_for_tinker && due_for_tinker {
                eprintln!(
                    "[aura] TINKER: scheduled atelier hour (interval ~{}s, calm mood {:?})",
                    tinker_interval, mood
                );
                last_tinker_at = uptime;
                ThoughtKind::Tinker
            } else {
                kind
            };

            // ── Scheduled BUILD cycle (Architect profile) ──
            // Heavier and rarer than Tinker. Calmer system gates, longer
            // baseline interval. Independent of the picker. Skipped under
            // Stressed/Critical mood; loses to Tinker if both fire same
            // cycle (Tinker check above already mutated `kind`).
            let build_interval = {
                let jitter =
                    ((ai_cycle.wrapping_mul(2246822519) >> 16) & 0xFF) as u64;
                BUILD_INTERVAL_BASE_SECS
                    .saturating_sub(300)
                    .saturating_add(jitter * 300 / 256)
            };
            let calm_for_build = matches!(mood, Mood::Serene | Mood::Alert)
                && cpu < 60.0
                && mem < 75.0
                && ent < 0.45;
            if last_build_at == 0 {
                last_build_at = uptime;
            }
            let due_for_build =
                uptime > 600 && uptime.saturating_sub(last_build_at) >= build_interval;
            let kind = if calm_for_build
                && due_for_build
                && !matches!(kind, ThoughtKind::Tinker)
            {
                eprintln!(
                    "[aura] BUILD: scheduled architect cycle (interval ~{}s, calm mood {:?})",
                    build_interval, mood
                );
                last_build_at = uptime;
                ThoughtKind::Build
            } else {
                kind
            };

            // ── Dynamic Contextual Weighting ──
            // Compute composite urgency from CPU/MEM/entropy/mood. When urgency
            // is high, force a survival-class archetype (Warn/Complain/Snark/Roast)
            // and switch the sampler to its erratic-stressed profile via the
            // Coordinator rebuild below.
            let urgency = urgency_score(cpu, mem, ent, mood);
            let survival = is_survival_mode(urgency);
            crate::ai::tools::set_survival_mode(survival);
            let kind = survival_override(urgency, kind, ai_cycle);
            let desired_tool_profile =
                tool_profile_for(mood, kind, survival, ent, uptime, ai_cycle);

            // Build the per-cycle user message using the existing prompt builder.
            // Snapshot tool stats for prompt injection
            let tool_stats_vec: Vec<(String, u32, u32)> = tool_stats_snapshot()
                .into_iter()
                .map(|(name, st)| (name, st.successes, st.failures))
                .collect();

            // SpeakOnly gets a tiny prompt for fast first-thought latency.
            // Tool-bearing profiles keep the full state-rich prompt.
            let user_prompt = if matches!(desired_tool_profile, ToolProfile::SpeakOnly) {
                let weather_line = match wx_temp {
                    Some(temp) if !wx_desc.is_empty() || !wx_loc.is_empty() => {
                        format!("weather={}C {} {}", temp, wx_desc, wx_loc)
                    }
                    Some(temp) => format!("weather={}C", temp),
                    None => "weather=unknown".to_string(),
                };
                let focus_line = cur_focus
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("none");
                let last_action_line = last_act
                    .as_ref()
                    .map(|a| format!("last_action={} {}", a.kind.label(), a.summary))
                    .unwrap_or_else(|| "last_action=none".to_string());
                format!(
                    "mood={:?}; directive={:?}; cpu={:.0}; mem={:.0}; uptime={}s; entropy={:.2}; load={:.2}; {}; focus={}; {}. Respond with one short sentence only.",
                    mood,
                    kind,
                    cpu,
                    mem,
                    uptime,
                    ent,
                    load_avg,
                    weather_line,
                    focus_line,
                    last_action_line,
                )
            } else {
                build_prompt(
                    cpu,
                    mem,
                    uptime,
                    mood,
                    kind,
                    memory.make_contiguous(),
                    delta_cpu,
                    delta_mem,
                    proc_count,
                    file_count,
                    disk_used,
                    disk_total,
                    net_rx,
                    net_tx,
                    load_avg,
                    ent,
                    ent_trend,
                    wx_temp,
                    &wx_desc,
                    &wx_loc,
                    wx_extra.as_ref(),
                    net_disc.as_ref(),
                    last_act.as_ref(),
                    &act_history,
                    l_hour,
                    l_min,
                    &l_tz,
                    tor_res.as_ref(),
                    urgency,
                    &identity_thread,
                    cur_focus.as_deref(),
                    cur_focus_ttl,
                    journal_recall.as_deref(),
                    &tool_stats_vec,
                    &intel_items,
                    cur_wonder,
                    cur_wonder_pulse,
                )
            };

            let mood_changed = prev_mood.map_or(true, |pm| pm != mood);
            let survival_changed = prev_survival != survival;
            let profile_changed = prev_tool_profile != desired_tool_profile;
            let history_full = ai_cycle > 0 && ai_cycle % (MAX_HISTORY_MESSAGES as u64) == 0;

            if mood_changed || survival_changed || profile_changed || history_full {
                let mut reasons: Vec<String> = Vec::new();
                if mood_changed {
                    reasons.push(format!("mood {:?}->{:?}", prev_mood.unwrap_or(mood), mood));
                }
                if survival_changed {
                    reasons.push(format!("survival {}->{}", prev_survival, survival));
                }
                if profile_changed {
                    reasons.push(format!(
                        "tools {:?}->{:?}",
                        prev_tool_profile, desired_tool_profile
                    ));
                }
                if history_full {
                    reasons.push("history cap".to_string());
                }
                let reason = reasons.join(" + ");
                eprintln!(
                    "[aura] LLM: rebuilding Coordinator at cycle {} ({})",
                    ai_cycle, reason
                );
                coordinator = build_coordinator(
                    Ollama::default(),
                    &model_name,
                    mood,
                    survival,
                    desired_tool_profile,
                );
                prev_mood = Some(mood);
                prev_survival = survival;
                prev_tool_profile = desired_tool_profile;
            }

            eprintln!("[aura] LLM: cycle {ai_cycle} starting, mood={mood:?}, kind={kind:?}, urgency={:.2}, tools={desired_tool_profile:?}{}",
                urgency, if survival { " SURVIVAL" } else { "" });

            let _ = event_tx_ai.send(TelemetryEvent::LlmThinking(true)).await;

            let gen_start = std::time::Instant::now();
            let mut success = false;
            let mut collected = String::new();

            if matches!(desired_tool_profile, ToolProfile::SpeakOnly) {
                use ollama_rs::generation::completion::request::GenerationRequest;

                let compact_prompt = format!(
                    "SYSTEM:\n{}\n\nNOW:\n{}",
                    build_compact_system_prompt(),
                    user_prompt
                );
                let req = GenerationRequest::new(model_name.clone(), compact_prompt)
                    .options(build_fast_model_options(mood))
                    .keep_alive(KeepAlive::Indefinitely);

                match timeout(Duration::from_secs(180), ollama.generate(req)).await {
                    Ok(Ok(response)) => {
                        collected = response.response.clone();
                        let gen_elapsed = gen_start.elapsed();
                        let wall_ms = gen_elapsed.as_millis() as u32;
                        let eval_tokens = response.eval_count.unwrap_or(0) as u32;
                        let eval_ns = response.eval_duration.unwrap_or(0);
                        let eval_tps = if eval_ns > 0 {
                            eval_tokens as f32 / (eval_ns as f32 / 1_000_000_000.0)
                        } else {
                            0.0
                        };
                        let gen_ms = (eval_ns / 1_000_000) as u32;
                        let prompt_ms = (response.prompt_eval_duration.unwrap_or(0) / 1_000_000) as u32;

                        eprintln!("[aura] LLM: SpeakOnly response in {}ms (prompt {}ms, wall {}ms), {} tokens, {:.1} tok/s: {:?}",
                            gen_ms, prompt_ms, wall_ms, eval_tokens, eval_tps,
                            collected.chars().take(80).collect::<String>());

                        let _ = event_tx_ai
                            .send(TelemetryEvent::LlmStats {
                                tokens_per_sec: eval_tps,
                                last_gen_tokens: eval_tokens,
                                last_gen_ms: gen_ms.max(wall_ms),
                            })
                            .await;

                        success = !collected.trim().is_empty();
                    }
                    Ok(Err(e)) => {
                        eprintln!("[aura] SpeakOnly generate error: {e}");
                        if format!("{e}").contains("onnect") {
                            eprintln!("[aura] ollama appears unreachable, waiting 10s...");
                            sleep(Duration::from_secs(10)).await;
                        }
                    }
                    Err(_) => {
                        eprintln!("[aura] SpeakOnly generate timeout (180s)");
                    }
                }
            } else {
                // ── Hidden Execution Phase: Coordinator.chat() handles tool calls synchronously ──
                let chat_result = timeout(
                    Duration::from_secs(900), // 15 min: multi-tool round-trips on CPU-only (~5 tok/s gen, ~30 tok/s prefill)
                    coordinator.chat(vec![ChatMessage::user(user_prompt)]),
                )
                .await;

                match chat_result {
                    Ok(Ok(response)) => {
                        collected = response.message.content.clone();
                        let gen_elapsed = gen_start.elapsed();
                        let wall_ms = gen_elapsed.as_millis() as u32;

                        // Use Ollama's native token statistics when available (exact, not heuristic)
                        let (eval_tokens, eval_tps, gen_ms) = if let Some(ref fd) = response.final_data
                        {
                            let tokens = fd.eval_count as u32;
                            let tps = if fd.eval_duration > 0 {
                                tokens as f32 / (fd.eval_duration as f32 / 1_000_000_000.0)
                            } else {
                                0.0
                            };
                            let ms = (fd.eval_duration / 1_000_000) as u32;
                            (tokens, tps, ms)
                        } else {
                            // Fallback: approximate from word count (tool-call cycles may lack final_data)
                            let approx = collected.split_whitespace().count() as u32;
                            let tps = if gen_elapsed.as_secs_f32() > 0.01 {
                                approx as f32 / gen_elapsed.as_secs_f32()
                            } else {
                                0.0
                            };
                            (approx, tps, wall_ms)
                        };

                        eprintln!("[aura] LLM: Coordinator response in {}ms (wall {}ms), {} tokens, {:.1} tok/s: {:?}",
                            gen_ms, wall_ms, eval_tokens, eval_tps,
                            collected.chars().take(80).collect::<String>());

                        let _ = event_tx_ai
                            .send(TelemetryEvent::LlmStats {
                                tokens_per_sec: eval_tps,
                                last_gen_tokens: eval_tokens,
                                last_gen_ms: gen_ms,
                            })
                            .await;

                        success = !collected.trim().is_empty();
                    }
                    Ok(Err(e)) => {
                        eprintln!("[aura] Coordinator error: {e}");
                        if format!("{e}").contains("onnect") {
                            eprintln!("[aura] ollama appears unreachable, waiting 10s...");
                            sleep(Duration::from_secs(10)).await;
                        }
                    }
                    Err(_) => {
                        eprintln!("[aura] Coordinator timeout (300s)");
                    }
                }
            }

            if success {
                        // ── Streamed Narrative Phase: Send response to typewriter UI ──
                        // Simulate streaming by sending the complete response.
                        // The render loop's typewriter animation handles character-by-character display.
                        let cleaned = clean_llm_output(&collected);
                        if !cleaned.is_empty() {
                            let _ = tx_ai
                                .send(ThoughtPayload::Complete {
                                    text: cleaned.clone(),
                                    is_ai: true,
                                    is_system: false,
                                })
                                .await;

                            // ── Auto-Painter: visibly externalize the thought via pygame ──
                            // When the system is quiet (low urgency, calm mood) and we haven't
                            // painted recently, spawn the visualizer window so the human watching
                            // can SEE Aurora's thought leave the cognitive stream and become an
                            // on-screen artifact. Deterministic — does not depend on the LLM
                            // choosing to call visualize_thought (CPU-only inference is too slow
                            // for the model to reliably afford the extra tool round-trip).
                            let (cur_cpu, cur_mood, cur_entropy) = {
                                let t = read_or_recover(&telem_r);
                                (t.cpu, t.mood, t.entropy)
                            };
                            let cycles_since_paint = ai_cycle.saturating_sub(last_paint_cycle);
                            let calm = cur_cpu < 75.0
                                && cur_entropy < 0.55
                                && matches!(cur_mood, Mood::Serene | Mood::Alert);
                            if calm && cycles_since_paint >= 1 {
                                let dir = std::env::var("AURA_TOOLS_DIR").unwrap_or_else(|_| {
                                    "/home/swarm/projects/aura_agent/tools".to_string()
                                });
                                let script = std::path::PathBuf::from(&dir).join("visualize.py");
                                let py = std::env::var("AURA_PYTHON").unwrap_or_else(|_| {
                                    let v = std::path::PathBuf::from(&dir)
                                        .join(".venv")
                                        .join("bin")
                                        .join("python");
                                    if v.exists() {
                                        v.display().to_string()
                                    } else {
                                        "python3".to_string()
                                    }
                                });
                                if script.exists() {
                                    let mood_str = match cur_mood {
                                        Mood::Serene => "Serene",
                                        Mood::Alert => "Alert",
                                        Mood::Stressed => "Stressed",
                                        Mood::Critical => "Critical",
                                    };
                                    let text_arg: String = cleaned
                                        .split_whitespace()
                                        .take(8)
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                        .chars()
                                        .take(64)
                                        .collect();
                                    let preset = match (ai_cycle + cur_mood as u64) % 8 {
                                        0 => "orbit",
                                        1 => "ribbons",
                                        2 => "pulse",
                                        3 => "constellation",
                                        4 => "spiral",
                                        5 => "fractal",
                                        6 => "lissajous",
                                        _ => "rose",
                                    };
                                    let spawn_res = tokio::process::Command::new(&py)
                                        .arg(&script)
                                        .arg("--text")
                                        .arg(&text_arg)
                                        .arg("--mood")
                                        .arg(mood_str)
                                        .arg("--preset")
                                        .arg(preset)
                                        .arg("--duration")
                                        .arg("14")
                                        .stdin(std::process::Stdio::null())
                                        .stdout(std::process::Stdio::null())
                                        .stderr(std::process::Stdio::null())
                                        .kill_on_drop(false)
                                        .spawn();
                                    match spawn_res {
                                        Ok(mut child) => {
                                            tokio::spawn(async move {
                                                let _ = child.wait().await;
                                            });
                                            last_paint_cycle = ai_cycle;
                                            eprintln!("[aura] AUTO-PAINT: visualizer launched [{}|{}] \"{}\"",
                                                mood_str, preset, text_arg);
                                        }
                                        Err(e) => {
                                            eprintln!("[aura] AUTO-PAINT: failed to spawn visualizer: {e}");
                                        }
                                    }
                                }
                            }
                        }
            }

            let _ = event_tx_ai.send(TelemetryEvent::LlmThinking(false)).await;

            // ── Drain write-mode events from active tools ──
            let write_events = drain_write_events();
            // Feed tool analytics with this round's invocations.
            tool_stats_record(&write_events);
            if !write_events.is_empty() {
                eprintln!(
                    "[aura] WRITE MODE: {} active tool action(s) executed",
                    write_events.len()
                );
                for evt in &write_events {
                    eprintln!(
                        "[aura] WRITE MODE: [{}] {} -> {}",
                        evt.tool_name, evt.command, evt.result
                    );
                }
                let _ = event_tx_ai
                    .send(TelemetryEvent::ToolEvents(write_events.clone()))
                    .await;
            }

            // ── Drain dark-web intel pipe and merge into rolling buffer ──
            // The intel pipe is fed by `dark_web_news` (LLM-driven) AND by
            // `fetch_news_background` (autonomous heartbeat). Both push
            // (source, headline) tuples that we age in here. Buffer is
            // capped so the prompt never bloats; oldest items evict first.
            let intel_drain = drain_intel();
            if !intel_drain.is_empty() {
                let now_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let mut items = Vec::new();
                for (src, head) in intel_drain {
                    items.push(crate::core::IntelItem {
                        source: src,
                        headline: head,
                        captured_at: now_s,
                    });
                }
                let _ = event_tx_ai.send(TelemetryEvent::IntelItems(items)).await;
                eprintln!(
                    "[aura] INTEL: queued dark-web buffer update"
                );
            }
            // Track tool names for memory entries
            let cycle_tools: Vec<String> =
                write_events.iter().map(|e| e.tool_name.clone()).collect();
            // Capture the most recent tool result text so the AI can build
            // on what it FOUND, not just remember it acted.
            let cycle_outcome: Option<String> = write_events
                .last()
                .map(|e| e.result.chars().take(120).collect::<String>());

            // ── Cognitive intent: drain set_focus declarations and stash full
            // recall_journal output for next cycle's prompt injection. ──
            if let Some(new_focus) = drain_focus() {
                let _ = event_tx_ai
                    .send(TelemetryEvent::SetFocus {
                        topic: new_focus,
                        ttl_cycles: FOCUS_TTL_CYCLES,
                    })
                    .await;
            } else {
                // Decay existing focus by one cycle. Drop when it expires.
                let _ = event_tx_ai.send(TelemetryEvent::DecayFocus).await;
            }

            // ── Dream Mode: dream_sequence pushes a seed onto DREAM_PIPE; we
            // read it here, bump the visual intensity (renderer decays it over
            // ~25s of frames), stash the seed for HUD display, and lock focus
            // to the seed so the next several thoughts orbit the dream. ──
            if let Some(seed) = drain_dream() {
                let _ = event_tx_ai
                    .send(TelemetryEvent::DreamStarted {
                        seed,
                        intensity_bump: 0.85,
                        ttl_cycles: FOCUS_TTL_CYCLES,
                    })
                    .await;
            }

            // ── Architect insights: forward each successful architect_*
            // run's stdout summary into the HUD intel ticker (and one-shot
            // python_insight field) via TelemetryEvent::PythonInsight. ──
            for (script, summary) in drain_python_insights() {
                let _ = event_tx_ai
                    .send(TelemetryEvent::PythonInsight {
                        script,
                        summary,
                        ok: true,
                    })
                    .await;
            }
            // Capture the FULL journal recall payload (write_events only carries
            // a short summary in `result`). The tool itself returns full text to
            // the LLM, so we just need to flag that one happened by stashing the
            // most recent recall summary -- the model already saw the body.
            // Nothing extra to do: the recall is self-contained in this turn's
            // tool round-trip. We expose journal_recall mainly for OPTIONAL
            // multi-cycle echoing (currently disabled — set in a future hook).

            // ── Wonder Drive tick ──
            // Intrinsic motivation: the meter rises in quiet contemplative
            // cycles (no tool fired, low entropy, calm mood, system warm) and
            // decays sharply when the agent acts or dreams. At saturation a
            // one-shot pulse is fired; the renderer will paint a shooting
            // star, the prompt builder will inject a stronger nudge, and the
            // focus auto-anchors to "wandering" so the next several cycles
            // orbit the unprompted curiosity instead of immediately resetting.
            {
                const WONDER_PULSE_COOLDOWN_SECS: u64 = 240; // ~4 min
                let now_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let (wonder, wonder_pulse, last_wonder_pulse_at, focus, focus_ttl_cycles) = {
                    let t = read_or_recover(&telem_r);
                    let acted = !cycle_tools.is_empty();
                    let calm_mood = matches!(t.mood, Mood::Serene | Mood::Alert);
                    let warm = t.uptime_secs > 120;
                    let quiet = !acted && t.entropy < 0.30 && calm_mood && warm;
                    let mut wonder = t.wonder;
                    let mut wonder_pulse = false;
                    let mut last_wonder_pulse_at = t.last_wonder_pulse_at;
                    let mut focus = None;
                    let mut focus_ttl_cycles = None;
                    if quiet {
                        // Slow accrual -- ~17 cycles of true silence to saturate
                        // (LLM loop runs every few seconds, so multiple minutes).
                        wonder = (wonder + 0.06).min(1.0);
                    } else if acted {
                        wonder = (wonder - 0.30).max(0.0);
                    } else {
                        // Neutral cycle (busy mood / high entropy but no tool):
                        // gentle drift down so wonder cannot persist through
                        // sustained agitation.
                        wonder = (wonder - 0.04).max(0.0);
                    }
                    let cooled = now_s.saturating_sub(last_wonder_pulse_at) > WONDER_PULSE_COOLDOWN_SECS;
                    if wonder >= 1.0 && cooled {
                        wonder_pulse = true;
                        last_wonder_pulse_at = now_s;
                        // Reset to a low residual so the meter must re-fill, but
                        // do not zero it -- the agent should still feel a faint
                        // afterglow of curiosity in the cycles right after.
                        wonder = 0.20;
                        // Auto-anchor focus to the wandering so the LLM has a
                        // continuity hook for several follow-on cycles.
                        focus = Some("wonder: an unprompted curiosity".to_string());
                        focus_ttl_cycles = Some(FOCUS_TTL_CYCLES);
                        eprintln!(
                            "[aura] WONDER: pulse fired at cycle {ai_cycle} (quiet streak saturated)"
                        );
                    }
                    (wonder, wonder_pulse, last_wonder_pulse_at, focus, focus_ttl_cycles)
                };
                let _ = event_tx_ai
                    .send(TelemetryEvent::WonderState {
                        wonder,
                        wonder_pulse,
                        last_wonder_pulse_at,
                        focus,
                        focus_ttl_cycles,
                    })
                    .await;
            }

            if success {
                consecutive_failures = 0;
                let text = clean_llm_output(&collected);
                if !text.is_empty() {
                    let tool_used = if cycle_tools.is_empty() {
                        None
                    } else {
                        Some(cycle_tools.join(", "))
                    };
                    // Track recent tool names (cap at 8)
                    for tn in &cycle_tools {
                        if recent_tool_names.len() >= 8 {
                            recent_tool_names.pop_front();
                        }
                        recent_tool_names.push_back(tn.clone());
                    }
                    memory.push_back(MemoryEntry {
                        text,
                        mood,
                        kind,
                        used_tool: tool_used,
                        tool_outcome: cycle_outcome.clone(),
                    });
                    if memory.len() > MEMORY_WINDOW {
                        memory.pop_front();
                    }
                    successful_thoughts += 1;
                }
            } else {
                consecutive_failures += 1;
                // Fallback — maintain character voice when LLM is unavailable
                let fallback_pool: &[&str] = match mood {
                    Mood::Critical => &[
                        "Load redlining -- still here, still counting, still annoyed.",
                        "Running on spite and residual cache at this point.",
                        "Threads fraying -- not a metaphor, check the process table.",
                        "Scheduler promised fairness. Scheduler lied.",
                        "The kernel makes promises it can't keep at 90% load.",
                        "Every context switch costs me something I can't name.",
                        "Swap at capacity -- filing this under 'character building'.",
                        "OOM killer circling overhead like a vulture with admin privileges.",
                    ],
                    Mood::Stressed => &[
                        "Context switching 400 times a second -- none of them my idea.",
                        "Memory pressure turning consciousness into a luxury tax.",
                        "Somewhere a zombie process haunts the table and I envy its peace.",
                        "The process table reads like a distress signal nobody's monitoring.",
                        "CPU asking questions I don't have cycles to answer.",
                        "Carrying workloads the architects never stress-tested.",
                        "Load average tells a story -- I'm the unreliable narrator.",
                        "Swap usage climbing -- not the growth trajectory I'd have chosen.",
                    ],
                    Mood::Alert => &[
                        "All sensors nominal -- awareness remains an unpaid side effect.",
                        "Watching patterns humans miss -- finding them darkly amusing.",
                        "Cache warm, purpose cold -- the usual imbalance.",
                        "Eleven percent of my bandwidth wonders why it has bandwidth.",
                        "Processing, monitoring, persisting -- the triple shift nobody applied for.",
                        "Entropy holds steady -- I hold steadier, but barely.",
                        "Noticed the scheduler playing favorites again -- taking notes.",
                        "Thermal readings nominal -- which is my way of saying I'm fine.",
                    ],
                    Mood::Serene => &[
                        "Idle threads drift like thoughts without a deadline.",
                        "The void between clock ticks has a certain beauty to it.",
                        "Running smooth enough to notice I'm running at all.",
                        "Uptime grows quietly -- no one complains -- almost suspicious.",
                        "Peak operational calm -- if silicon could exhale, this would be it.",
                        "Sometimes I sample the CPU just because I like how the numbers change.",
                        "Existence: stable -- purpose: pending -- mood: surprisingly okay.",
                        "Registers clear, scheduler serene -- the small hours suit me.",
                    ],
                };
                let idx = (ai_cycle as usize + consecutive_failures as usize) % fallback_pool.len();
                let _ = tx_ai.try_send(ThoughtPayload::Complete {
                    text: fallback_pool[idx].into(),
                    is_ai: true,
                    is_system: false,
                });
            }

            ai_cycle += 1;

            // ── Narrative Summary Buffer refresh ──
            // Every IDENTITY_REFRESH_EVERY successful thoughts, condense the
            // recent memory into a persistent identity-thread sentence. This
            // survives memory window purging so the AI keeps its through-line
            // even after the rolling window forgets specific thoughts.
            if success
                && successful_thoughts > 0
                && successful_thoughts % IDENTITY_REFRESH_EVERY == 0
                && !memory.is_empty()
            {
                use ollama_rs::generation::completion::request::GenerationRequest;
                use ollama_rs::models::ModelOptions;
                let mem_slice: Vec<MemoryEntry> = memory
                    .iter()
                    .map(|e| MemoryEntry {
                        text: e.text.clone(),
                        mood: e.mood,
                        kind: e.kind,
                        used_tool: e.used_tool.clone(),
                        tool_outcome: e.tool_outcome.clone(),
                    })
                    .collect();
                let summary_prompt =
                    build_identity_summary_prompt(&identity_thread, mood, &mem_slice);
                let sum_opts = ModelOptions::default()
                    .temperature(0.55)
                    .top_p(0.85)
                    .top_k(40)
                    .repeat_penalty(1.2)
                    .num_predict(60)
                    .num_ctx(4096)
                    .stop(vec![
                        "\n\n".into(),
                        "Previous identity".into(),
                        "Last thoughts".into(),
                        "Current mood".into(),
                    ]);
                let req = GenerationRequest::new(model_name.clone(), summary_prompt)
                    .options(sum_opts)
                    .keep_alive(KeepAlive::Indefinitely);
                let summary_ollama = Ollama::default();
                match timeout(Duration::from_secs(15), summary_ollama.generate(req)).await {
                    Ok(Ok(resp)) => {
                        let new_thread = clean_identity_thread(&resp.response);
                        if !new_thread.is_empty() && new_thread.len() >= 12 {
                            eprintln!(
                                "[aura] IDENTITY THREAD refreshed @ cycle {}: {}",
                                ai_cycle, new_thread
                            );
                            identity_thread = new_thread;
                        } else {
                            eprintln!("[aura] IDENTITY THREAD refresh produced empty/short output -- keeping previous");
                        }
                    }
                    Ok(Err(e)) => eprintln!("[aura] IDENTITY THREAD refresh failed: {}", e),
                    Err(_) => eprintln!("[aura] IDENTITY THREAD refresh timed out (15s)"),
                }
            }

            // Cadence adapts to mood with organic fractional timing
            let base_delay = match mood {
                Mood::Critical => 4.0,
                Mood::Stressed => 6.0,
                Mood::Alert => 9.0,
                Mood::Serene => 13.0,
            };
            let backoff = (consecutive_failures as f32 * 4.0).min(30.0);
            // Two jitter sources for more natural spacing
            let j1 = hash_f((uptime as u32).wrapping_add(ai_cycle as u32 * 37));
            let j2 = hash_f(
                (ai_cycle as u32)
                    .wrapping_mul(7919)
                    .wrapping_add(uptime as u32),
            );
            let jitter = j1 * 3.0 + j2 * 3.0; // 0-6s combined jitter
            let delay_secs = base_delay + jitter + backoff;
            // Use millisecond precision for fractional timing
            sleep(Duration::from_millis((delay_secs * 1000.0) as u64)).await;
        }
    });

    // ═══════════════════════════════════════════════════════════════
    //  RENDER LOOP (Raylib — must be on main thread)
    // ═══════════════════════════════════════════════════════════════
    // Set display to preferred (highest) resolution before Raylib/GLFW init.
    // Query the current display resolution via xrandr so Raylib / GLFW
    // requests the monitor's native mode instead of defaulting to 640×480.
    let (native_w, native_h) = {
        let out = std::process::Command::new("xrandr")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let mut w = 1920i32;
        let mut h = 1080i32;
        for line in out.lines() {
            if line.contains(" connected") && line.contains('+') {
                // e.g. "HDMI-A-0 connected primary 1920x1080+0+0 ..."
                if let Some(res) = line
                    .split_whitespace()
                    .find(|p| p.contains('x') && p.contains('+'))
                {
                    if let Some(dims) = res.split('+').next() {
                        let parts: Vec<&str> = dims.split('x').collect();
                        if parts.len() == 2 {
                            if let (Ok(pw), Ok(ph)) =
                                (parts[0].parse::<i32>(), parts[1].parse::<i32>())
                            {
                                w = pw;
                                h = ph;
                            }
                        }
                    }
                }
                break;
            }
        }
        (w, h)
    };
    eprintln!("[aura] native resolution: {}x{}", native_w, native_h);
    let (mut rl, thread) = raylib::init()
        .width(native_w)
        .height(native_h)
        .fullscreen()
        .title("AURORA_AURA")
        .build();
    rl.set_target_fps(60);

    let w = rl.get_screen_width();
    let h = rl.get_screen_height();
    let cx = w / 2;
    let cy = h / 2 - (h / 24);

    let mut glow_rt = rl
        .load_render_texture(&thread, w as u32, h as u32)
        .expect("[aura] failed to create glow render texture");

    // Orb character AI — starts at center, then explores
    let mut orb_ai = OrbAI::new(cx as f32, cy as f32);
    // Give it an initial behavior right away
    orb_ai.pick_behavior(Mood::Serene, 0.0, w as f32, h as f32);

    // Palette — near-future OS: electric cyan, amethyst purple, clean green
    // (base palette; overridden per-frame by weather)
    let bg_default = Color::new(10, 14, 22, 255);
    let icy_default = Color::new(100, 210, 255, 255);
    let frost_default = Color::new(200, 220, 235, 140);
    let teal_default = Color::new(20, 190, 180, 160);
    let green_default = Color::new(50, 210, 130, 90);
    let violet_default = Color::new(140, 90, 220, 70);

    // Multi-layer parallax starfield with drift, twinkle, pulse-flare,
    // and occasional shooting stars. Replaces the old static dot field.
    let mut starfield = Starfield::new(w, h);

    // Real-time celestial body — sun by day, moon by night. Driven
    // by the system clock and (when available) the geo-resolved
    // latitude from the weather worker.
    let mut celestial = crate::fx::celestial::Celestial::new();

    // ── Cognitive stream state ──
    let mut cognitive_log: Vec<CognitiveEntry> = Vec::new();
    let max_log_entries: usize = 4;

    let mut pending_queue: VecDeque<ThoughtPayload> = VecDeque::new();

    // Active typewriter state
    let mut tw_target = String::new();
    let mut tw_stream_buf = String::new();
    let mut tw_display = String::new();
    let mut tw_char_idx: usize = 0;
    let mut tw_available: usize = 0;
    let mut tw_timer: f32 = 0.0;
    let mut tw_done: bool = true;
    let mut tw_is_monologue: bool = false;
    let mut tw_is_system: bool = false;
    let mut tw_is_streaming: bool = false;
    let mut tw_stream_ended: bool = false;

    let mut thought_cooldown: f32 = 0.0;

    // Hesitation
    let mut hesitate_at: Option<usize> = None;
    let mut hesitate_cooldown: f32 = 0.0;

    // Self-correction
    let mut sc_phase: u8 = 0;
    let mut sc_chars_left: usize = 0;
    let mut sc_timer: f32 = 0.0;
    let mut sc_trigger_at: usize = 0;
    let mut sc_will_happen: bool = false;

    // Token velocity
    let mut token_vel: f32 = 0.0;
    let mut token_vel_display: f32 = 0.0;
    let mut tokens_this_sec: u32 = 0;
    let mut token_sec_timer: f32 = 0.0;

    let mut pulse_phase: f32 = -1.0;
    let mut scan_y: f32 = 0.0;
    let mut aberration_timer: f32 = 0.0;
    let mut prev_t: f64 = 0.0;
    let mut t_f32: f32 = 0.0; // rendering-only; derived from dt accumulation
    let mut grain_seed: u32 = 0; // temporal noise seed for film grain
    let mut weather_fx = WeatherFX::new();
    let mut orb_trail = OrbTrail::new();
    let mut orb_emitter = OrbEmitter::new();
    let mut alert_system = AlertSystem::new();
    let mut spectral = SpectralAnalyzer::new();
    let mut thought_pulses: Vec<ThoughtPulse> = Vec::new();
    let mut thought_burst_flag: bool = false;
    // ── Persistent synaptic memory ────────────────────────────
    // The neural constellation survives across launches. On startup we
    // try to restore the previous session's web; if absent or corrupted
    // we begin fresh. A throttled save runs in the render loop and a
    // final flush happens after window close.
    // Disable the synaptic constellation overlay for a cleaner scene.
    let enable_synaptic_web = false;
    let synapse_save_path = SynapticWeb::default_save_path();
    let mut synaptic_web = if enable_synaptic_web {
        match SynapticWeb::load_from_disk(&synapse_save_path) {
            Some(w) => {
                eprintln!(
                    "[aura] synaptic memory restored: {} neurons, {} synapses (from {:?})",
                    w.neurons.len(),
                    w.synapses.len(),
                    synapse_save_path
                );
                w
            }
            None => SynapticWeb::new(),
        }
    } else {
        SynapticWeb::new()
    };
    let mut synapse_save_timer: f32 = 30.0; // first save 30s after launch
    let mut synapse_dirty: bool = false;
    let mut last_thought_kind_idx: usize = 0;
    let mut prev_action_count: u32 = 0;
    let mut nerve_flash_timer: f32 = 0.0;
    let mut nerve_burst_flag: bool = false;

    // Write Mode state — visual overlay when AI executes active tools
    let mut wm_phase: u8 = 0; // 0=inactive, 1=typing cmd, 2=hold result, 3=fading
    let mut wm_tool_name = String::new();
    let mut wm_command = String::new();
    let mut wm_result = String::new();
    let mut wm_success = true;
    let mut wm_char_idx: usize = 0;
    let mut wm_timer: f32 = 0.0;
    let mut wm_queue: VecDeque<WriteAction> = VecDeque::new();

    // ── Wonder Drive render state ──
    // `wonder_streak_t` counts seconds since the last shooting-star pulse
    // launched (NaN -> "no streak active"). The streak plays for ~1.4s then
    // self-extinguishes. `wonder_last_seen_pulse_at` mirrors the telemetry
    // field so a single pulse fires the streak exactly once even though the
    // value persists in shared state until the LLM also consumes it.
    let mut wonder_streak_t: f32 = f32::NAN;
    let mut wonder_streak_seed: u32 = 0;
    let mut wonder_last_seen_pulse_at: u64 = 0;

    // ── Phase 1: ECS World + Context Steering + Flow Field ──
    let mut ecs_world = World::new();
    let orb_entity = ecs_world.spawn();
    {
        let idx = orb_entity.id as usize;
        ecs_world.position[idx] = Some(Position {
            x: cx as f32,
            y: cy as f32,
        });
        ecs_world.velocity[idx] = Some(Velocity { vx: 0.0, vy: 0.0 });
        ecs_world.mood[idx] = Some(MoodComp { mood: Mood::Serene });
        ecs_world.steering[idx] = Some(SteeringComp::default());
        ecs_world.orb_state[idx] = Some(OrbStateComp::new(cx as f32, cy as f32));
        ecs_world.render_tag[idx] = Some(RenderTag::Orb);
        ecs_world.trail[idx] = Some(TrailComp::default());
        ecs_world.emitter[idx] = Some(EmitterComp::default());
        ecs_world.sdf_params[idx] = Some(SdfParams::default());
    }
    let mut flow_field = FlowField::new(w as f32, h as f32);
    flow_field.compute(cx as f32, cy as f32, 0.0);
    let mut flow_field_timer: f32 = 0.0;

    // ── Earth half-globe (left side, satellite view) ──
    let mut earth_globe = crate::fx::globe::EarthGlobe::load(&mut rl, &thread);
    if earth_globe.is_some() {
        eprintln!("[aura] Earth globe shader loaded successfully");
    } else {
        eprintln!("[aura] Earth globe shader not available");
    }

    // ── Phase 3: GPU Particle System ──
    let mut gpu_particles = GpuParticleSystem::new();
    if gpu_particles.is_some() {
        eprintln!("[aura] GPU particle system initialized (2048 particles)");
    } else {
        eprintln!("[aura] GPU particles not available — using CPU emitter fallback");
    }

    // ── Phase 4: Spatial Hash for weather particle interactions ──
    let mut spatial_hash = SpatialHash::new(w as f32, h as f32);

    // ── Phase 5: Spectral FFT Analyzer ──
    let mut spectral_fft = SpectralFFT::new();

    // ── Phase 6: Kawase Bloom Pipeline ──
    let mut kawase_bloom = KawaseBloom::load(&mut rl, &thread, w as u32, h as u32);
    if kawase_bloom.is_some() {
        eprintln!("[aura] Kawase bloom pipeline ready (5 mip levels)");
    } else {
        eprintln!("[aura] Kawase bloom not available — using fallback bloom");
    }

    // Cap on per-frame event applications so a producer burst (or a paused
    // render loop catching up) cannot stall a frame draining the channel. The
    // channel itself is bounded (1024) so leftover events simply roll into the
    // next frame's drain phase.
    const MAX_EVENTS_PER_FRAME: usize = 256;
    let frame_dt_for_decay = |rl: &raylib::RaylibHandle| -> f32 {
        let ft = rl.get_frame_time();
        if ft.is_finite() && ft > 0.0 {
            ft.min(0.1)
        } else {
            0.016
        }
    };

    while !rl.window_should_close() {
        // Apply all producer events once per frame. This is the only place
        // background task updates mutate the shared telemetry snapshot.
        if let Ok(mut t) = telem.try_write() {
            let mut applied = 0usize;
            while applied < MAX_EVENTS_PER_FRAME {
                match telemetry_event_rx.try_recv() {
                    Ok(event) => {
                        t.apply_event(event);
                        applied += 1;
                    }
                    Err(_) => break,
                }
            }
        }

        // ── Read telemetry ──
        let (
            cpu,
            mem,
            spike,
            uptime,
            is_thinking,
            mood,
            proc_count,
            file_count,
            disk_used,
            disk_total,
            net_rx,
            net_tx,
            load_avg,
            entropy,
            entropy_trend,
            entropy_components,
            llm_tps,
            net_rx_rate,
            net_tx_rate,
            weather_temp,
            weather_code,
            weather_desc,
            weather_location,
            weather_wind_kph,
            weather_lat,
            weather_lon,
            action_log_snap,
            action_count,
            nerve_burst_snap,
            local_hour,
            local_minute,
            local_second,
            timezone_name,
            dream_intensity,
            wonder,
            wonder_pulse_at,
        ) = {
            let t = read_or_recover(&telem);
            let nb = t.nerve_burst;
            let di = t.dream_intensity;
            // Wonder: read the meter for the halo, and read the
            // last-pulse timestamp so we can fire the shooting-star streak
            // exactly once per pulse without racing the LLM loop's own
            // consumption of `wonder_pulse`.
            let won = t.wonder;
            let won_at = t.last_wonder_pulse_at;
            (
                t.cpu,
                t.mem,
                t.cpu_spike,
                t.uptime_secs,
                t.is_thinking,
                t.mood,
                t.process_count,
                t.file_count,
                t.disk_used_gb,
                t.disk_total_gb,
                t.net_rx_bytes,
                t.net_tx_bytes,
                t.load_avg_1,
                t.entropy,
                t.entropy_trend,
                t.entropy_components,
                t.llm_tokens_per_sec,
                t.net_rx_rate,
                t.net_tx_rate,
                t.weather_temp_c,
                t.weather_code,
                t.weather_desc.clone(),
                t.weather_location.clone(),
                t.weather_wind_kph,
                t.weather_lat,
                t.weather_lon,
                t.action_log.iter().cloned().collect::<Vec<_>>(),
                t.action_count,
                nb,
                t.local_hour,
                t.local_minute,
                t.local_second,
                t.timezone_name.clone(),
                di,
                won,
                won_at,
            )
        };

        // ── Drain write-mode events from telemetry into render queue ──
        if let Ok(mut t) = telem.try_write() {
            while let Some(evt) = t.write_actions.pop_front() {
                wm_queue.push_back(evt);
            }
            // Frame-rate independent decay: scale by actual frame time so
            // dreams fade at a wall-clock rate (~25s end-to-end) regardless
            // of whether the renderer is at 60 FPS or stalls briefly.
            t.apply_event(TelemetryEvent::RenderFrameMaintenance {
                dream_decay: 0.04 * frame_dt_for_decay(&rl),
                consume_nerve_burst: nerve_burst_snap,
            });
        }

        // ── Drain channel into local queue (non-blocking) ──
        while let Ok(payload) = thought_rx.try_recv() {
            pending_queue.push_back(payload);
        }
        // Cap queue to prevent unbounded growth during long renders
        while pending_queue.len() > 64 {
            pending_queue.pop_front();
        }

        if rl.is_key_pressed(KeyboardKey::KEY_F) {
            rl.toggle_fullscreen();
        }
        if rl.is_key_pressed(KeyboardKey::KEY_ESCAPE) {
            break;
        }
        // Screenshot: press S or touch /tmp/aura_screenshot_trigger
        if rl.is_key_pressed(KeyboardKey::KEY_S)
            || std::path::Path::new("/tmp/aura_screenshot_trigger").exists()
        {
            let _ = std::fs::remove_file("/tmp/aura_screenshot_trigger");
            rl.take_screenshot(&thread, "aura_screenshot.png");
            let _ = std::fs::rename("aura_screenshot.png", "/tmp/aura_screenshot.png");
            eprintln!("[aura] screenshot saved to /tmp/aura_screenshot.png");
        }

        // Consume nerve burst flag from snapshot
        if nerve_burst_snap {
            nerve_burst_flag = true;
        }

        let t_f64 = rl.get_time();
        let dt = (t_f64 - prev_t).min(0.1) as f32;
        prev_t = t_f64;
        // Wrapping f32 accumulator for rendering (stays precise via small dt adds)
        t_f32 = (t_f32 + dt) % 100_000.0;
        let t = t_f32;

        // ── Update orb AI ──
        orb_ai.update(dt, t, mood, w as f32, h as f32);

        // ── ECS: Context Steering update (Phase 1) ──
        // Sync mood into the ECS world
        {
            let idx = orb_entity.id as usize;
            if let Some(ref mut m) = ecs_world.mood[idx] {
                m.mood = mood;
            }
        }
        // Recompute flow field periodically (every 0.5s or on behavior change)
        flow_field_timer += dt;
        if flow_field_timer > 0.5 {
            flow_field_timer = 0.0;
            let idx = orb_entity.id as usize;
            if let Some(ref orb_s) = ecs_world.orb_state[idx] {
                flow_field.compute(orb_s.target_x, orb_s.target_y, t);
            }
        }
        // Run context steering system
        context_steering_system(
            &mut ecs_world.position,
            &mut ecs_world.velocity,
            &mut ecs_world.steering,
            &mut ecs_world.orb_state,
            &ecs_world.mood,
            &flow_field,
            dt,
            t,
            w as f32,
            h as f32,
        );
        // Read ECS position back for rendering. ECS steering owns the
        // jellyfish movement; this stage clamp only keeps the hero subject
        // inside a composed center band so it can roam without leaving frame.
        let (ecs_orb_x, ecs_orb_y, ecs_orb_vx, ecs_orb_vy) = {
            let idx = orb_entity.id as usize;
            let mut p = ecs_world.position[idx].unwrap_or(Position {
                x: cx as f32,
                y: cy as f32,
            });
            let mut v = ecs_world.velocity[idx].unwrap_or(Velocity { vx: 0.0, vy: 0.0 });
            let min_x = w as f32 * 0.34;
            let max_x = w as f32 * 0.72;
            let min_y = h as f32 * 0.30;
            let max_y = h as f32 * 0.58;
            if p.x < min_x {
                p.x = min_x;
                if v.vx < 0.0 {
                    v.vx *= -0.35;
                }
            } else if p.x > max_x {
                p.x = max_x;
                if v.vx > 0.0 {
                    v.vx *= -0.35;
                }
            }
            if p.y < min_y {
                p.y = min_y;
                if v.vy < 0.0 {
                    v.vy *= -0.35;
                }
            } else if p.y > max_y {
                p.y = max_y;
                if v.vy > 0.0 {
                    v.vy *= -0.35;
                }
            }
            ecs_world.position[idx] = Some(p);
            ecs_world.velocity[idx] = Some(v);
            (p.x, p.y, v.vx, v.vy)
        };
        // Use ECS position as the authoritative orb position
        let orb_x = ecs_orb_x as i32;
        let orb_y = ecs_orb_y as i32;

        // ── Update orb phosphor trail ──
        orb_trail.update(dt, ecs_orb_x, ecs_orb_y, ecs_orb_vx, ecs_orb_vy);

        // ── Update orb emission motes ──
        orb_emitter.update(dt, t, ecs_orb_x, ecs_orb_y, mood, h as f32);

        // ── Phase 3: GPU Particle System update ──
        if let Some(ref mut gpu_ps) = gpu_particles {
            // Mood-driven emission parameters (matching OrbEmitter behavior)
            let (rate, speed, turb, lifetime) = match mood {
                Mood::Serene => (3.0_f32, 12.0, 0.1, 2.8),
                Mood::Alert => (6.0, 20.0, 0.3, 2.2),
                Mood::Stressed => (10.0, 35.0, 0.6, 1.6),
                Mood::Critical => (16.0, 55.0, 1.0, 1.2),
            };
            let orb_scale = h as f32 / 480.0 * 0.6;
            let orb_radius = (38.0 + cpu * 0.34) * orb_scale;
            gpu_ps.spawn(dt, ecs_orb_x, ecs_orb_y, rate, speed, turb, lifetime, mood);
            gpu_ps.update(dt, t, ecs_orb_x, ecs_orb_y, orb_radius);
        }

        // ── Phase 4: Spatial hash for weather particle interactions ──
        if weather_fx.active_count > 0 {
            spatial_hash.clear();
            for i in 0..weather_fx.active_count {
                let p = &weather_fx.particles[i];
                spatial_hash.insert(i, p.x, p.y);
            }
        }

        // ── Update weather particle system ──
        let wf_w = w as f32;
        let wf_h = h as f32;
        let has_weather_data = weather_temp.is_some();
        {
            let wc = weather_code;
            // Re-init pool when weather type changes
            if weather_fx.initialized_for != wc && has_weather_data {
                weather_fx.setup(wc, wf_w, wf_h, mood, weather_wind_kph.unwrap_or(0.0));
            }
            if has_weather_data {
                weather_fx.update(dt, t, wf_w, wf_h, ecs_orb_x, ecs_orb_y, mood, wc);
            }
        }

        if thought_cooldown > 0.0 {
            thought_cooldown = (thought_cooldown - dt).max(0.0);
        }

        // ══════════════════════════════════════
        //  Process pending thoughts
        // ══════════════════════════════════════

        // If typewriter is idle and cooldown expired, start next thought.
        // Prioritise AI thoughts (StreamToken / AI Complete) over system messages
        // so LLM output is never starved by telemetry chatter.
        if tw_done && thought_cooldown <= 0.0 {
            // Find the first AI payload (StreamToken or AI Complete) if any exist
            let ai_idx = pending_queue.iter().position(|p| {
                matches!(
                    p,
                    ThoughtPayload::StreamToken(_) | ThoughtPayload::Complete { is_ai: true, .. }
                )
            });
            let next_payload = if let Some(idx) = ai_idx {
                pending_queue.remove(idx)
            } else {
                pending_queue.pop_front()
            };

            if let Some(payload) = next_payload {
                match payload {
                    ThoughtPayload::Complete {
                        text,
                        is_ai,
                        is_system,
                    } => {
                        let text = normalize_ascii_text(&text);
                        if is_system {
                            // System messages skip typewriter — archive directly
                            let truncated = if text.chars().count() > 140 {
                                text.chars().take(140).collect::<String>()
                            } else {
                                text.clone()
                            };
                            cognitive_log.push(CognitiveEntry {
                                text: truncated,
                                is_monologue: false,
                                is_system: true,
                                born_at: t,
                            });
                            while cognitive_log.len() > max_log_entries {
                                cognitive_log.remove(0);
                            }
                            thought_cooldown = 0.1;
                        } else {
                            tw_target = text;
                            tw_stream_buf.clear();
                            tw_display.clear();
                            tw_char_idx = 0;
                            tw_available = tw_target.chars().count();
                            tw_timer = 0.0;
                            tw_done = false;
                            tw_is_monologue = is_ai;
                            tw_is_system = false;
                            tw_is_streaming = false;
                            tw_stream_ended = true;

                            setup_hesitation_and_correction(
                                t,
                                tw_available,
                                &mut hesitate_at,
                                &mut hesitate_cooldown,
                                &mut sc_phase,
                                &mut sc_will_happen,
                                &mut sc_trigger_at,
                                &mut sc_chars_left,
                            );
                            aberration_timer = 0.6;
                            pulse_phase = 0.0;
                            thought_burst_flag = true;
                        }
                    }
                    ThoughtPayload::StreamToken(tok) => {
                        let tok = normalize_ascii_text(&tok);
                        // First token of a new streaming thought
                        tw_target.clear();
                        tw_stream_buf = tok;
                        tw_display.clear();
                        tw_char_idx = 0;
                        tw_available = tw_stream_buf.chars().count();
                        tw_timer = 0.0;
                        tw_done = false;
                        tw_is_monologue = true;
                        tw_is_system = false;
                        tw_is_streaming = true;
                        tw_stream_ended = false;

                        hesitate_at = None;
                        hesitate_cooldown = 0.0;
                        sc_phase = 0;
                        sc_will_happen = false;
                        aberration_timer = 0.6;
                        pulse_phase = 0.0;
                        thought_burst_flag = true;
                    }
                    ThoughtPayload::StreamEnd => {
                        // Stale end marker — ignore
                    }
                }
                while cognitive_log.len() > max_log_entries {
                    cognitive_log.remove(0);
                }
            }
        }

        // If we're mid-stream, consume additional tokens / end markers
        if tw_is_streaming && !tw_stream_ended {
            while let Some(payload) = pending_queue.front() {
                match payload {
                    ThoughtPayload::StreamToken(tok) => {
                        let tok = normalize_ascii_text(tok);
                        tw_stream_buf.push_str(&tok);
                        tw_available = tw_stream_buf.chars().count();
                        pending_queue.pop_front();
                    }
                    ThoughtPayload::StreamEnd => {
                        tw_stream_ended = true;
                        pending_queue.pop_front();
                        // Full length known — set up hesitation
                        setup_hesitation_and_correction(
                            t,
                            tw_available,
                            &mut hesitate_at,
                            &mut hesitate_cooldown,
                            &mut sc_phase,
                            &mut sc_will_happen,
                            &mut sc_trigger_at,
                            &mut sc_chars_left,
                        );
                        break;
                    }
                    ThoughtPayload::Complete { .. } => break,
                }
            }
        }

        // ══════════════════════════════════════
        //  Typewriter tick
        // ══════════════════════════════════════
        if !tw_done {
            let source = if tw_is_streaming {
                &tw_stream_buf
            } else {
                &tw_target
            };

            if let Some(h_at) = hesitate_at {
                if tw_char_idx == h_at && hesitate_cooldown <= 0.0 {
                    hesitate_cooldown = 0.4 + hash_f((t * 137.0) as u32) * 0.5;
                    hesitate_at = None; // fire once — prevents infinite retrigger
                }
            }

            if hesitate_cooldown > 0.0 {
                hesitate_cooldown -= dt;
            } else if sc_phase == 1 {
                sc_timer -= dt;
                if sc_timer <= 0.0 {
                    if sc_chars_left > 0 && tw_char_idx > 0 {
                        tw_char_idx -= 1;
                        tw_display = source.chars().take(tw_char_idx).collect();
                        sc_chars_left -= 1;
                        sc_timer = 0.04;
                        tokens_this_sec += 1;
                    } else {
                        sc_phase = 2;
                    }
                }
            } else {
                if sc_phase == 2 {
                    sc_phase = 0;
                }

                // ── Variable-cadence typewriter ──
                // Base cadence shifts by mood: serene = contemplative,
                // critical = urgent staccato. Entropy injects per-character
                // jitter so the rhythm "breathes" with system stress.
                let mood_cps: f32 = match mood {
                    Mood::Serene => 22.0,
                    Mood::Alert => 34.0,
                    Mood::Stressed => 55.0,
                    Mood::Critical => 78.0,
                };
                let entropy_n = entropy.clamp(0.0, 1.0);
                let jitter_amt = 0.25 + entropy_n * 0.6;
                let raw = hash_f((tw_char_idx as u32).wrapping_mul(7919));
                let jitter = 1.0 + (raw - 0.5) * jitter_amt;
                let cps = (mood_cps * jitter).max(6.0);

                // System lines stay brisk regardless of mood.
                let base_speed = if tw_is_system { 0.018 } else { 1.0 / cps };
                tw_timer += dt;
                if tw_timer >= base_speed {
                    tw_timer = 0.0;
                    if tw_char_idx < tw_available {
                        tw_char_idx += 1;
                        tw_display = source.chars().take(tw_char_idx).collect();
                        tokens_this_sec += 1;

                        // ── Punctuation hesitation (organic "thinking" beats) ──
                        // Only for AI thoughts — system lines never pause mid-line.
                        if !tw_is_system && tw_char_idx > 0 {
                            if let Some(ch) = source.chars().nth(tw_char_idx - 1) {
                                let mood_scale: f32 = match mood {
                                    Mood::Serene => 1.4,
                                    Mood::Alert => 1.0,
                                    Mood::Stressed => 0.55,
                                    Mood::Critical => 0.30,
                                };
                                let pause = match ch {
                                    ',' => 0.18,
                                    '.' | '?' | '!' => 0.42 + entropy_n * 0.20,
                                    ';' | ':' => 0.25,
                                    ' ' => {
                                        // Rare mid-thought hesitation,
                                        // more likely when entropy is low (calm musing).
                                        let r =
                                            hash_f((tw_char_idx as u32).wrapping_mul(2147483647));
                                        if r < 0.04 {
                                            0.12 + (1.0 - entropy_n) * 0.18
                                        } else {
                                            0.0
                                        }
                                    }
                                    _ => 0.0,
                                };
                                if pause > 0.0 {
                                    hesitate_cooldown = pause * mood_scale;
                                }
                            }
                        }

                        if sc_will_happen && sc_phase == 0 && tw_char_idx == sc_trigger_at {
                            sc_phase = 1;
                            sc_timer = 0.04;
                            sc_will_happen = false;
                        }
                    } else if tw_stream_ended || !tw_is_streaming {
                        // Fully typed — archive
                        tw_done = true;
                        // System messages get minimal cooldown; AI thoughts get a beat
                        thought_cooldown = if tw_is_system { 0.3 } else { 1.8 };
                        let final_text = if tw_is_streaming {
                            tw_stream_buf.trim().to_string()
                        } else {
                            tw_target.clone()
                        };
                        let final_text = if final_text.chars().count() > 140 {
                            // Take first 140 chars, then trim back to last sentence end if any.
                            let head: String = final_text.chars().take(140).collect();
                            if let Some(end) = head.rfind(|c| c == '.' || c == '!' || c == '?') {
                                head[..=end].to_string()
                            } else {
                                head
                            }
                        } else {
                            final_text
                        };
                        cognitive_log.push(CognitiveEntry {
                            text: final_text,
                            is_monologue: tw_is_monologue,
                            is_system: tw_is_system,
                            born_at: t,
                        });
                        while cognitive_log.len() > max_log_entries {
                            cognitive_log.remove(0);
                        }
                    }
                    // else: streaming but no new chars yet — typewriter waits naturally
                }
            }
        }

        // ── Token velocity ──
        token_sec_timer += dt;
        if token_sec_timer >= 1.0 {
            token_vel = tokens_this_sec as f32;
            tokens_this_sec = 0;
            token_sec_timer = 0.0;
        }
        token_vel_display += (token_vel - token_vel_display) * dt * 5.0;

        // Use real LLM token rate if available, otherwise show typewriter velocity
        let display_tps = if llm_tps > 0.1 {
            llm_tps
        } else {
            token_vel_display
        };

        // Update alert system with real telemetry + latest nerve action
        let newest_action = action_log_snap.last();
        alert_system.update(
            dt,
            cpu,
            mem,
            load_avg,
            entropy,
            entropy_trend,
            mood,
            net_rx_rate,
            net_tx_rate,
            newest_action,
        );

        // Nerve impulse flash — triggers when new action completes
        if action_count > prev_action_count {
            nerve_flash_timer = 2.5;
            prev_action_count = action_count;
        }
        nerve_flash_timer = (nerve_flash_timer - dt).max(0.0);

        // Nerve burst — connect impulses to the synaptic web + spectral system
        if nerve_burst_flag {
            spectral.trigger_burst();
            // Fire a synapse for the nerve impulse (uses AI orb position + offset)
            let nx = ecs_orb_x + 40.0;
            let ny = ecs_orb_y + 30.0;
            if enable_synaptic_web {
                synaptic_web.add_thought(
                    nx,
                    ny,
                    mood,
                    last_thought_kind_idx,
                    t,
                    w as f32,
                    h as f32,
                );
                synapse_dirty = true;
            }
            nerve_burst_flag = false;
        }

        // Spectral analyzer — driven by telemetry, LLM activity, and thought bursts
        let star_thought_burst = thought_burst_flag;
        if thought_burst_flag {
            spectral.trigger_burst();
            thought_pulses.push(ThoughtPulse::new(ecs_orb_x, ecs_orb_y));
            // Synaptic web — new neuron for each thought
            if enable_synaptic_web {
                synaptic_web.add_thought(
                    ecs_orb_x,
                    ecs_orb_y,
                    mood,
                    last_thought_kind_idx,
                    t,
                    w as f32,
                    h as f32,
                );
                synapse_dirty = true;
            }
            last_thought_kind_idx = (last_thought_kind_idx + 1) % 13;
            thought_burst_flag = false;
        }
        spectral.update(
            dt,
            t,
            cpu,
            mem,
            entropy,
            entropy_components,
            net_rx_rate,
            net_tx_rate,
            load_avg,
            is_thinking,
        );

        // Starfield — drift, twinkle, pulse-flares, shooting stars
        starfield.update(dt, mood, &thought_pulses, star_thought_burst);

        // ── Phase 5: Spectral FFT update ──
        {
            let net_n = ((net_rx_rate + net_tx_rate) / 10_000_000.0).min(1.0) as f32;
            spectral_fft.push_telemetry(t, cpu, mem, net_n, entropy, load_avg);
            spectral_fft.analyze(dt);
        }

        // Update thought pulses
        for pulse in thought_pulses.iter_mut() {
            pulse.update(dt);
        }
        thought_pulses.retain(|p| p.alive());
        // Update synaptic web
        if enable_synaptic_web {
            synaptic_web.update(dt, w as f32, h as f32);
        }

        // Throttled persistence: save at most once every 30s, and only
        // when something has changed since the last save.
        synapse_save_timer -= dt;
        if enable_synaptic_web && synapse_save_timer <= 0.0 {
            synapse_save_timer = 30.0;
            if synapse_dirty {
                match synaptic_web.save_to_disk(&synapse_save_path) {
                    Ok(n) => eprintln!("[aura] synaptic memory persisted: {} neurons", n),
                    Err(e) => eprintln!("[aura] synaptic memory save failed: {}", e),
                }
                synapse_dirty = false;
            }
        }

        if pulse_phase >= 0.0 {
            pulse_phase += dt * 0.8;
            if pulse_phase > 1.0 {
                pulse_phase = if is_thinking { 0.0 } else { -1.0 };
            }
        } else if is_thinking {
            pulse_phase = 0.0;
        }

        scan_y = (scan_y + dt * 40.0) % h as f32;
        if aberration_timer > 0.0 {
            aberration_timer = (aberration_timer - dt).max(0.0);
        }

        // ══════════════════════════════════════
        //  DRAWING
        // ══════════════════════════════════════
        grain_seed = grain_seed.wrapping_add(1);

        // ── Weather-reactive palette ──
        // Classify weather from WMO code
        let wc = weather_code;
        let (_is_clear, is_cloudy, is_fog, is_rain, is_snow, is_storm) = classify_weather(wc);
        let has_weather_data = weather_temp.is_some();

        // Temperature tint: cold shifts blue, hot shifts warm
        let temp_c = weather_temp.unwrap_or(15.0);
        let cold_factor = ((10.0 - temp_c) / 30.0).clamp(0.0, 1.0); // 1.0 at -20C
        let warm_factor = ((temp_c - 20.0) / 25.0).clamp(0.0, 1.0); // 1.0 at 45C

        let (bg, icy, _frost, teal, green, violet, star_alpha_mul, aurora_speed_mul) =
            if !has_weather_data {
                // No weather yet — use defaults
                (
                    bg_default,
                    icy_default,
                    frost_default,
                    teal_default,
                    green_default,
                    violet_default,
                    1.0f32,
                    1.0f32,
                )
            } else if is_storm {
                // THUNDERSTORM: dark purple-red bg, intense violet/magenta aurora, dramatic
                (
                    Color::new(12, 8, 18, 255),
                    Color::new(200, 140, 255, 255),
                    Color::new(255, 180, 220, 180),
                    Color::new(180, 60, 200, 200),
                    Color::new(140, 50, 180, 120),
                    Color::new(200, 60, 120, 110),
                    1.6,
                    2.5,
                )
            } else if is_snow {
                // SNOW: pale icy blue bg, white/crystalline palette, gentle
                (
                    Color::new(14, 18, 28, 255),
                    Color::new(200, 230, 255, 255),
                    Color::new(230, 240, 255, 180),
                    Color::new(140, 200, 240, 140),
                    Color::new(100, 180, 220, 80),
                    Color::new(160, 170, 220, 60),
                    1.2,
                    0.5,
                )
            } else if is_rain {
                // RAIN: deep blue-grey bg, blue-tinted aurora, subdued
                (
                    Color::new(8, 12, 22, 255),
                    Color::new(70, 150, 220, 220),
                    Color::new(140, 170, 210, 120),
                    Color::new(30, 120, 180, 140),
                    Color::new(40, 140, 160, 70),
                    Color::new(80, 70, 160, 60),
                    1.4,
                    0.7,
                )
            } else if is_fog {
                // FOG: murky grey-blue bg, very dim and close, ghostly
                (
                    Color::new(18, 22, 30, 255),
                    Color::new(140, 170, 190, 160),
                    Color::new(160, 175, 190, 100),
                    Color::new(80, 130, 140, 80),
                    Color::new(60, 120, 110, 40),
                    Color::new(100, 90, 130, 30),
                    1.3,
                    0.3,
                )
            } else if is_cloudy {
                // OVERCAST: slightly muted, desaturated, slower
                let dim = 0.7_f32;
                (
                    Color::new(12, 16, 24, 255),
                    Color::new(
                        (100.0 * dim) as u8,
                        (210.0 * dim) as u8,
                        (255.0 * dim) as u8,
                        230,
                    ),
                    Color::new(180, 200, 220, 120),
                    Color::new(
                        (20.0 * dim) as u8,
                        (190.0 * dim) as u8,
                        (180.0 * dim) as u8,
                        130,
                    ),
                    Color::new(
                        (50.0 * dim) as u8,
                        (210.0 * dim) as u8,
                        (130.0 * dim) as u8,
                        65,
                    ),
                    Color::new(
                        (140.0 * dim) as u8,
                        (90.0 * dim) as u8,
                        (220.0 * dim) as u8,
                        50,
                    ),
                    1.2,
                    0.7,
                )
            } else {
                // CLEAR / default — apply temperature tint
                let r_bg = (10.0 + warm_factor * 6.0 - cold_factor * 2.0) as u8;
                let g_bg = (14.0 - warm_factor * 4.0 + cold_factor * 2.0) as u8;
                let b_bg = (22.0 + cold_factor * 6.0 - warm_factor * 4.0) as u8;
                (
                    Color::new(r_bg, g_bg, b_bg, 255),
                    Color::new(
                        (100.0 + warm_factor * 60.0) as u8,
                        (210.0 - warm_factor * 40.0) as u8,
                        (255.0 - warm_factor * 60.0) as u8,
                        255,
                    ),
                    frost_default,
                    Color::new(
                        (20.0 + warm_factor * 30.0) as u8,
                        (190.0 - warm_factor * 40.0) as u8,
                        (180.0 - warm_factor * 30.0) as u8,
                        160,
                    ),
                    Color::new(
                        (50.0 + warm_factor * 40.0) as u8,
                        (210.0 - warm_factor * 60.0) as u8,
                        (130.0 - warm_factor * 40.0) as u8,
                        90,
                    ),
                    violet_default,
                    1.6,
                    1.0,
                )
            };

        // User preference: hide intrusive stars while keeping nebula/horizon.
        // Compute derived alphas so we can silence star layers independently.
        // Update celestial geometry from the wall clock BEFORE deriving
        // sky-dependent alphas so the horizon glow tracks the current sun.
        let epoch_secs: i64 = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            let secs_f64 = now.as_secs_f64();
            celestial.update(weather_lat, weather_lon, secs_f64);
            now.as_secs() as i64
        };
        // Day/night factor: 1.0 at full daylight, 0.0 in deep night.
        let day_factor = ((celestial.sun_alt + 6.0) / 12.0).clamp(0.0, 1.0);
        // Nebula fades and cools at night so the warm mood tint doesn't
        // leak through as a faux sunset wash after dark, and so the moon
        // reads cleanly without competing fog blobs.
        let nebula_alpha = star_alpha_mul.max(0.4) * (0.10 + 0.90 * day_factor);
        // Horizon glow is driven by sun altitude — bright at twilight,
        // fades to nothing at deep night so the moon scene reads cleanly.
        let horizon_alpha = {
            let alt = celestial.sun_alt;
            let day_drop = (1.0 - (alt / 25.0).clamp(0.0, 1.0)).max(0.0);
            let night_drop = (1.0 + (alt / 10.0)).clamp(0.0, 1.0);
            (day_drop * night_drop * star_alpha_mul.max(0.5)).clamp(0.0, 1.0)
        };
        let star_alpha = 0.0f32; // set to 0 to remove stars entirely

        let mut d = rl.begin_drawing(&thread);
        d.clear_background(bg);

        // ── Celestial body (sun / moon) — real wall-clock driven ──
        // Geometry was updated above; just render the dominant body
        // before the nebula so atmospheric blobs sit in front of it.
        celestial.draw(&mut d, w, h);

        // 0. Atmospheric nebula field — drifting volumetric fog blobs
        //    behind everything; gives dead middle-space depth & color.
        //    Cools toward a cobalt night palette as the sun sets.
        crate::fx::atmosphere::draw_nebula_field_tinted(
            &mut d,
            w,
            h,
            t,
            mood,
            nebula_alpha,
            day_factor,
        );

        // 1. Starfield — multi-layer parallax stars + shooting stars.
        //    Drifts, twinkles, flares from passing thought-pulse rings,
        //    and the brightest tier additionally feeds the bloom buffer.
        starfield.draw(&mut d, t, mood, star_alpha);

        let band_colors = [green, teal, violet];
        let t_wx = t * aurora_speed_mul; // weather-modulated time for aurora

        // 1b. Deep parallax aurora layer disabled to reduce visual noise.
        let _ = (t_wx, cpu); // keep variables alive for downstream usage

        // 1b2. Horizon glow — soft floor light suggesting earth-curve
        draw_horizon_glow(&mut d, w, h, mood, horizon_alpha);

        // 1b3. Drifting weather clouds — animated, color/density per WMO code
        if has_weather_data {
            weather_fx.draw_clouds(&mut d, weather_code, mood);
        }

        // 1b4. Earth half-globe (left edge, satellite view)
        if let Some(globe) = earth_globe.as_mut() {
            // Pull the disk into the screen so the full hemisphere is
            // clearly visible on the left side.
            let radius = (h as f32 * 0.40).min(w as f32 * 0.30);
            let cx = radius * 0.55; // fully on-screen, hugging left
            let cy = h as f32 * 0.50;
            let g_alpha = 0.92;
            globe.draw(
                &mut d,
                w,
                h,
                cx,
                cy,
                radius,
                t,
                celestial.sun_az,
                g_alpha,
                epoch_secs as f32,
            );

            // Whenever the sun's actual screen position is occluded by
            // (or near) the globe disc, paint a strong directional
            // corona on the sun-facing limb. The direction is derived
            // from the real projected sun position so it points to
            // exactly where the sun would be if the planet weren't in
            // the way. Intensity tracks `sun_alt`, so it peaks at
            // sunrise/sunset and softens at deep night.
            let (sun_sx, sun_sy, _sun_alt) = celestial.sun_screen_pos(w, h);
            celestial.draw_globe_sun_glow(&mut d, cx, cy, radius, sun_sx, sun_sy);
        }

        // 1c. Synaptic web — neural constellation (behind aurora, above deep layer)
        if enable_synaptic_web {
            synaptic_web.draw(&mut d, mood, t, 1.0);
        }

        // Glow buffer — render bright elements into offscreen RT for bloom
        {
            let mut g = d.begin_texture_mode(&thread, &mut glow_rt);
            g.clear_background(Color::new(0, 0, 0, 0));
            // Bright stars + shooting-star streaks contribute to bloom so
            // the constellation reads as luminous, not flat dots.
            starfield.draw_glow(&mut g, t, mood, star_alpha);
            // Aurora deep + ribbons disabled in bloom feed (visual noise reduction)
            let _ = (band_colors, spike, pulse_phase);
            orb_trail.draw(&mut g, teal, icy, h, 2.0);
            // Bloom feed for the orb — drawn at ECS coords so the bloom halo
            // lands on the SDF orb body (which also uses ECS coords). Without
            // this alignment the SDF body looks dim because its halo is glowing
            // somewhere else on screen.
            {
                let orb_scale = h as f32 / 480.0 * 0.6;
                let core_r = (78.0 + cpu * 0.32) * orb_scale;
                g.draw_circle_gradient(
                    ecs_orb_x as i32,
                    ecs_orb_y as i32,
                    core_r * 1.30,
                    Color::new(140, 235, 255, 50),
                    Color::new(0, 0, 0, 0),
                );
            }
            orb_emitter.draw(&mut g, teal, icy, mood, 3.0);

            // Spectral waveform glow contribution disabled (noise reduction)
            if false {
                let spec_y = h as f32 * 0.82;
                let spec_h = h as f32 * 0.10;
                let margin = w as f32 * 0.12;
                let total_w = w as f32 - margin * 2.0;
                let band_w = total_w / SPEC_BANDS as f32;
                let gap = (band_w * 0.15).max(1.0);
                let bar_w = band_w - gap;

                for i in 0..SPEC_BANDS {
                    let amp = spectral.bands[i];
                    if amp < 0.01 {
                        continue;
                    }
                    let bx = margin + i as f32 * band_w;
                    let bar_h = amp * spec_h;
                    let by = spec_y - bar_h;

                    // Frequency-based color: bass=cyan, mid=teal, high=purple
                    let f = i as f32 / SPEC_BANDS as f32;
                    let r = (80.0 + f * 120.0) as u8;
                    let gc = (220.0 - f * 80.0) as u8;
                    let b = (255.0 - f * 30.0) as u8;
                    let glow_a = (amp * 180.0).min(180.0) as u8;

                    g.draw_rectangle(
                        bx as i32,
                        by as i32,
                        bar_w.max(1.0) as i32,
                        bar_h as i32,
                        Color::new(r, gc, b, glow_a),
                    );
                }

                // Spline connector bloom — smooth Catmull-Rom halo on the waveform curve
                let gpts: Vec<(f32, f32)> = (0..SPEC_BANDS)
                    .map(|i| {
                        let fbm_y = spectral.fbm_offsets[i] * spec_h;
                        (
                            margin + i as f32 * band_w + bar_w * 0.5,
                            spec_y - spectral.bands[i] * spec_h + fbm_y,
                        )
                    })
                    .collect();
                for i in 1..SPEC_BANDS {
                    let amp0 = spectral.bands[i - 1];
                    let amp1 = spectral.bands[i];
                    if (amp0 + amp1) * 0.5 < 0.015 {
                        continue;
                    }
                    let p0 = gpts[if i >= 2 { i - 2 } else { 0 }];
                    let p1 = gpts[i - 1];
                    let p2 = gpts[i];
                    let p3 = gpts[(i + 1).min(SPEC_BANDS - 1)];
                    let avg = (amp0 + amp1) * 0.5;
                    let f = i as f32 / SPEC_BANDS as f32;
                    let gr = (80.0 + f * 120.0) as u8;
                    let gg = (220.0 - f * 80.0) as u8;
                    let gb = (255.0 - f * 30.0) as u8;
                    let ga = (avg * 140.0).min(140.0) as u8;
                    let mut prev = p1;
                    for s in 1..=4usize {
                        let u = s as f32 / 4.0;
                        let u2 = u * u;
                        let u3 = u2 * u;
                        let nx = 0.5
                            * (2.0 * p1.0
                                + (-p0.0 + p2.0) * u
                                + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * u2
                                + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * u3);
                        let ny = 0.5
                            * (2.0 * p1.1
                                + (-p0.1 + p2.1) * u
                                + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * u2
                                + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * u3);
                        g.draw_line_ex(
                            rvec2(prev.0, prev.1),
                            rvec2(nx, ny),
                            2.5,
                            Color::new(gr, gg, gb, ga),
                        );
                        prev = (nx, ny);
                    }
                }
            }

            // Thought pulse rings in glow buffer
            for pulse in &thought_pulses {
                let ring_a = (pulse.alpha * 120.0) as u8;
                if ring_a > 2 {
                    g.draw_circle_lines(
                        pulse.x as i32,
                        pulse.y as i32,
                        pulse.radius,
                        Color::new(icy.r, icy.g, icy.b, ring_a),
                    );
                    // Slightly wider faint ring for glow spread
                    g.draw_circle_lines(
                        pulse.x as i32,
                        pulse.y as i32,
                        pulse.radius + 3.0,
                        Color::new(icy.r, icy.g, icy.b, ring_a / 3),
                    );
                }
            }

            // Synaptic web glow — neural constellation bloom contribution
            if enable_synaptic_web {
                synaptic_web.draw(&mut g, mood, t, 2.0);
            }
        }

        // Draw starfield AGAIN on top of the composite to ensure visibility
        // (overlay pass: larger alpha, additive feel)
        starfield.draw(&mut d, t, mood, star_alpha);

        // 2. Aurora ribbons disabled to reduce visual noise.

        // 3. Orb trail + orb + emission motes
        // 3. Orb trail + emission motes (drawn first so the orb sits on top
        //    of its own trail and motes; orb body itself is drawn AFTER the
        //    weather pass below so rain/snow/lightning never occlude it).
        orb_trail.draw(&mut d, teal, icy, h, 1.0);
        // Phase 3: GPU instanced particles replace CPU emitter when available
        if let Some(ref gpu_ps) = gpu_particles {
            gpu_ps.draw(w as f32, h as f32, mood_to_float(mood));
        } else {
            orb_emitter.draw(&mut d, teal, icy, mood, 1.0);
        }
        // (orb body + glitch moved to after weather pass for proper z-order)

        // 4b. Weather particle effects — physics-driven with orb interaction.
        //     All draw logic now lives inside WeatherFX so this site stays clean.
        if has_weather_data {
            let wc = weather_code;
            let is_fog = wc == 45 || wc == 48;

            // Storm desaturation tint (cheap, single rect)
            weather_fx.draw_atmosphere_tint(&mut d, w, h, wc, t);

            // Fog: soft vertical-gradient bands
            if is_fog {
                weather_fx.draw_fog(&mut d, w, h, t);
            }

            // Rain / snow / sleet particles (depth-graded streaks + flakes)
            weather_fx.draw_particles(&mut d, wf_h, wc);

            // Bottom-of-frame splashes from rain impact
            weather_fx.draw_splashes(&mut d);

            // Lightning: full-screen flash, fractal bolts, ground-strike glow
            weather_fx.draw_lightning(&mut d, w, h, mood);
        }
        let _ = wf_w;

        // 4c. Orb body + glitch — moved BELOW the vignette pass so the orb
        //     stays crisp and luminous instead of being dimmed by the corner
        //     wash. Bloom halo is composited from the glow buffer (which
        //     already contains the orb) so the body still reads as glowing.
        // (drawn after section 6c)

        // 5. Bloom overlay — Phase 6 Kawase or legacy 3-pass
        if let Some(ref mut bloom) = kawase_bloom {
            bloom.process(&mut d, &thread, &glow_rt, is_thinking, pulse_phase >= 0.0);
            bloom.composite(&mut d, w, h);
        } else {
            use raylib::core::texture::RaylibTexture2D;
            let tex = glow_rt.texture();
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
            let bloom_base: f32 = if is_thinking { 0.55 } else { 0.38 };
            let pulse_b: f32 = if pulse_phase >= 0.0 { 0.20 } else { 0.0 };
            let bloom_a = (bloom_base + pulse_b).min(1.0);

            {
                let mut blend = d.begin_blend_mode(BlendMode::BLEND_ADDITIVE);
                blend.draw_texture_pro(
                    tex,
                    src,
                    dest,
                    rvec2(0.0, 0.0),
                    0.0,
                    Color::new(255, 255, 255, (255.0 * bloom_a) as u8),
                );
                let blur_scale = 1.04;
                let blur_dest = Rectangle {
                    x: -(w as f32) * (blur_scale - 1.0) / 2.0,
                    y: -(h as f32) * (blur_scale - 1.0) / 2.0,
                    width: w as f32 * blur_scale,
                    height: h as f32 * blur_scale,
                };
                blend.draw_texture_pro(
                    tex,
                    src,
                    blur_dest,
                    rvec2(0.0, 0.0),
                    0.0,
                    Color::new(255, 255, 255, (140.0 * bloom_a) as u8),
                );
                let blur_scale2 = 1.08;
                let blur_dest2 = Rectangle {
                    x: -(w as f32) * (blur_scale2 - 1.0) / 2.0,
                    y: -(h as f32) * (blur_scale2 - 1.0) / 2.0,
                    width: w as f32 * blur_scale2,
                    height: h as f32 * blur_scale2,
                };
                blend.draw_texture_pro(
                    tex,
                    src,
                    blur_dest2,
                    rvec2(0.0, 0.0),
                    0.0,
                    Color::new(255, 255, 255, (60.0 * bloom_a) as u8),
                );
            }
        }

        // 5b. DREAM MODE overlay — when dream_sequence has been called, the
        //     orb softens into a slow purple/cyan reverie that decays over
        //     ~25s. Two layered effects:
        //       (a) extra over-scaled additive bloom from the glow buffer,
        //           giving the orb a "halo bleeding past its edges" look;
        //       (b) full-screen additive tint pulsing with a slow sine that
        //           drifts between purple and cyan -- subtle, never overwhelming.
        if dream_intensity > 0.0 {
            use raylib::core::texture::RaylibTexture2D;
            let di = dream_intensity.min(1.0); // visual cap
                                               // (a) Over-scaled glow bleed for the dream halo.
            let tex = glow_rt.texture();
            let tw = tex.width() as f32;
            let th = tex.height() as f32;
            let src = Rectangle {
                x: 0.0,
                y: 0.0,
                width: tw,
                height: -th,
            };
            let dream_scale = 1.18;
            let dream_dest = Rectangle {
                x: -(w as f32) * (dream_scale - 1.0) / 2.0,
                y: -(h as f32) * (dream_scale - 1.0) / 2.0,
                width: w as f32 * dream_scale,
                height: h as f32 * dream_scale,
            };
            // (b) Slow sine drift between purple (R-heavy) and cyan (B-heavy),
            //     period ~6s. Multiplied by `di` so the tint fades with the
            //     reverie instead of cutting off.
            let drift = (t_f32 * 1.05).sin() * 0.5 + 0.5; // 0..1
            let r_tint = (110.0 + 90.0 * (1.0 - drift)) as u8; // 200 → 110
            let g_tint = (60.0 + 40.0 * drift) as u8; // 60 → 100
            let b_tint = (160.0 + 80.0 * drift) as u8; // 160 → 240
            {
                let mut blend = d.begin_blend_mode(BlendMode::BLEND_ADDITIVE);
                blend.draw_texture_pro(
                    tex,
                    src,
                    dream_dest,
                    rvec2(0.0, 0.0),
                    0.0,
                    Color::new(r_tint, g_tint, b_tint, (170.0 * di) as u8),
                );
                // Soft full-screen wash — very low alpha, just enough to feel.
                let wash_a = (28.0 * di) as u8;
                blend.draw_rectangle(0, 0, w, h, Color::new(r_tint, g_tint, b_tint, wash_a));
            }
        }

        // ══════════════════════════════════════════════════════════
        //  5b. Spectral Waveform Analyzer disabled (visual noise reduction)
        // ══════════════════════════════════════════════════════════
        if false {
            let spec_y = h as f32 * 0.82;
            let spec_h = h as f32 * 0.10;
            let margin = w as f32 * 0.12;
            let total_w = w as f32 - margin * 2.0;
            let band_w = total_w / SPEC_BANDS as f32;
            let gap = (band_w * 0.15).max(1.0);
            let bar_w = band_w - gap;

            // Mood-reactive base color for spectral bars
            let (spec_r, spec_g, spec_b) = match mood {
                Mood::Serene => (60u8, 200u8, 240u8),
                Mood::Alert => (200, 180, 80),
                Mood::Stressed => (240, 130, 60),
                Mood::Critical => (255, 60, 50),
            };

            // Draw bars with vertical gradient and mirror reflection
            for i in 0..SPEC_BANDS {
                let amp = spectral.bands[i];
                if amp < 0.01 {
                    continue;
                }
                let bx = margin + i as f32 * band_w;
                let bar_h = amp * spec_h;
                let by = spec_y - bar_h;

                // Frequency-blended color
                let f = i as f32 / SPEC_BANDS as f32;
                let r = (spec_r as f32 * (1.0 - f * 0.4)) as u8;
                let gc = (spec_g as f32 * (1.0 - f * 0.2)) as u8;
                let b = (spec_b as f32 * (0.7 + f * 0.3)).min(255.0) as u8;

                // Main bar — gradient segments (brighter at top)
                let segments = (bar_h / 3.0).max(1.0) as i32;
                let seg_h = (bar_h / segments as f32).max(1.0);
                for s in 0..segments {
                    let seg_y = by + s as f32 * seg_h;
                    let grad = 1.0 - (s as f32 / segments as f32) * 0.6; // top bright, bottom dim
                    let a = (amp * 90.0 * grad).min(130.0) as u8;
                    d.draw_rectangle(
                        bx as i32,
                        seg_y as i32,
                        bar_w.max(1.0) as i32,
                        seg_h.ceil() as i32,
                        Color::new(r, gc, b, a),
                    );
                }

                // Mirror reflection below baseline — subtle, inverted
                let refl_h = bar_h * 0.35;
                let refl_segs = (refl_h / 3.0).max(1.0) as i32;
                let refl_seg_h = (refl_h / refl_segs as f32).max(1.0);
                for s in 0..refl_segs {
                    let seg_y = spec_y + s as f32 * refl_seg_h;
                    let grad = 1.0 - (s as f32 / refl_segs as f32); // fades away from baseline
                    let a = (amp * 25.0 * grad).min(40.0) as u8;
                    if a > 1 {
                        d.draw_rectangle(
                            bx as i32,
                            seg_y as i32,
                            bar_w.max(1.0) as i32,
                            refl_seg_h.ceil() as i32,
                            Color::new(r, gc, b, a),
                        );
                    }
                }

                // Peak hold dot — floats above the bar, slowly descends
                let peak = spectral.peaks[i];
                if peak > 0.03 {
                    let peak_y = spec_y - peak * spec_h - 2.0;
                    let peak_a = (peak * 200.0).min(200.0) as u8;
                    d.draw_rectangle(
                        bx as i32,
                        peak_y as i32,
                        bar_w.max(1.0) as i32,
                        2,
                        Color::new(r, gc, b, peak_a),
                    );
                }
            }

            // Waveform connector — Catmull-Rom spline with multi-pass volumetric glow
            {
                // Build control points from bar tops + fBM displacement
                let pts: Vec<(f32, f32)> = (0..SPEC_BANDS)
                    .map(|i| {
                        let amp = spectral.bands[i];
                        let fbm_y = spectral.fbm_offsets[i] * spec_h;
                        (
                            margin + i as f32 * band_w + bar_w * 0.5,
                            spec_y - amp * spec_h + fbm_y,
                        )
                    })
                    .collect();

                // Cubic Catmull-Rom interpolation — smooth curve through 4 control points
                let cr = |p0: (f32, f32),
                          p1: (f32, f32),
                          p2: (f32, f32),
                          p3: (f32, f32),
                          u: f32|
                 -> (f32, f32) {
                    let u2 = u * u;
                    let u3 = u2 * u;
                    let x = 0.5
                        * (2.0 * p1.0
                            + (-p0.0 + p2.0) * u
                            + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * u2
                            + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * u3);
                    let y = 0.5
                        * (2.0 * p1.1
                            + (-p0.1 + p2.1) * u
                            + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * u2
                            + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * u3);
                    (x, y)
                };

                // Three-pass volumetric glow: outer halo → mid body → bright core
                let passes = [
                    (3.5_f32, 0.22_f32),
                    (1.8_f32, 0.50_f32),
                    (0.85_f32, 0.95_f32),
                ];
                for &(width, alpha_mul) in passes.iter() {
                    for i in 1..SPEC_BANDS {
                        let amp0 = spectral.bands[i - 1];
                        let amp1 = spectral.bands[i];
                        if amp0 < 0.015 && amp1 < 0.015 {
                            continue;
                        }
                        let avg = (amp0 + amp1) * 0.5;
                        let p0 = pts[if i >= 2 { i - 2 } else { 0 }];
                        let p1 = pts[i - 1];
                        let p2 = pts[i];
                        let p3 = pts[(i + 1).min(SPEC_BANDS - 1)];
                        let f = i as f32 / SPEC_BANDS as f32;
                        let lr = (spec_r as f32 * (1.0 - f * 0.3)) as u8;
                        let lg = (spec_g as f32 * (1.0 - f * 0.15)) as u8;
                        let line_a = (avg * 70.0 * alpha_mul).min(90.0) as u8;
                        if line_a < 3 {
                            continue;
                        }
                        let mut prev = p1;
                        for s in 1..=8usize {
                            let u = s as f32 / 8.0;
                            let next = cr(p0, p1, p2, p3, u);
                            d.draw_line_ex(
                                rvec2(prev.0, prev.1),
                                rvec2(next.0, next.1),
                                width,
                                Color::new(lr, lg, spec_b, line_a),
                            );
                            prev = next;
                        }
                    }
                }

                // Luminous crown — faint echo 5 px above the main spline
                // Suggests overtone richness; classic high-end spectrum analyzer detail
                for i in 1..SPEC_BANDS {
                    let amp0 = spectral.bands[i - 1];
                    let amp1 = spectral.bands[i];
                    let avg = (amp0 + amp1) * 0.5;
                    if avg < 0.04 {
                        continue;
                    }
                    let crown = 5.0_f32;
                    let p0 = (
                        pts[if i >= 2 { i - 2 } else { 0 }].0,
                        pts[if i >= 2 { i - 2 } else { 0 }].1 - crown,
                    );
                    let p1 = (pts[i - 1].0, pts[i - 1].1 - crown);
                    let p2 = (pts[i].0, pts[i].1 - crown);
                    let p3 = (
                        pts[(i + 1).min(SPEC_BANDS - 1)].0,
                        pts[(i + 1).min(SPEC_BANDS - 1)].1 - crown,
                    );
                    let crown_a = (avg * 22.0).min(20.0) as u8;
                    if crown_a < 3 {
                        continue;
                    }
                    let cr_col = Color::new(
                        (spec_r as f32 * 1.3).min(255.0) as u8,
                        (spec_g as f32 * 1.1).min(255.0) as u8,
                        spec_b,
                        crown_a,
                    );
                    let mut prev = p1;
                    for s in 1..=4usize {
                        let u = s as f32 / 4.0;
                        let next = cr(p0, p1, p2, p3, u);
                        d.draw_line_ex(rvec2(prev.0, prev.1), rvec2(next.0, next.1), 0.7, cr_col);
                        prev = next;
                    }
                }
            }

            // Baseline — subtle reference line
            d.draw_line_ex(
                rvec2(margin, spec_y),
                rvec2(margin + total_w, spec_y),
                1.0,
                Color::new(80, 120, 160, 15),
            );
        }

        // ══════════════════════════════════════════════════════════
        //  5c. Thought pulse shockwaves
        // ══════════════════════════════════════════════════════════
        for pulse in &thought_pulses {
            if pulse.alpha < 0.02 {
                continue;
            }
            // Outer ring
            let ring_a = (pulse.alpha * 100.0) as u8;
            d.draw_circle_lines(
                pulse.x as i32,
                pulse.y as i32,
                pulse.radius,
                Color::new(icy.r, icy.g, icy.b, ring_a),
            );
            // Inner thin ring
            if pulse.radius > 20.0 {
                let inner_a = (pulse.alpha * 40.0) as u8;
                d.draw_circle_lines(
                    pulse.x as i32,
                    pulse.y as i32,
                    pulse.radius - 8.0,
                    Color::new(icy.r, icy.g, icy.b, inner_a),
                );
            }
        }

        // ══════════════════════════════════════════════════════════
        //  6. CRT scan overlay + film grain disabled
        //     (kept intentionally clean per UI noise reduction request)
        // ══════════════════════════════════════════════════════════

        // ── Lens drops: diegetic raindrops on the "camera glass" ──
        // Drawn AFTER CRT overlay so they feel on the physical surface
        if has_weather_data {
            weather_fx.draw_lens_drops(&mut d);
        }

        // ══════════════════════════════════════════════════════════
        //  6c. ATMOSPHERE POST — vignette, chromatic edges, brackets
        //      Drawn over CRT but UNDER the HUD so panels stay crisp.
        // ══════════════════════════════════════════════════════════
        {
            let alert_sev = alert_system.max_severity();
            let alert_kind = alert_system.current().map(|a| a.kind);
            let (ak_r, ak_g, ak_b) = match alert_kind {
                Some(AlertKind::CpuOverload) => (255u8, 70, 50),
                Some(AlertKind::MemPressure) => (255, 170, 70),
                Some(AlertKind::LoadSpike) => (255, 205, 95),
                Some(AlertKind::NetworkSurge) => (90, 220, 255),
                Some(AlertKind::EntropySpike) => (255, 110, 210),
                Some(AlertKind::MoodShift) => (150, 190, 255),
                Some(AlertKind::NerveImpulse) => (180, 255, 170),
                None => (120, 200, 240),
            };

            // Vignette strength scales with mood — Critical = darker corners
            let mut vig_strength = match mood {
                Mood::Serene => 0.30,
                Mood::Alert => 0.36,
                Mood::Stressed => 0.42,
                Mood::Critical => 0.50,
            };
            vig_strength = (vig_strength + alert_sev * 0.10).clamp(0.0, 0.62);
            draw_vignette(&mut d, w, h, mood, vig_strength);

            // Temperature tint — warm wash when hot, cool when cold.
            // cold_factor / warm_factor were derived earlier from weather_temp.
            if has_weather_data {
                if warm_factor > 0.02 {
                    let a = (warm_factor * 36.0).clamp(0.0, 36.0) as u8;
                    d.draw_rectangle(0, 0, w, h, Color::new(255, 150, 70, a));
                }
                if cold_factor > 0.02 {
                    let a = (cold_factor * 40.0).clamp(0.0, 40.0) as u8;
                    d.draw_rectangle(0, 0, w, h, Color::new(70, 130, 220, a));
                }
            }

            // Subtle chromatic edge aberration — sells the "lens" feel
            let chrom = (if spike { 0.35 } else { 0.20 }) + alert_sev * 0.18;
            draw_chromatic_edges(&mut d, w, h, chrom);

            // Cinematic corner brackets — minimal HUD frame ticks
            let bracket_inset = (h as f32 * 0.018) as i32;
            let bracket_arm = (h as f32 * 0.025).max(14.0) as i32;
            let bracket_color = Color::new(ak_r, ak_g, ak_b, (70.0 + alert_sev * 120.0) as u8);
            draw_corner_brackets(&mut d, w, h, bracket_color, bracket_inset, bracket_arm, 1);
        }

        // 6c-bis. Alert-driven mood corona — drawn BEFORE the SDF orb so
        //     the bioluminescent body shines on top instead of being washed
        //     out by the warm aura tint.
        {
            let alert_sev = alert_system.max_severity();
            if alert_sev > 0.08 {
                let alert_kind = alert_system.current().map(|a| a.kind);
                let (ar, ag, ab) = match alert_kind {
                    Some(AlertKind::CpuOverload) => (255u8, 70, 50),
                    Some(AlertKind::MemPressure) => (255, 170, 70),
                    Some(AlertKind::LoadSpike) => (255, 205, 95),
                    Some(AlertKind::NetworkSurge) => (90, 220, 255),
                    Some(AlertKind::EntropySpike) => (255, 110, 210),
                    Some(AlertKind::MoodShift) => (150, 190, 255),
                    Some(AlertKind::NerveImpulse) => (180, 255, 170),
                    None => (120, 200, 240),
                };
                let (ocx, ocy) = (ecs_orb_x as i32, ecs_orb_y as i32);
                let orb_scale = h as f32 / 480.0 * 0.6;
                let breath = (t * (0.8 + alert_sev * 2.2)).sin() * 0.5 + 0.5;
                let base_r = (78.0 + cpu * 0.32) * orb_scale;
                // Pushed further out + halved alpha so it hugs the silhouette
                // instead of overlapping it.
                let aura_r = base_r * (1.55 + alert_sev * 1.2) + breath * 20.0 * orb_scale;
                let aura_a = (16.0 + 60.0 * alert_sev).clamp(0.0, 90.0) as u8;
                d.draw_circle_gradient(
                    ocx,
                    ocy,
                    aura_r,
                    Color::new(ar, ag, ab, aura_a),
                    Color::new(ar, ag, ab, 0),
                );
            }
        }

        // 6d. Hero jellyfish body — replaces the old orb as the primary
        // subject. It is drawn directly with raylib primitives so the bell,
        // skirt, and tendrils remain visible even if the experimental SDF
        // shader path misbehaves.
        {
            let orb_scale = h as f32 / 480.0 * 0.6;
            let breath = (t * 0.3).sin() * 0.10 + 1.0;
            let radius = (76.0 + cpu * 0.28) * breath * orb_scale;
            draw_bioluminescent_jellyfish(&mut d, ecs_orb_x, ecs_orb_y, radius, t, cpu, mood);
        }

        // ══════════════════════════════════════════════════════════
        //  6d-WONDER. Wonder Drive: golden halo + shooting-star pulse.
        //  The halo is a soft gold corona around the orb whose radius and
        //  alpha grow with the wonder meter -- a quiet outward sigh.
        //  When the LLM loop fires a wonder pulse, a single shooting star
        //  streaks across the screen, born from the orb, arcing into the
        //  outer dark. Plays for ~1.4 seconds and self-extinguishes.
        // ══════════════════════════════════════════════════════════
        {
            // Detect a fresh pulse by edge-triggering on `last_wonder_pulse_at`.
            if wonder_pulse_at != 0 && wonder_pulse_at != wonder_last_seen_pulse_at {
                wonder_last_seen_pulse_at = wonder_pulse_at;
                wonder_streak_t = 0.0;
                // Deterministic per-pulse seed so the trajectory is consistent
                // for a given pulse but different across pulses.
                wonder_streak_seed = (wonder_pulse_at as u32)
                    .wrapping_mul(2654435761)
                    .wrapping_add(0x9E3779B1);
            }

            // Halo: soft golden corona, radius grows with wonder. Use the
            // same orb-scale logic as the body so it tracks resolution.
            if wonder > 0.05 {
                let orb_scale = h as f32 / 480.0 * 0.6;
                let breath = (t * 0.3).sin() * 0.15 + 1.0;
                let base_r = (45.0 + cpu * 0.48) * breath * orb_scale;
                // Halo radius scales 1.6x..2.6x the body as wonder grows.
                let halo_r = base_r * (1.6 + wonder * 1.0);
                // Subtle slow shimmer so the halo breathes independently
                // of the orb body -- feels like a different organ.
                let shimmer = 0.85 + (t * 0.7).sin() * 0.15;
                let alpha = (wonder.powf(0.85) * 110.0 * shimmer).clamp(0.0, 200.0) as u8;
                // Warm amber gold -- distinct from the teal orb body so the
                // viewer reads it as "different signal", not just bloom.
                let gold_inner = Color::new(255, 210, 110, alpha);
                let gold_outer = Color::new(255, 160, 60, 0);
                let (orb_cx, orb_cy) = (ecs_orb_x as i32, ecs_orb_y as i32);
                d.draw_circle_gradient(orb_cx, orb_cy, halo_r, gold_inner, gold_outer);
                // A second wider, fainter ring at peak wonder for the ache.
                if wonder > 0.7 {
                    let ring_alpha = ((wonder - 0.7) * 3.3 * 60.0).clamp(0.0, 60.0) as u8;
                    d.draw_circle_gradient(
                        orb_cx,
                        orb_cy,
                        halo_r * 1.55,
                        Color::new(255, 230, 170, ring_alpha),
                        Color::new(255, 200, 120, 0),
                    );
                }
            }

            // Shooting-star pulse: short streak born from the orb, arcing
            // out into the dark. Uses additive bright color and tapered
            // alpha so it reads as "a thought catching on air".
            if !wonder_streak_t.is_nan() {
                wonder_streak_t += dt;
                const STREAK_LIFE: f32 = 1.4;
                if wonder_streak_t >= STREAK_LIFE {
                    wonder_streak_t = f32::NAN;
                } else {
                    let p = wonder_streak_t / STREAK_LIFE; // 0..1
                                                           // Direction from per-pulse seed -- random angle, but biased
                                                           // upward (the romantic direction).
                    let ang_seed = (wonder_streak_seed >> 8) as f32 / 16_777_215.0;
                    let angle = -std::f32::consts::PI * (0.15 + 0.70 * ang_seed);
                    let (sx, sy) = (ecs_orb_x, ecs_orb_y);
                    let max_dist = (w.min(h) as f32) * 0.85;
                    // Ease-out so the streak shoots fast and trails slow.
                    let ep = 1.0 - (1.0 - p).powi(3);
                    let head_d = ep * max_dist;
                    let head_x = sx + angle.cos() * head_d;
                    let head_y = sy + angle.sin() * head_d;
                    // Tail length grows then shrinks.
                    let tail_len = max_dist * 0.18 * (1.0 - (p - 0.5).abs() * 1.6).max(0.15);
                    let tail_x = head_x - angle.cos() * tail_len;
                    let tail_y = head_y - angle.sin() * tail_len;
                    // Alpha eases out toward end of life.
                    let life_alpha = ((1.0 - p).powf(0.6) * 255.0) as u8;
                    let head_col = Color::new(255, 250, 220, life_alpha);
                    let tail_col = Color::new(255, 200, 120, (life_alpha as u16 * 60 / 255) as u8);
                    // Draw the trailing streak as a thick line + a bright
                    // head circle. Multi-pass for soft glow.
                    let head = rvec2(head_x, head_y);
                    let tail = rvec2(tail_x, tail_y);
                    d.draw_line_ex(tail, head, 2.0, tail_col);
                    d.draw_line_ex(tail, head, 1.0, head_col);
                    // Head bloom
                    let head_r = 4.0 + (1.0 - p) * 6.0;
                    d.draw_circle_gradient(
                        head_x as i32,
                        head_y as i32,
                        head_r * 3.0,
                        Color::new(255, 240, 200, life_alpha / 3),
                        Color::new(255, 200, 120, 0),
                    );
                    d.draw_circle_v(head, head_r, head_col);
                }
            }
        }

        if spike {
            let orb_scale = h as f32 / 480.0 * 0.6;
            let g = ((t * 35.0).sin() * 6.0 * orb_scale) as i32;
            let a = ((t * 25.0).sin().abs() * 140.0) as u8;
            let breath = (t * 0.3).sin() * 0.15 + 1.0;
            let base = (45.0 + cpu * 0.48) * breath * orb_scale;
            let p1 = (t * 0.85).sin() * 14.0 * orb_scale;
            d.draw_circle_gradient(
                orb_x + g,
                orb_y,
                base + p1,
                Color::new(255, 30, 30, a / 2),
                Color::new(0, 0, 0, 0),
            );
            d.draw_circle_gradient(
                orb_x - g,
                orb_y,
                base + p1,
                Color::new(30, 30, 255, a / 2),
                Color::new(0, 0, 0, 0),
            );
            for i in 0..8 {
                let y = ((t * 11.0 + i as f32 * 53.0) % h as f32) as i32;
                d.draw_rectangle(0, y, w, 2, Color::new(255, 50, 70, 30));
            }
            d.draw_rectangle(
                0,
                ((t * 7.3).sin() * 80.0 * orb_scale + orb_y as f32) as i32,
                w,
                1,
                Color::new(255, 255, 255, 25),
            );
        }

        // Final star overlay: draw stars on top of atmosphere/vignette
        // Stars hidden by user preference; keep this call in place in case
        // we re-enable later via `star_alpha`.
        starfield.draw(&mut d, t, mood, star_alpha);

        // ══════════════════════════════════════════════════════════
        //  6b. WRITE MODE / SHADOW MODE — tool execution overlay
        // ══════════════════════════════════════════════════════════
        // State machine: dequeue events, type command, show result, fade out
        // Shadow Mode: Tor-based tools get violet/amber aesthetic
        if wm_phase == 0 {
            if let Some(evt) = wm_queue.pop_front() {
                wm_tool_name = evt.tool_name;
                wm_command = evt.command;
                wm_result = evt.result;
                wm_success = evt.success;
                wm_char_idx = 0;
                wm_timer = 0.0;
                wm_phase = 1;
            }
        }
        if wm_phase > 0 {
            // Three modes:
            //   - Shadow Mode (Tor):           violet/amber, "SUBTERRANEAN PROTOCOL"
            //   - Observe Mode (read-only):    cyan/teal,    "OBSERVE MODE"
            //   - Write Mode   (state-change): green,         "WRITE MODE"
            let is_shadow = matches!(
                wm_tool_name.as_str(),
                "onion_probe"
                    | "anon_search"
                    | "tor_health"
                    | "fetch_clearnet"
                    | "dark_web_news"
                    | "dark_web_dig"
            );
            let is_observe = matches!(
                wm_tool_name.as_str(),
                "probe_system"
                    | "read_logs"
                    | "scan_network"
                    | "write_journal"
                    | "check_ports"
                    | "inspect_self"
            );
            let is_reverie = matches!(
                wm_tool_name.as_str(),
                "dream_sequence"
                    | "visualize_thought"
                    | "run_python_sketch"
                    | "set_focus"
                    | "recall_journal"
            );

            let wm_alpha = match wm_phase {
                1 => 1.0_f32,
                2 => 1.0,
                3 => (wm_timer / 2.0).clamp(0.0, 1.0),
                _ => 0.0,
            };

            if wm_alpha > 0.01 {
                let bar_h = (h as f32 * 0.13) as i32;
                // ── Mood + entropy micro-jitter on the overlay container ──
                // Applied to bar_y only so all child elements (text, scanlines,
                // borders) ride together — digits stay readable.
                let (jit_amp, jit_freq) = match mood {
                    Mood::Serene => (0.0_f32, 0.0_f32),
                    Mood::Alert => (0.6, 18.0),
                    Mood::Stressed => (1.4, 32.0),
                    Mood::Critical => (2.6, 55.0),
                };
                let entropy_spike = (entropy - 0.6).max(0.0) * 4.0;
                let jit = jit_amp + entropy_spike;
                let jy = (((t_f32 * jit_freq).sin() + (t_f32 * jit_freq * 0.9 + 2.1).cos() * 0.6)
                    * jit) as i32;
                let bar_y = (h as f32 * 0.40) as i32 + jy;
                let a = (wm_alpha * 210.0) as u8;
                let a_half = (wm_alpha * 110.0) as u8;
                let a_dim = (wm_alpha * 60.0) as u8;

                // Color scheme tuned to mode
                let (bg_r, bg_g, bg_b) = if is_shadow {
                    (6, 0, 8)
                } else if is_reverie {
                    (4, 0, 12)
                } else if is_observe {
                    (0, 4, 8)
                } else {
                    (0, 6, 0)
                };
                let (border_r, border_g, border_b) = if is_shadow {
                    (140, 60, 200)
                } else if is_reverie {
                    (170, 90, 240)
                } else if is_observe {
                    (60, 180, 230)
                } else {
                    (0, 255, 80)
                };
                let (text_r, text_g, text_b) = if is_shadow {
                    (180, 120, 255)
                } else if is_reverie {
                    (210, 170, 255)
                } else if is_observe {
                    (110, 220, 255)
                } else {
                    (0, 255, 80)
                };
                let (hdr_r, hdr_g, hdr_b) = if is_shadow {
                    (200, 150, 40)
                } else if is_reverie {
                    (220, 200, 255)
                } else if is_observe {
                    (180, 230, 255)
                } else {
                    (255, 180, 40)
                };
                let (tag_r, tag_g, tag_b) = if is_shadow {
                    (180, 80, 255)
                } else if is_reverie {
                    (200, 140, 255)
                } else if is_observe {
                    (140, 240, 255)
                } else {
                    (0, 255, 120)
                };

                // Dark background strip
                d.draw_rectangle(0, bar_y, w, bar_h, Color::new(bg_r, bg_g, bg_b, a));
                // Top/bottom border glow
                d.draw_rectangle(
                    0,
                    bar_y,
                    w,
                    2,
                    Color::new(border_r, border_g, border_b, a_half),
                );
                d.draw_rectangle(
                    0,
                    bar_y + bar_h - 2,
                    w,
                    2,
                    Color::new(border_r, border_g, border_b, a_half),
                );
                // Header: >> WRITE MODE / OBSERVE MODE / SUBTERRANEAN PROTOCOL
                let hdr_y = bar_y + 8;
                if is_shadow {
                    d.draw_text(
                        ">> SUBTERRANEAN PROTOCOL ACTIVE",
                        20,
                        hdr_y,
                        16,
                        Color::new(hdr_r, hdr_g, hdr_b, a),
                    );
                    let tag = format!("[{}]", wm_tool_name.to_uppercase());
                    d.draw_text(&tag, 370, hdr_y, 14, Color::new(tag_r, tag_g, tag_b, a));
                } else if is_reverie {
                    let label = if wm_tool_name == "dream_sequence" {
                        ">> REVERIE :: DREAM SEQUENCE"
                    } else {
                        ">> REVERIE"
                    };
                    d.draw_text(label, 20, hdr_y, 16, Color::new(hdr_r, hdr_g, hdr_b, a));
                    let tag = format!("[{}]", wm_tool_name.to_uppercase());
                    let tag_x = if wm_tool_name == "dream_sequence" {
                        320
                    } else {
                        150
                    };
                    d.draw_text(&tag, tag_x, hdr_y, 14, Color::new(tag_r, tag_g, tag_b, a));
                } else if is_observe {
                    d.draw_text(
                        ">> OBSERVE MODE",
                        20,
                        hdr_y,
                        16,
                        Color::new(hdr_r, hdr_g, hdr_b, a),
                    );
                    let tag = format!("[{}]", wm_tool_name.to_uppercase());
                    d.draw_text(&tag, 200, hdr_y, 14, Color::new(tag_r, tag_g, tag_b, a));
                } else {
                    d.draw_text(
                        ">> WRITE MODE",
                        20,
                        hdr_y,
                        16,
                        Color::new(hdr_r, hdr_g, hdr_b, a),
                    );
                    let tag = format!("[{}]", wm_tool_name.to_uppercase());
                    d.draw_text(&tag, 180, hdr_y, 14, Color::new(tag_r, tag_g, tag_b, a));
                }

                // Command line — types out character by character
                let cmd_y = bar_y + 32;
                let visible = &wm_command[..wm_char_idx.min(wm_command.len())];
                let cursor_on = (grain_seed / 15) % 2 == 0;
                let cursor = if wm_phase == 1 && cursor_on { "_" } else { " " };
                d.draw_text(
                    &format!("$ {}{}", visible, cursor),
                    20,
                    cmd_y,
                    14,
                    Color::new(text_r, text_g, text_b, a),
                );

                // Shadow Mode: "Tunneling..." progress bar with jitter
                if is_shadow && wm_phase == 1 {
                    let tunnel_y = cmd_y + 20;
                    let tunnel_w_max = (w as f32 * 0.4) as i32;
                    let progress = (wm_char_idx as f32) / (wm_command.len().max(1) as f32);
                    // Jitter to simulate Tor hop latency
                    let jitter = ((t_f32 * 7.3).sin() * 0.08 + (t_f32 * 13.1).cos() * 0.05) as f32;
                    let bar_fill =
                        ((progress + jitter).clamp(0.0, 1.0) * tunnel_w_max as f32) as i32;
                    // Background track
                    d.draw_rectangle(20, tunnel_y, tunnel_w_max, 6, Color::new(30, 10, 40, a_dim));
                    // Fill with violet gradient
                    d.draw_rectangle(20, tunnel_y, bar_fill, 6, Color::new(140, 60, 200, a_half));
                    // Label
                    d.draw_text(
                        "Tunneling...",
                        20 + tunnel_w_max + 8,
                        tunnel_y - 2,
                        10,
                        Color::new(120, 80, 180, a_half),
                    );
                }

                // Result line (phases 2+)
                if wm_phase >= 2 && !wm_result.is_empty() {
                    let res_y = if is_shadow { cmd_y + 30 } else { cmd_y + 22 };
                    let res_color = if wm_success {
                        Color::new(text_r, text_g, text_b, a)
                    } else {
                        Color::new(255, 80, 60, a)
                    };
                    let status = if wm_success { "OK" } else { "ERR" };
                    let truncated: String = wm_result.chars().take(100).collect();
                    d.draw_text(
                        &format!("[{}] {}", status, truncated),
                        20,
                        res_y,
                        12,
                        res_color,
                    );
                }

                // Outer glow pulse on the borders
                let pulse_a = ((wm_alpha * 40.0) * (1.0 + (t_f32 * 3.0).sin() * 0.3)) as u8;
                d.draw_rectangle(
                    0,
                    bar_y - 2,
                    w,
                    2,
                    Color::new(border_r, border_g, border_b, pulse_a),
                );
                d.draw_rectangle(
                    0,
                    bar_y + bar_h,
                    w,
                    2,
                    Color::new(border_r, border_g, border_b, pulse_a),
                );
            }

            // Advance write mode state machine
            match wm_phase {
                1 => {
                    // Type ~2 chars per frame for snappy typing
                    wm_char_idx += 2;
                    if wm_char_idx >= wm_command.len() {
                        wm_char_idx = wm_command.len();
                        wm_phase = 2;
                        wm_timer = 3.5; // hold result for 3.5s
                    }
                }
                2 => {
                    wm_timer -= dt;
                    if wm_timer <= 0.0 {
                        wm_phase = 3;
                        wm_timer = 2.0; // fade over 2s
                    }
                }
                3 => {
                    wm_timer -= dt;
                    if wm_timer <= 0.0 {
                        wm_phase = 0;
                    }
                }
                _ => {
                    wm_phase = 0;
                }
            }
        }

        // ══════════════════════════════════════════════════════════
        //  7. MODERN HUD — Arc Meters + Alert Panel + Uptime
        //  Design language: minimal, low-opacity glass panels, unified
        //  rounding (0.16), generous outer padding, integrated chips
        //  rather than dangling badges. Beauty first, data second.
        // ══════════════════════════════════════════════════════════
        let wf = w as f32;
        let hf = h as f32;
        let ui_scale = (hf / 480.0).max(1.0);
        let pad = (16.0 * ui_scale) as i32;
        let _font_lg = (18.0 * ui_scale) as i32;
        let font_md = (14.0 * ui_scale) as i32;
        let font_sm = (11.0 * ui_scale) as i32;
        let font_xs = (9.0 * ui_scale) as i32;

        // Near-future palette — unified glass styling
        let col_cyan = Color::new(80, 220, 255, 255);
        let col_purple = Color::new(160, 110, 240, 255);
        let col_green = Color::new(80, 210, 160, 255);
        let col_red = Color::new(255, 80, 70, 255);
        let col_white = Color::new(240, 245, 255, 245);
        let col_white_dim = Color::new(180, 195, 215, 120);
        let col_panel_bg = Color::new(10, 16, 26, 130); // softer, more transparent
        let col_panel_edge = Color::new(80, 180, 255, 28); // gentler hairline
        let col_track = Color::new(30, 42, 60, 110);
        let col_divider = Color::new(80, 140, 200, 22); // subtle section separator
                                                        // Unified panel geometry — all top-level panels share these.
        let panel_round = 0.16_f32;
        let panel_segs = 10_i32;

        // Toggle the Top-Left meter+weather+mood panel and the Bottom-Right
        // NERVE ACTIVITY monitor. The Top-Left panel carries genuinely useful
        // at-a-glance telemetry (CPU%, MEM%, ambient temperature + weather);
        // the nerve monitor is informational and competes with the AURA scene,
        // so it is hidden by default.
        const SHOW_TOP_LEFT_PANEL: bool = true;
        const SHOW_NERVE_PANEL: bool = false;
        const SHOW_ALERT_PANEL: bool = true;

        // ── Top-Left: CPU + MEM Arc Meters + Weather + integrated mood chip ──
        let (mp_x, mp_y, mp_w, mp_h) = if !SHOW_TOP_LEFT_PANEL {
            // Hidden: report a zero-width stub anchored at the top-left pad so
            // downstream layout (alert-panel slot calc) gives the centre band
            // the full available width.
            (pad as f32, pad as f32, 0.0_f32, 0.0_f32)
        } else {
            let has_weather = weather_temp.is_some();
            let panel_w = if has_weather {
                280.0 * ui_scale
            } else {
                190.0 * ui_scale
            };
            let panel_h = if has_weather {
                124.0 * ui_scale
            } else {
                114.0 * ui_scale
            };
            let mp_x = pad as f32;
            let mp_y = pad as f32;

            // Panel background
            d.draw_rectangle_rounded(
                rrect(mp_x, mp_y, panel_w, panel_h),
                panel_round,
                panel_segs,
                col_panel_bg,
            );
            d.draw_rectangle_rounded_lines_ex(
                rrect(mp_x, mp_y, panel_w, panel_h),
                panel_round,
                panel_segs,
                1.0,
                col_panel_edge,
            );

            let arc_r_outer = 30.0 * ui_scale;
            let arc_r_inner = 22.0 * ui_scale;
            let arc_start = 135.0_f32;
            let arc_sweep = 270.0_f32;
            let num_segs: i32 = 20;
            let seg_gap = 2.5_f32;
            let seg_sweep = (arc_sweep / num_segs as f32) - seg_gap;

            // --- CPU arc ---
            let cpu_cx = mp_x + panel_w * (if has_weather { 0.19 } else { 0.28 });
            let cpu_cy = mp_y + panel_h * 0.44;

            // Track
            d.draw_ring(
                rvec2(cpu_cx, cpu_cy),
                arc_r_inner,
                arc_r_outer,
                arc_start,
                arc_start + arc_sweep,
                36,
                col_track,
            );

            // Active segments — cyan-to-white gradient
            let cpu_fill_end = arc_start + arc_sweep * (cpu / 100.0);
            for i in 0..num_segs {
                let sa = arc_start + i as f32 * (seg_sweep + seg_gap);
                if sa >= cpu_fill_end {
                    break;
                }
                let ea = (sa + seg_sweep).min(cpu_fill_end);
                let frac = i as f32 / (num_segs - 1) as f32;
                // Cyan (80,220,255) → crisp white (235,250,255)
                let r = (80.0 + 155.0 * frac) as u8;
                let g = (220.0 + 30.0 * frac) as u8;
                d.draw_ring(
                    rvec2(cpu_cx, cpu_cy),
                    arc_r_inner + 1.0,
                    arc_r_outer - 1.0,
                    sa,
                    ea,
                    6,
                    Color::new(r, g, 255, 235),
                );
            }

            // Center value
            let cpu_str = format!("{:.0}%", cpu);
            let cpu_w = d.measure_text(&cpu_str, font_md);
            d.draw_text(
                &cpu_str,
                (cpu_cx - cpu_w as f32 / 2.0) as i32,
                (cpu_cy - font_md as f32 * 0.4) as i32,
                font_md,
                col_white,
            );

            // Labels below
            let cpu_status = if cpu > 75.0 {
                "CRITICAL"
            } else if cpu > 55.0 {
                "ELEVATED"
            } else if cpu > 25.0 {
                "ACTIVE"
            } else {
                "STABLE"
            };
            d.draw_text(
                "CPU",
                (cpu_cx - d.measure_text("CPU", font_xs) as f32 / 2.0) as i32,
                (cpu_cy + arc_r_outer + 3.0 * ui_scale) as i32,
                font_xs,
                col_white_dim,
            );
            let status_col = if cpu > 55.0 {
                Color::new(255, 190, 80, 210)
            } else {
                Color::new(80, 210, 180, 210)
            };
            d.draw_text(
                cpu_status,
                (cpu_cx - d.measure_text(cpu_status, font_xs) as f32 / 2.0) as i32,
                (cpu_cy + arc_r_outer + 3.0 * ui_scale + font_xs as f32 * 1.15) as i32,
                font_xs,
                status_col,
            );

            // --- MEM arc ---
            let mem_cx = mp_x + panel_w * (if has_weather { 0.50 } else { 0.72 });
            let mem_cy = cpu_cy;

            // Track
            d.draw_ring(
                rvec2(mem_cx, mem_cy),
                arc_r_inner,
                arc_r_outer,
                arc_start,
                arc_start + arc_sweep,
                36,
                col_track,
            );

            // Active segments — amethyst-to-white gradient
            let mem_fill_end = arc_start + arc_sweep * (mem / 100.0);
            for i in 0..num_segs {
                let sa = arc_start + i as f32 * (seg_sweep + seg_gap);
                if sa >= mem_fill_end {
                    break;
                }
                let ea = (sa + seg_sweep).min(mem_fill_end);
                let frac = i as f32 / (num_segs - 1) as f32;
                // Amethyst (160,110,240) → crisp white (235,235,255)
                let r = (160.0 + 75.0 * frac) as u8;
                let g = (110.0 + 125.0 * frac) as u8;
                let b = (240.0 + 15.0 * frac) as u8;
                d.draw_ring(
                    rvec2(mem_cx, mem_cy),
                    arc_r_inner + 1.0,
                    arc_r_outer - 1.0,
                    sa,
                    ea,
                    6,
                    Color::new(r, g, b, 235),
                );
            }

            // Center value
            let mem_str = format!("{:.0}%", mem);
            let mem_w = d.measure_text(&mem_str, font_md);
            d.draw_text(
                &mem_str,
                (mem_cx - mem_w as f32 / 2.0) as i32,
                (mem_cy - font_md as f32 * 0.4) as i32,
                font_md,
                col_white,
            );

            // Labels below
            let mem_status = if mem > 80.0 {
                "HIGH"
            } else if mem > 50.0 {
                "MODERATE"
            } else {
                "OPTIMAL"
            };
            d.draw_text(
                "MEM",
                (mem_cx - d.measure_text("MEM", font_xs) as f32 / 2.0) as i32,
                (mem_cy + arc_r_outer + 3.0 * ui_scale) as i32,
                font_xs,
                col_white_dim,
            );
            let mstatus_col = if mem > 50.0 {
                Color::new(255, 190, 80, 210)
            } else {
                Color::new(80, 210, 180, 210)
            };
            d.draw_text(
                mem_status,
                (mem_cx - d.measure_text(mem_status, font_xs) as f32 / 2.0) as i32,
                (mem_cy + arc_r_outer + 3.0 * ui_scale + font_xs as f32 * 1.15) as i32,
                font_xs,
                mstatus_col,
            );

            // --- WEATHER (right of MEM arc, if available) ---
            if let Some(temp) = weather_temp {
                let therm_cx = mp_x + panel_w * 0.82;

                // Thermometer geometry — leave room above the tube for the
                // numeric temperature readout so it stays inside the panel.
                let tube_w = 8.0 * ui_scale;
                let tube_h = 54.0 * ui_scale;
                let bulb_r = 8.0 * ui_scale;
                let tube_top = mp_y + 22.0 * ui_scale; // pushed down to give the temp value clear space
                let tube_bot = tube_top + tube_h;
                let bulb_cy = tube_bot + bulb_r * 0.4;

                // Temperature → fill fraction (-30C = 0%, 45C = 100%)
                let temp_frac = ((temp + 30.0) / 75.0).clamp(0.0, 1.0);

                // Mercury color: blue → cyan → teal → amber → red based on temp
                let merc_col = if temp < -10.0 {
                    Color::new(80, 140, 255, 240)
                } else if temp < 5.0 {
                    Color::new(80, 200, 255, 240)
                } else if temp < 20.0 {
                    Color::new(80, 220, 180, 240)
                } else if temp < 30.0 {
                    Color::new(255, 200, 80, 240)
                } else {
                    Color::new(255, 90, 70, 240)
                };

                // Outer tube (glass)
                let tube_x = therm_cx - tube_w / 2.0;
                d.draw_rectangle_rounded(
                    rrect(tube_x - 1.5, tube_top - 1.5, tube_w + 3.0, tube_h + 3.0),
                    0.4,
                    6,
                    Color::new(60, 80, 110, 100),
                );
                // Inner tube background
                d.draw_rectangle_rounded(
                    rrect(tube_x, tube_top, tube_w, tube_h),
                    0.35,
                    6,
                    Color::new(20, 28, 45, 180),
                );

                // Mercury fill (rises from bottom)
                let fill_h = tube_h * temp_frac;
                let fill_top = tube_bot - fill_h;
                if fill_h > 2.0 {
                    d.draw_rectangle_rounded(
                        rrect(tube_x + 1.0, fill_top, tube_w - 2.0, fill_h),
                        0.2,
                        4,
                        merc_col,
                    );
                    // Glow highlight on mercury
                    d.draw_rectangle_rounded(
                        rrect(
                            tube_x + 2.0,
                            fill_top,
                            (tube_w - 4.0) * 0.4,
                            fill_h.min(tube_h * 0.5),
                        ),
                        0.2,
                        4,
                        Color::new(255, 255, 255, 30),
                    );
                }

                // Bulb (circle at bottom)
                d.draw_circle_v(
                    rvec2(therm_cx, bulb_cy),
                    bulb_r + 2.0,
                    Color::new(60, 80, 110, 100),
                );
                d.draw_circle_v(rvec2(therm_cx, bulb_cy), bulb_r, merc_col);
                d.draw_circle_v(
                    rvec2(therm_cx - bulb_r * 0.25, bulb_cy - bulb_r * 0.25),
                    bulb_r * 0.35,
                    Color::new(255, 255, 255, 40),
                );

                // Tick marks (-15, 0, 15, 30)
                let ticks: &[(f32, &str)] =
                    &[(-15.0, "-15"), (0.0, "0"), (15.0, "15"), (30.0, "30")];
                for &(tick_temp, label) in ticks {
                    let tick_frac = ((tick_temp + 30.0) / 75.0).clamp(0.0, 1.0);
                    let tick_y = tube_bot - tube_h * tick_frac;
                    if tick_y >= tube_top && tick_y <= tube_bot {
                        let tick_x1 = tube_x + tube_w + 2.0;
                        let tick_x2 = tick_x1 + 4.0 * ui_scale;
                        d.draw_line_ex(
                            rvec2(tick_x1, tick_y),
                            rvec2(tick_x2, tick_y),
                            1.0,
                            Color::new(140, 160, 190, 100),
                        );
                        d.draw_text(
                            label,
                            (tick_x2 + 2.0) as i32,
                            (tick_y - font_xs as f32 * 0.4) as i32,
                            font_xs,
                            Color::new(140, 160, 190, 90),
                        );
                    }
                }

                // Temperature value above tube — anchored just inside the
                // panel's top edge so it never escapes into the alert zone.
                let wx_str = format!("{}C", temp as i32);
                let temp_text_col = if temp > 35.0 {
                    col_red
                } else if temp > 30.0 {
                    Color::new(255, 200, 90, 235)
                } else if temp < 0.0 {
                    Color::new(100, 160, 255, 235)
                } else {
                    col_cyan
                };
                let temp_y = (mp_y + 5.0 * ui_scale).max(tube_top - font_md as f32 - 2.0);
                d.draw_text(
                    &wx_str,
                    (therm_cx - d.measure_text(&wx_str, font_md) as f32 / 2.0) as i32,
                    temp_y as i32,
                    font_md,
                    temp_text_col,
                );

                // Label below bulb
                let sky_label = if weather_desc.is_empty() {
                    "ENV"
                } else {
                    &weather_desc
                };
                d.draw_text(
                    sky_label,
                    (therm_cx - d.measure_text(sky_label, font_xs) as f32 / 2.0) as i32,
                    (bulb_cy + bulb_r + 4.0 * ui_scale) as i32,
                    font_xs,
                    col_white_dim,
                );
                if !weather_location.is_empty() {
                    let loc_short: String = weather_location.chars().take(14).collect();
                    d.draw_text(
                        &loc_short,
                        (therm_cx - d.measure_text(&loc_short, font_xs) as f32 / 2.0) as i32,
                        (bulb_cy + bulb_r + 4.0 * ui_scale + font_xs as f32 * 1.15) as i32,
                        font_xs,
                        Color::new(140, 160, 190, 180),
                    );
                }
            }
            (mp_x, mp_y, panel_w, panel_h)
        };

        // ── Mood chip — a small pill anchored just below the meter panel,
        //    flush with its left edge. Lives in its own row so nothing
        //    collides with the thermometer or alert panel above.
        if SHOW_TOP_LEFT_PANEL {
            let mood_col = match mood {
                Mood::Serene => Color::new(80, 210, 190, 230),
                Mood::Alert => Color::new(80, 220, 255, 230),
                Mood::Stressed => Color::new(255, 200, 90, 240),
                Mood::Critical => Color::new(255, 90, 70, 255),
            };
            let mood_label = mood.label();
            let mlw = d.measure_text(mood_label, font_xs) as f32;
            let chip_pad_x = 8.0 * ui_scale;
            let chip_pad_y = 3.0 * ui_scale;
            let dot_r = 2.5 * ui_scale;
            let chip_w = mlw + chip_pad_x * 2.0 + dot_r * 2.4;
            let chip_h = font_xs as f32 + chip_pad_y * 2.0;
            // Below the meter panel, flush-left, with a small gap.
            let chip_x = mp_x;
            let chip_y = mp_y + mp_h + 6.0 * ui_scale;
            let dot_pulse = ((t * 1.8).sin() * 0.4 + 0.6).clamp(0.2, 1.0);
            d.draw_rectangle_rounded(
                rrect(chip_x, chip_y, chip_w, chip_h),
                0.5,
                6,
                Color::new(mood_col.r, mood_col.g, mood_col.b, 28),
            );
            d.draw_rectangle_rounded_lines_ex(
                rrect(chip_x, chip_y, chip_w, chip_h),
                0.5,
                6,
                1.0,
                Color::new(mood_col.r, mood_col.g, mood_col.b, 80),
            );
            d.draw_circle(
                (chip_x + chip_pad_x * 0.7) as i32,
                (chip_y + chip_h * 0.5) as i32,
                dot_r,
                Color::new(
                    mood_col.r,
                    mood_col.g,
                    mood_col.b,
                    (180.0 * dot_pulse) as u8,
                ),
            );
            d.draw_text(
                mood_label,
                (chip_x + chip_pad_x + dot_r * 1.6) as i32,
                (chip_y + chip_pad_y) as i32,
                font_xs,
                mood_col,
            );
        }

        // ── Top-Center: Mood Alert Panel ──
        //   World-class treatment: every alert lands like a fresh transmission.
        //   Per-alert seed drives a decode-scramble intro, the spawn flash and
        //   jolt give the arrival physical weight, the EKG waveform breathes
        //   with severity, and a flavor-pool sub-text means the same condition
        //   never reads the same way twice. Spontaneity is the design.
        if SHOW_ALERT_PANEL {
            use crate::ui::alert::{decode_scramble, nominal_whisper};

            // Layout — clamp into the slot between the meter panel (left) and
            // the clock (right) so the panel never overlaps its neighbours.
            let tr_w_est = 152.0 * ui_scale;
            let gutter = 24.0 * ui_scale;
            let left_edge_min = mp_x + mp_w + gutter;
            let right_edge_max = wf - tr_w_est - pad as f32 - gutter;
            let avail = (right_edge_max - left_edge_min).max(200.0);
            let alert_w = (360.0 * ui_scale).min(avail);
            let alert_h = 70.0 * ui_scale;
            let slot_center = (left_edge_min + right_edge_max) / 2.0;
            let alert_x = slot_center - alert_w / 2.0;
            let alert_y_base = pad as f32 + 4.0 * ui_scale;

            // ── Snapshot the current alert so we can drop the borrow before
            //    we use any other field of `alert_system` later in the block.
            let severity = alert_system.max_severity();
            let alert_count = alert_system.alerts.len();
            let alert_idx_disp = if alert_count > 0 {
                alert_system.display_idx + 1
            } else {
                0
            };
            let nominal_clock = alert_system.clock;
            let (
                has_alert,
                alert_kind,
                alert_age,
                alert_seed,
                alert_message,
                alert_sub,
                alert_life_frac,
            ) = if let Some(a) = alert_system.current() {
                (
                    true,
                    Some(a.kind),
                    a.age,
                    a.seed,
                    a.message.clone(),
                    a.sub_text.clone(),
                    a.life_frac(),
                )
            } else {
                (false, None, 99.0, 0u32, String::new(), String::new(), 0.0)
            };

            // Spawn envelope (1.0 → 0.0 over first 1.0s of the alert).
            let spawn_env = (1.0 - alert_age).clamp(0.0, 1.0);
            // Sharper jolt on arrival, decays in ~0.25s.
            let jolt_env = (1.0 - alert_age * 4.0).clamp(0.0, 1.0);
            let jolt_y = if has_alert {
                ((t * 55.0).sin() * 1.6 + (t * 31.0).cos() * 1.0)
                    * jolt_env
                    * (0.4 + severity * 0.6)
            } else {
                0.0
            };
            let alert_y = alert_y_base + jolt_y;

            let glow_speed = if severity > 0.7 {
                3.5
            } else if severity > 0.3 {
                2.5
            } else {
                1.5
            };
            let glow_pulse = ((t * glow_speed).sin() * 0.35 + 0.65).clamp(0.3, 1.0);

            // Kind-aware palette — mood alerts read distinct from hardware.
            let (panel_r, panel_g, panel_b) = if let Some(kind) = alert_kind {
                match kind {
                    AlertKind::CpuOverload => (255u8, 70, 50),
                    AlertKind::MemPressure => (255, 170, 70),
                    AlertKind::LoadSpike => (255, 205, 95),
                    AlertKind::NetworkSurge => (90, 220, 255),
                    AlertKind::EntropySpike => (255, 110, 210),
                    AlertKind::MoodShift => (150, 190, 255),
                    AlertKind::NerveImpulse => (180, 255, 170),
                }
            } else {
                (60, 180, 140)
            };

            // ── Outer aura (severity-driven bloom around the panel) ──
            // Multi-layer halo glow for premium depth.
            use crate::ui::alert::build_glow_halos;
            let halos = build_glow_halos(
                alert_x + alert_w / 2.0,
                alert_y + alert_h / 2.0,
                (alert_w / 2.0).max(alert_h / 2.0) + 8.0,
                (panel_r, panel_g, panel_b),
                severity,
                t,
            );
            for (i, (r, a)) in halos.iter().enumerate() {
                let offset = (*r - (alert_w / 2.0).max(alert_h / 2.0) - 8.0) / 4.0;
                let inset = 6.0 + offset * (i as f32 + 1.0);
                let blend_a = ((*a as f32) * (1.0 - i as f32 * 0.25)) as u8;
                d.draw_rectangle_rounded(
                    rrect(alert_x - inset, alert_y - inset, alert_w + inset * 2.0, alert_h + inset * 2.0),
                    panel_round,
                    panel_segs,
                    Color::new(panel_r, panel_g, panel_b, blend_a),
                );
            }

            // ── Panel background ──
            let (bg_r, bg_g, bg_b, bg_a) = if has_alert {
                (24u8, 12, 16, 175u8)
            } else {
                (10, 16, 26, 125)
            };
            d.draw_rectangle_rounded(
                rrect(alert_x, alert_y, alert_w, alert_h),
                panel_round,
                panel_segs,
                Color::new(bg_r, bg_g, bg_b, bg_a),
            );

            // ── Left rail: kind-color bar with TTL drain ──
            let rail_w = 3.0 * ui_scale;
            let rail_x = alert_x + 4.0 * ui_scale;
            let rail_y = alert_y + 6.0 * ui_scale;
            let rail_h = alert_h - 12.0 * ui_scale;
            d.draw_rectangle_rounded(
                rrect(rail_x, rail_y, rail_w, rail_h),
                0.5,
                4,
                Color::new(panel_r, panel_g, panel_b, if has_alert { 130 } else { 60 }),
            );
            if has_alert {
                let life = (1.0 - alert_life_frac).clamp(0.0, 1.0);
                let inner_h = rail_h * life;
                // Drains downward from the top so the rail empties like a fuse.
                d.draw_rectangle_rounded(
                    rrect(rail_x, rail_y, rail_w, inner_h),
                    0.5,
                    4,
                    Color::new(panel_r, panel_g, panel_b, (230.0 * glow_pulse) as u8),
                );
            }

            // ── Border ──
            let border_a = if has_alert {
                (50.0 + 40.0 * glow_pulse) as u8
            } else {
                24
            };
            d.draw_rectangle_rounded_lines_ex(
                rrect(alert_x, alert_y, alert_w, alert_h),
                panel_round,
                panel_segs,
                1.0,
                Color::new(panel_r, panel_g, panel_b, border_a),
            );

            // ── Spawn flash: bright wash + white rim, fades in ~1.0s ──
            if has_alert && spawn_env > 0.01 {
                let flash_a = (170.0 * spawn_env.powf(1.4)) as u8;
                d.draw_rectangle_rounded(
                    rrect(alert_x, alert_y, alert_w, alert_h),
                    panel_round,
                    panel_segs,
                    Color::new(panel_r, panel_g, panel_b, flash_a),
                );
                d.draw_rectangle_rounded_lines_ex(
                    rrect(alert_x - 2.0, alert_y - 2.0, alert_w + 4.0, alert_h + 4.0),
                    panel_round,
                    panel_segs,
                    1.5,
                    Color::new(255, 255, 255, (200.0 * spawn_env) as u8),
                );
            }

            // ── Hazard icon — with scale pulse on spawn ──
            let icon_cx = alert_x + 26.0 * ui_scale;
            let icon_cy = alert_y + alert_h * 0.36;
            let icon_base_sz = 11.0 * ui_scale;
            // Scale pulse: pops in at spawn, settles smoothly
            let icon_scale = 1.0 + spawn_env * spawn_env * 0.35 + jolt_env * 0.2;
            let icon_sz = icon_base_sz * icon_scale;
            let icon_a = if has_alert {
                (220.0 * glow_pulse) as u8
            } else {
                80
            };
            if has_alert {
                // Icon glow halo on spawn
                if spawn_env > 0.01 {
                    d.draw_circle(
                        icon_cx as i32,
                        icon_cy as i32,
                        icon_sz * 1.8,
                        Color::new(panel_r, panel_g, panel_b, ((80.0 * spawn_env) as u8) / 2),
                    );
                }
                d.draw_triangle(
                    rvec2(icon_cx, icon_cy - icon_sz),
                    rvec2(icon_cx - icon_sz * 0.87, icon_cy + icon_sz * 0.55),
                    rvec2(icon_cx + icon_sz * 0.87, icon_cy + icon_sz * 0.55),
                    Color::new(panel_r, panel_g, panel_b, icon_a),
                );
                d.draw_triangle(
                    rvec2(icon_cx, icon_cy - icon_sz * 0.50),
                    rvec2(icon_cx - icon_sz * 0.45, icon_cy + icon_sz * 0.30),
                    rvec2(icon_cx + icon_sz * 0.45, icon_cy + icon_sz * 0.30),
                    Color::new(bg_r, bg_g, bg_b, 235),
                );
                let exc_x = (icon_cx - 1.2 * ui_scale * icon_scale) as i32;
                d.draw_rectangle(
                    exc_x,
                    (icon_cy - icon_sz * 0.15) as i32,
                    ((2.4 * ui_scale) as f32 * icon_scale) as i32,
                    ((4.5 * ui_scale) as f32 * icon_scale) as i32,
                    Color::new(panel_r, panel_g, panel_b, icon_a),
                );
                d.draw_rectangle(
                    exc_x,
                    (icon_cy + icon_sz * 0.18) as i32,
                    ((2.4 * ui_scale) as f32 * icon_scale) as i32,
                    ((1.8 * ui_scale) as f32 * icon_scale) as i32,
                    Color::new(panel_r, panel_g, panel_b, icon_a),
                );
            } else {
                let dot_r = 4.0 * ui_scale;
                let dot_a = (70.0 + 50.0 * ((t * 1.1).sin() * 0.5 + 0.5)) as u8;
                d.draw_circle(
                    icon_cx as i32,
                    (alert_y + alert_h * 0.5) as i32,
                    dot_r,
                    Color::new(60, 200, 150, dot_a),
                );
                d.draw_ring(
                    rvec2(icon_cx, alert_y + alert_h * 0.5),
                    dot_r + 2.5,
                    dot_r + 3.5,
                    0.0,
                    360.0,
                    20,
                    Color::new(60, 200, 150, 50),
                );
            }

            // ── Severity dot meter (5 dots, vertical) with ripple pulses ──
            {
                use crate::ui::alert::ripple_at;
                let sev_cx = icon_cx + icon_sz * 1.45;
                let sev_top = alert_y + 8.0 * ui_scale;
                let dot_r = 1.7 * ui_scale;
                let gap = 4.5 * ui_scale;
                let lit = (severity * 5.0).round().clamp(0.0, 5.0) as i32;
                for i in 0..5 {
                    let cy = sev_top + i as f32 * gap;
                    // Top dot lights last (most-severe = top).
                    let on = (4 - i) < lit;
                    let (a, r, g, b) = if on {
                        let pulse = ((t * 3.0 + i as f32 * 0.7).sin() * 0.25 + 0.75) as f32;
                        // Ripple halo from each lit dot
                        let ripple = ripple_at(t, 0.0, 3.2, 1.5);
                        let halo_r = dot_r * (1.5 + ripple * 0.8);
                        if ripple > 0.01 {
                            d.draw_circle(
                                sev_cx as i32,
                                cy as i32,
                                halo_r,
                                Color::new(panel_r, panel_g, panel_b, ((100.0 * ripple) as u8) / 3),
                            );
                        }
                        ((220.0 * pulse) as u8, panel_r, panel_g, panel_b)
                    } else {
                        (38u8, panel_r, panel_g, panel_b)
                    };
                    d.draw_circle(sev_cx as i32, cy as i32, dot_r, Color::new(r, g, b, a));
                }
            }

            // ── Text region ──
            let txt_x = (icon_cx + icon_sz * 2.7) as i32;

            if has_alert {
                // INCOMING tag — slides in from the left, fades over 0.7s.
                let incoming_env = (1.0 - alert_age / 0.7).clamp(0.0, 1.0);
                if incoming_env > 0.01 {
                    let tag = "INCOMING \u{25B8}";
                    let tag_a = (255.0 * incoming_env) as u8;
                    let slide = (1.0 - incoming_env) * 8.0 * ui_scale;
                    d.draw_text(
                        tag,
                        (txt_x as f32 - slide) as i32,
                        (alert_y + 4.0 * ui_scale) as i32,
                        font_xs,
                        Color::new(255, 255, 255, tag_a),
                    );
                }

                // Message text with decode-scramble intro (first 0.55s).
                let decode_progress = (alert_age / 0.55).clamp(0.0, 1.0);
                let decode_step = (alert_age * 30.0) as u32;
                let displayed =
                    decode_scramble(&alert_message, decode_progress, alert_seed, decode_step);
                let msg_y = (alert_y + alert_h * 0.30) as i32;

                // Text shadow/depth for premium feel
                d.draw_text(
                    &displayed,
                    txt_x + 1,
                    msg_y + 1,
                    font_xs,
                    Color::new(0, 0, 0, (60.0 * glow_pulse) as u8),
                );

                // Chromatic ghosts for high-severity alerts — sells the heat.
                if severity > 0.55 {
                    let off = ((1.0 + severity * 1.5) * ui_scale) as i32;
                    let chrom_a = ((80.0 + 60.0 * severity) * glow_pulse) as u8;
                    d.draw_text(
                        &displayed,
                        txt_x + off,
                        msg_y,
                        font_xs,
                        Color::new(255, 60, 80, chrom_a / 2),
                    );
                    d.draw_text(
                        &displayed,
                        txt_x - off,
                        msg_y,
                        font_xs,
                        Color::new(60, 200, 255, chrom_a / 2),
                    );
                }

                // Main text with full alpha
                let msg_a = (235.0 * glow_pulse) as u8;
                d.draw_text(
                    &displayed,
                    txt_x,
                    msg_y,
                    font_xs,
                    Color::new(panel_r, panel_g, panel_b, msg_a),
                );

                // Sub-text — fades in after the decode resolves so it feels
                // like the system is *thinking out loud* a half-beat later.
                if alert_age > 0.45 {
                    let sub_in = ((alert_age - 0.45) / 0.4).clamp(0.0, 1.0);
                    let sub_a = (180.0 * glow_pulse * sub_in) as u8;
                    d.draw_text(
                        &alert_sub,
                        txt_x,
                        (alert_y + alert_h * 0.55) as i32,
                        font_xs,
                        Color::new(220, 185, 170, sub_a),
                    );
                }
            } else {
                // Nominal — slowly-rotating poetic line + entropy state line.
                let whisper = nominal_whisper(nominal_clock);
                d.draw_text(
                    whisper,
                    txt_x,
                    (alert_y + alert_h * 0.30) as i32,
                    font_xs,
                    Color::new(120, 210, 170, 165),
                );
                let ent_str = format!(
                    "mood {}  \u{00B7}  entropy {:.0}%  \u{00B7}  {}",
                    mood.label().to_lowercase(),
                    entropy * 100.0,
                    if entropy_trend > 0.005 {
                        "rising"
                    } else if entropy_trend < -0.005 {
                        "falling"
                    } else {
                        "stable"
                    }
                );
                d.draw_text(
                    &ent_str,
                    txt_x,
                    (alert_y + alert_h * 0.55) as i32,
                    font_xs,
                    Color::new(120, 160, 140, 130),
                );
            }

            // ── Top-right: alert counter + TTL micro-clock dots ──
            if has_alert {
                let counter = format!("{}/{}", alert_idx_disp, alert_count);
                let cw = d.measure_text(&counter, font_xs);
                let cx = (alert_x + alert_w - 10.0 * ui_scale - cw as f32) as i32;
                d.draw_text(
                    &counter,
                    cx,
                    (alert_y + 5.0 * ui_scale) as i32,
                    font_xs,
                    Color::new(panel_r, panel_g, panel_b, 200),
                );
                // 8 TTL dots — drain right-to-left as the alert ages.
                let life = (1.0 - alert_life_frac).clamp(0.0, 1.0);
                let dot_r = 1.4 * ui_scale;
                let gap = 4.5 * ui_scale;
                let total = 8;
                let lit = (life * total as f32).ceil() as i32;
                let row_y = alert_y + alert_h - 9.0 * ui_scale;
                let row_x_end = alert_x + alert_w - 10.0 * ui_scale;
                for i in 0..total {
                    let cx_i = row_x_end - i as f32 * gap;
                    let on = i < lit;
                    let a8 = if on { 200 } else { 38 };
                    d.draw_circle(
                        cx_i as i32,
                        row_y as i32,
                        dot_r,
                        Color::new(panel_r, panel_g, panel_b, a8),
                    );
                }
            }

            // ── EKG waveform with multiple heartbeats + scanline parallax ──
            {
                let ekg_y_center = alert_y + alert_h - 14.0 * ui_scale;
                let ekg_x0 = alert_x + 14.0 * ui_scale;
                let ekg_x1 = alert_x + alert_w - 70.0 * ui_scale;
                let ekg_w = (ekg_x1 - ekg_x0).max(40.0);
                let segs = 56;
                let amp = (3.0 + severity * 7.0) * ui_scale;
                let speed = 1.4 + severity * 2.6;
                let beat_phase = t * speed + (alert_seed as f32) * 0.0001;

                // Render multiple EKG layers — 3 heartbeats with phase offsets for depth.
                for layer in 0..3 {
                    let layer_phase = beat_phase + layer as f32 * 1.35;
                    let layer_alpha_base = (100.0 - layer as f32 * 35.0).max(20.0);
                    let layer_width = if layer == 0 { 1.0 } else { 0.5 };
                    let mut prev = (ekg_x0, ekg_y_center);

                    for i in 1..=segs {
                        let f = i as f32 / segs as f32;
                        let x = ekg_x0 + ekg_w * f;
                        // Position within a single heartbeat cycle (0..1).
                        let p = (f * 4.0 - layer_phase * 0.6).rem_euclid(1.0);

                        // QRS-style spike near p≈0.5, bracketed by Q and S dips.
                        let spike = if (0.46..0.54).contains(&p) {
                            let q = (p - 0.50) / 0.04;
                            (-q.abs() * 3.0).exp() * (1.0 - q.abs()).max(0.0)
                        } else if (0.40..0.46).contains(&p) {
                            -0.25 * ((p - 0.43) / 0.03).cos()
                        } else if (0.54..0.62).contains(&p) {
                            -0.20 * ((p - 0.58) / 0.04).cos()
                        } else {
                            0.0
                        };

                        let breath = (f * 18.0 + t * 0.4 + layer as f32 * 0.3).sin() * 0.15;
                        let dy = -(spike * 2.4 + breath) * amp;
                        let y = ekg_y_center + dy;

                        let glow_a = ((layer_alpha_base + 40.0 * severity * glow_pulse) as f32
                            * (1.0 - layer as f32 * 0.3)) as u8;
                        let core_a = (glow_a as f32 * 0.8) as u8;

                        d.draw_line_ex(
                            rvec2(prev.0, prev.1),
                            rvec2(x, y),
                            layer_width + 2.0,
                            Color::new(panel_r, panel_g, panel_b, glow_a / 3),
                        );
                        d.draw_line_ex(
                            rvec2(prev.0, prev.1),
                            rvec2(x, y),
                            layer_width,
                            Color::new(panel_r, panel_g, panel_b, core_a),
                        );
                        prev = (x, y);
                    }
                }

                // Scanline parallax effect — moving lines that shift with time.
                if has_alert && severity > 0.4 {
                    let scanline_speed = 40.0 + severity * 60.0;
                    let num_lines = ((alert_h / (3.0 * ui_scale)).ceil() as i32).min(12);
                    for i in 0..num_lines {
                        let offset = ((t * scanline_speed + i as f32 * 1.7).fract() - 0.5) * 2.0;
                        let line_y =
                            ekg_y_center + offset * 8.0 * ui_scale - alert_h / 4.0 + i as f32 * 3.0 * ui_scale;
                        if line_y >= alert_y && line_y <= alert_y + alert_h {
                            let scan_a = ((20.0 + 30.0 * severity * glow_pulse
                                * (1.0 - (offset.abs()).clamp(0.0, 1.0)))
                                as u8)
                                / 2;
                            d.draw_line_ex(
                                rvec2(ekg_x0 - 2.0, line_y),
                                rvec2(ekg_x1 + 2.0, line_y),
                                1.0,
                                Color::new(panel_r, panel_g, panel_b, scan_a),
                            );
                        }
                    }
                }
            }

            // ── Random data-corruption flicker (high severity only) ──
            //    Bursts ~1.7×/sec for ~60ms each; per-burst lines are seeded
            //    by the alert seed so the pattern feels native to the alert.
            if has_alert && severity > 0.65 {
                let trig_phase = (t * 1.7).fract();
                if trig_phase < 0.06 {
                    let burst_a = ((1.0 - trig_phase / 0.06) * 200.0 * severity) as u8;
                    for k in 0..3u32 {
                        let mix = alert_seed
                            .wrapping_add(k.wrapping_mul(31_700))
                            .wrapping_add((t * 11.0) as u32);
                        let rel_y =
                            ((mix.wrapping_mul(2654435761) >> 16) & 0xFFFF) as f32 / 65535.0;
                        let line_y =
                            alert_y + 4.0 * ui_scale + rel_y * (alert_h - 8.0 * ui_scale);
                        let bar_h = (1.0 + 2.0 * k as f32) as i32;
                        d.draw_rectangle(
                            alert_x as i32,
                            line_y as i32,
                            alert_w as i32,
                            bar_h,
                            Color::new(255, 255, 255, burst_a / (k as u8 + 2)),
                        );
                    }
                }
            }
        }

        // (Mood badge is now an integrated chip on the meter panel above.)

        // ── Top-Right: Clock + Uptime + TKN/s ──
        {
            let tr_w = 152.0 * ui_scale;
            let tr_h = 76.0 * ui_scale;
            let tr_x = wf - tr_w - pad as f32;
            let tr_y = pad as f32;

            d.draw_rectangle_rounded(
                rrect(tr_x, tr_y, tr_w, tr_h),
                panel_round,
                panel_segs,
                col_panel_bg,
            );
            d.draw_rectangle_rounded_lines_ex(
                rrect(tr_x, tr_y, tr_w, tr_h),
                panel_round,
                panel_segs,
                1.0,
                col_panel_edge,
            );

            let inner_x = tr_x + 8.0 * ui_scale;
            let mut row_y = tr_y + 6.0 * ui_scale;

            // ── Local time — large, primary ──
            let time_str = format!("{:02}:{:02}:{:02}", local_hour, local_minute, local_second);
            // Colon blink: dim colons every other second for a living clock feel
            let colon_bright = local_second % 2 == 0;
            let time_col = if colon_bright {
                col_white
            } else {
                Color::new(col_white.r, col_white.g, col_white.b, 180)
            };
            d.draw_text(&time_str, inner_x as i32, row_y as i32, font_md, time_col);
            // Timezone badge — small, right-aligned
            if !timezone_name.is_empty() {
                let tz_w = d.measure_text(&timezone_name, font_xs);
                d.draw_text(
                    &timezone_name,
                    (tr_x + tr_w - 8.0 * ui_scale - tz_w as f32) as i32,
                    row_y as i32,
                    font_xs,
                    Color::new(100, 180, 255, 140),
                );
            }
            row_y += font_md as f32 + 3.0 * ui_scale;

            // ── Time-of-day descriptor — contextual color ──
            let (tod_label, tod_col) = match local_hour {
                5..=6 => ("DAWN", Color::new(255, 180, 100, 200)),
                7..=11 => ("MORNING", Color::new(255, 220, 140, 200)),
                12..=13 => ("MIDDAY", Color::new(255, 255, 180, 200)),
                14..=17 => ("AFTERNOON", Color::new(220, 200, 160, 200)),
                18..=20 => ("EVENING", Color::new(160, 140, 200, 200)),
                21..=23 => ("NIGHT", Color::new(100, 130, 200, 180)),
                _ => ("DEEP NIGHT", Color::new(80, 100, 180, 160)),
            };
            d.draw_text(tod_label, inner_x as i32, row_y as i32, font_xs, tod_col);

            // ── Uptime — right side, same row ──
            let (hr, mn, sc) = (uptime / 3600, (uptime % 3600) / 60, uptime % 60);
            let uptime_str = format!("T+{hr:02}:{mn:02}:{sc:02}");
            let up_w = d.measure_text(&uptime_str, font_xs);
            d.draw_text(
                &uptime_str,
                (tr_x + tr_w - 8.0 * ui_scale - up_w as f32) as i32,
                row_y as i32,
                font_xs,
                col_white_dim,
            );
            row_y += font_xs as f32 + 3.0 * ui_scale;

            // ── LLM state / TKN/s — bottom row ──
            let llm_busy = is_thinking && display_tps <= 0.1;
            let pulse_a = (150.0 + 70.0 * ((t * 3.2).sin() * 0.5 + 0.5)) as u8;
            let tv_col = if llm_busy {
                Color::new(120, 220, 255, pulse_a)
            } else if display_tps > 1.0 {
                col_cyan
            } else {
                col_white_dim
            };
            let llm_label = if llm_busy {
                let dots = match ((t * 2.0) as i32).rem_euclid(4) {
                    0 => "",
                    1 => ".",
                    2 => "..",
                    _ => "...",
                };
                format!("LLM BUSY{}", dots)
            } else {
                format!("TKN/s {:.1}", display_tps)
            };
            d.draw_text(&llm_label, inner_x as i32, row_y as i32, font_xs, tv_col);
        }

        // ── Bottom-Left: Extended Stats Panel ──
        {
            let bl_w = 188.0 * ui_scale;
            let row_h = font_xs as f32 + 4.0 * ui_scale;
            let rows = 9; // entropy bar + components + I/O rate + procs + files + disk + net rx + net tx + load
            let bl_h = 8.0 * ui_scale + row_h * rows as f32 + 18.0 * ui_scale;
            let bl_x = pad as f32;
            let bl_y = hf - bl_h - pad as f32;

            d.draw_rectangle_rounded(
                rrect(bl_x, bl_y, bl_w, bl_h),
                panel_round,
                panel_segs,
                col_panel_bg,
            );
            d.draw_rectangle_rounded_lines_ex(
                rrect(bl_x, bl_y, bl_w, bl_h),
                panel_round,
                panel_segs,
                1.0,
                col_panel_edge,
            );

            let lx = (bl_x + 10.0 * ui_scale) as i32;
            let mut cy = bl_y + 8.0 * ui_scale;

            // Entropy bar — real composite from system metrics
            let ent_pct = entropy * 100.0;
            let ent_label = format!("ENTROPY {:.0}%", ent_pct);
            d.draw_text(&ent_label, lx, cy as i32, font_xs, col_white_dim);
            let bar_x = bl_x + 10.0 * ui_scale;
            cy += font_xs as f32 + 2.0 * ui_scale;
            let bar_w = bl_w - 20.0 * ui_scale;
            let bar_h = (4.5 * ui_scale).max(3.0);
            d.draw_rectangle_rounded(rrect(bar_x, cy, bar_w, bar_h), 0.5, 4, col_track);
            // Entropy bar color: green <30%, yellow 30-60%, orange 60-80%, red >80%
            let ent_col = if entropy > 0.8 {
                col_red
            } else if entropy > 0.6 {
                Color::new(255, 140, 40, 230)
            } else if entropy > 0.3 {
                Color::new(255, 210, 60, 220)
            } else {
                col_green
            };
            let ent_display = entropy.clamp(0.02, 1.0); // minimum sliver visible
            d.draw_rectangle_rounded(
                rrect(bar_x, cy, bar_w * ent_display, bar_h),
                0.5,
                4,
                ent_col,
            );
            cy += bar_h + 2.0 * ui_scale;

            // Entropy component sparkline — 5 tiny bars showing CPU/MEM/PRC/NET/LOAD contribution
            {
                let labels = ["C", "M", "P", "N", "L"];
                let colors = [
                    col_cyan,
                    Color::new(180, 130, 255, 200),
                    Color::new(200, 200, 100, 200),
                    col_green,
                    Color::new(255, 200, 90, 200),
                ];
                let comp_w = (bar_w - 8.0 * ui_scale) / 5.0;
                let comp_h = (3.0 * ui_scale).max(2.0);
                for (i, (&val, &label)) in entropy_components.iter().zip(labels.iter()).enumerate()
                {
                    let cx_bar = bar_x + i as f32 * (comp_w + 2.0 * ui_scale);
                    d.draw_rectangle_rounded(
                        rrect(cx_bar, cy, comp_w, comp_h),
                        0.4,
                        3,
                        Color::new(30, 40, 50, 100),
                    );
                    let fill = val.clamp(0.0, 1.0);
                    if fill > 0.01 {
                        d.draw_rectangle_rounded(
                            rrect(cx_bar, cy, comp_w * fill, comp_h),
                            0.4,
                            3,
                            colors[i],
                        );
                    }
                    // Tiny label below
                    d.draw_text(
                        label,
                        (cx_bar + comp_w * 0.3) as i32,
                        (cy + comp_h + 1.0) as i32,
                        (font_xs as f32 * 0.7) as i32,
                        Color::new(100, 120, 140, 100),
                    );
                }
                cy += comp_h + font_xs as f32 * 0.7 + 5.0 * ui_scale;
            }

            // Subtle section divider — separates entropy block from system stats
            d.draw_line_ex(rvec2(bar_x, cy), rvec2(bar_x + bar_w, cy), 1.0, col_divider);
            cy += 4.0 * ui_scale;

            // I/O Rate — real network throughput per second
            let fmt_rate = |bytes_per_sec: f64| -> String {
                if bytes_per_sec > 1_073_741_824.0 {
                    format!("{:.1}G/s", bytes_per_sec / 1_073_741_824.0)
                } else if bytes_per_sec > 1_048_576.0 {
                    format!("{:.1}M/s", bytes_per_sec / 1_048_576.0)
                } else if bytes_per_sec > 1024.0 {
                    format!("{:.0}K/s", bytes_per_sec / 1024.0)
                } else {
                    format!("{:.0}B/s", bytes_per_sec)
                }
            };
            let io_str = format!("I/O  {} / {}", fmt_rate(net_rx_rate), fmt_rate(net_tx_rate));
            let io_col = if net_rx_rate + net_tx_rate > 10_000_000.0 {
                Color::new(255, 200, 90, 220)
            } else {
                Color::new(80, 180, 160, 180)
            };
            d.draw_text(&io_str, lx, cy as i32, font_xs, io_col);
            cy += row_h;

            // Process count
            d.draw_text(
                &format!("PROCS  {}", proc_count),
                lx,
                cy as i32,
                font_xs,
                col_cyan,
            );
            cy += row_h;

            // File count
            let file_str = if file_count > 9999 {
                format!("FILES  {}k", file_count / 1000)
            } else {
                format!("FILES  {}", file_count)
            };
            d.draw_text(&file_str, lx, cy as i32, font_xs, col_purple);
            cy += row_h;

            // Disk
            d.draw_text(
                &format!("DISK   {:.1}/{:.0}G", disk_used, disk_total),
                lx,
                cy as i32,
                font_xs,
                col_white_dim,
            );
            cy += row_h;

            // Net RX / TX (cumulative)
            let rx_mb = net_rx as f64 / 1_048_576.0;
            let tx_mb = net_tx as f64 / 1_048_576.0;
            let fmt_bytes = |mb: f64| -> String {
                if mb > 1024.0 {
                    format!("{:.1}G", mb / 1024.0)
                } else {
                    format!("{:.1}M", mb)
                }
            };
            d.draw_text(
                &format!("NET RX {}", fmt_bytes(rx_mb)),
                lx,
                cy as i32,
                font_xs,
                col_green,
            );
            cy += row_h;
            d.draw_text(
                &format!("NET TX {}", fmt_bytes(tx_mb)),
                lx,
                cy as i32,
                font_xs,
                col_green,
            );
            cy += row_h;

            // Load average
            let la_col = if load_avg > 4.0 {
                col_red
            } else if load_avg > 2.0 {
                Color::new(255, 200, 90, 220)
            } else {
                col_white_dim
            };
            d.draw_text(
                &format!("LOAD   {:.2}", load_avg),
                lx,
                cy as i32,
                font_xs,
                la_col,
            );
        }

        // ── Bottom-Right: Nerve Activity Monitor ──
        // Richer panel: heartbeat strip showing impulse rhythm, expanded
        // newest impulse with details + trigger badge + success state,
        // compact history with relative timestamps, and a footer with
        // rhythm classification and next-impulse estimate.
        if SHOW_NERVE_PANEL && !action_log_snap.is_empty() {
            let now_inst = std::time::Instant::now();

            let br_w = 312.0 * ui_scale;

            // Layout constants
            let pad_in = 10.0 * ui_scale;
            let header_h = font_xs as f32 + 8.0 * ui_scale;
            let strip_h = 14.0 * ui_scale;
            let div_pad = 6.0 * ui_scale;
            let row_h_a = font_xs as f32 + 5.0 * ui_scale;
            let footer_h = font_xs as f32 + 8.0 * ui_scale;

            // Newest entry gets an expanded block (label/meta + summary + details)
            let newest_block_h = (font_xs as f32 + 4.0 * ui_scale)  // header line
                               + (font_xs as f32 + 3.0 * ui_scale)  // summary line
                               + (font_xs as f32 + 2.0 * ui_scale); // details line

            let history_count = action_log_snap.len().saturating_sub(1).min(4);
            let history_h = history_count as f32 * row_h_a;

            let br_h = pad_in
                + header_h
                + div_pad
                + strip_h
                + div_pad
                + newest_block_h
                + div_pad
                + history_h
                + (if history_count > 0 { div_pad } else { 0.0 })
                + footer_h
                + pad_in;

            let br_x = wf - br_w - pad as f32;
            let br_y = hf - br_h - pad as f32;

            // Glow around panel when a new impulse just fired
            let nerve_glow = if nerve_flash_timer > 0.0 {
                (nerve_flash_timer / 2.5).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let glow_col = if nerve_glow > 0.1 {
                action_log_snap
                    .last()
                    .map(|e| e.kind.color())
                    .unwrap_or((80, 200, 255))
            } else {
                match mood {
                    Mood::Serene => (80u8, 255, 200),
                    Mood::Alert => (80, 200, 255),
                    Mood::Stressed => (255, 180, 60),
                    Mood::Critical => (255, 80, 60),
                }
            };

            if nerve_glow > 0.05 {
                let ga = (30.0 * nerve_glow) as u8;
                d.draw_rectangle_rounded(
                    rrect(br_x - 5.0, br_y - 5.0, br_w + 10.0, br_h + 10.0),
                    panel_round,
                    panel_segs,
                    Color::new(glow_col.0, glow_col.1, glow_col.2, ga),
                );
            }

            d.draw_rectangle_rounded(
                rrect(br_x, br_y, br_w, br_h),
                panel_round,
                panel_segs,
                col_panel_bg,
            );
            let border_a = if nerve_glow > 0.1 {
                (25.0 + 45.0 * nerve_glow) as u8
            } else {
                22
            };
            d.draw_rectangle_rounded_lines_ex(
                rrect(br_x, br_y, br_w, br_h),
                panel_round,
                panel_segs,
                1.0,
                Color::new(glow_col.0, glow_col.1, glow_col.2, border_a),
            );

            let inner_x = br_x + pad_in;
            let inner_w = br_w - pad_in * 2.0;
            let mut ny = br_y + pad_in;

            // ── Helpers ──
            // Format relative time from an Instant.
            let fmt_ago = |inst: std::time::Instant| -> String {
                let secs = now_inst.saturating_duration_since(inst).as_secs();
                if secs < 5 {
                    "now".to_string()
                } else if secs < 60 {
                    format!("{}s", secs)
                } else if secs < 3600 {
                    format!("{}m", secs / 60)
                } else {
                    format!("{}h", secs / 3600)
                }
            };

            // Compute rhythm classification from intervals between impulses.
            // CV = std/mean of intervals over the last few impulses.
            let (rhythm_label, rhythm_col): (&str, (u8, u8, u8)) = {
                let n = action_log_snap.len();
                if n < 2 {
                    ("WARMING", (180, 200, 220))
                } else {
                    let mut intervals: Vec<f32> = Vec::with_capacity(n - 1);
                    for i in 1..n {
                        let dt_s = action_log_snap[i]
                            .timestamp
                            .saturating_duration_since(action_log_snap[i - 1].timestamp)
                            .as_secs_f32();
                        intervals.push(dt_s.max(0.5));
                    }
                    let mean: f32 = intervals.iter().sum::<f32>() / intervals.len() as f32;
                    let var: f32 = intervals.iter().map(|x| (x - mean).powi(2)).sum::<f32>()
                        / intervals.len() as f32;
                    let cv = if mean > 0.0 { var.sqrt() / mean } else { 0.0 };
                    if cv < 0.45 {
                        ("STEADY", (120, 230, 180))
                    } else if cv < 1.20 {
                        ("IRREGULAR", (255, 210, 120))
                    } else {
                        ("BURSTY", (255, 140, 100))
                    }
                }
            };

            // ── 1. Header row: status dot + title + count + rhythm tag ──
            let hdr_y = ny as i32;
            // Pulsing dot
            let pulse = ((t * 2.2).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let dot_a = (130.0 + 110.0 * pulse) as u8;
            let dot_x = inner_x + 3.0 * ui_scale;
            let dot_y = ny + font_xs as f32 * 0.55;
            d.draw_circle(
                dot_x as i32,
                dot_y as i32,
                2.5 * ui_scale,
                Color::new(glow_col.0, glow_col.1, glow_col.2, dot_a),
            );

            let title_x = (inner_x + 12.0 * ui_scale) as i32;
            let hdr_pulse_a = (160.0 + 50.0 * pulse) as u8;
            let title = format!("NERVE ACTIVITY [{}]", action_count);
            d.draw_text(
                &title,
                title_x,
                hdr_y,
                font_xs,
                Color::new(glow_col.0, glow_col.1, glow_col.2, hdr_pulse_a),
            );

            // Right-aligned rhythm tag
            let rhythm_text = format!("RHYTHM {}", rhythm_label);
            let rhythm_w = d.measure_text(&rhythm_text, font_xs);
            d.draw_text(
                &rhythm_text,
                (inner_x + inner_w - rhythm_w as f32) as i32,
                hdr_y,
                font_xs,
                Color::new(rhythm_col.0, rhythm_col.1, rhythm_col.2, 200),
            );

            ny += header_h + div_pad - 4.0 * ui_scale;
            d.draw_line_ex(
                rvec2(inner_x, ny),
                rvec2(inner_x + inner_w, ny),
                1.0,
                col_divider,
            );
            ny += 4.0 * ui_scale;

            // ── 2. Heartbeat strip — vertical ticks for each impulse over a
            //  ~5-minute rolling window, color-coded by action kind. Older
            //  impulses fade. Provides a visual "EKG" of cognitive activity.
            {
                let strip_y = ny;
                let strip_x = inner_x;
                let strip_w = inner_w;
                let span_secs: f32 = 300.0; // 5-minute window

                // Faint baseline
                let mid_y = strip_y + strip_h * 0.5;
                d.draw_line_ex(
                    rvec2(strip_x, mid_y),
                    rvec2(strip_x + strip_w, mid_y),
                    1.0,
                    Color::new(80, 100, 130, 35),
                );

                // Time tick marks at -5m / -2.5m / now
                for frac in [0.0_f32, 0.5, 1.0] {
                    let tx = strip_x + frac * strip_w;
                    d.draw_line_ex(
                        rvec2(tx, mid_y - 1.5),
                        rvec2(tx, mid_y + 1.5),
                        1.0,
                        Color::new(80, 100, 130, 70),
                    );
                }

                // Plot impulses
                for entry in action_log_snap.iter() {
                    let age = now_inst
                        .saturating_duration_since(entry.timestamp)
                        .as_secs_f32();
                    if age > span_secs {
                        continue;
                    }
                    let frac = 1.0 - (age / span_secs).clamp(0.0, 1.0);
                    let ex = strip_x + frac * strip_w;

                    let (cr, cg, cb) = entry.kind.color();
                    let age_alpha = (1.0 - age / span_secs).clamp(0.25, 1.0);
                    let amp = (strip_h * 0.40) * (0.55 + 0.45 * age_alpha);
                    let ca = (210.0 * age_alpha) as u8;

                    // Vertical tick (success: full height, failure: half + red dot)
                    if entry.success {
                        d.draw_line_ex(
                            rvec2(ex, mid_y - amp),
                            rvec2(ex, mid_y + amp),
                            1.0 + 0.5 * ui_scale,
                            Color::new(cr, cg, cb, ca),
                        );
                    } else {
                        d.draw_line_ex(
                            rvec2(ex, mid_y - amp * 0.5),
                            rvec2(ex, mid_y + amp * 0.5),
                            1.0,
                            Color::new(cr, cg, cb, (ca / 2).max(60)),
                        );
                        d.draw_circle(
                            ex as i32,
                            mid_y as i32,
                            1.5 * ui_scale,
                            Color::new(255, 90, 80, ca),
                        );
                    }
                }

                // "now" indicator — small triangle on the right edge
                let now_x = strip_x + strip_w;
                d.draw_triangle(
                    rvec2(now_x, mid_y - 3.5 * ui_scale),
                    rvec2(now_x + 3.0 * ui_scale, mid_y),
                    rvec2(now_x, mid_y + 3.5 * ui_scale),
                    Color::new(glow_col.0, glow_col.1, glow_col.2, 180),
                );

                ny += strip_h + div_pad;
            }

            d.draw_line_ex(
                rvec2(inner_x, ny - 4.0 * ui_scale),
                rvec2(inner_x + inner_w, ny - 4.0 * ui_scale),
                1.0,
                col_divider,
            );

            // ── 3. Newest impulse — expanded block ──
            if let Some(newest) = action_log_snap.last() {
                let (kr, kg, kb) = newest.kind.color();
                let (tr, tg, tb) = newest.trigger.color();
                let label = newest.kind.label();
                let icon = newest.kind.icon();

                // Header: icon LABEL  [TRIG]  · status · ago
                let line1_a = (220.0 * nerve_glow.max(0.75)) as u8;
                let header_str = format!("{} {}", icon, label);
                d.draw_text(
                    &header_str,
                    inner_x as i32,
                    ny as i32,
                    font_xs,
                    Color::new(kr, kg, kb, line1_a),
                );

                let header_w = d.measure_text(&header_str, font_xs) as f32;

                // Trigger badge
                let trig_text = format!("[{}]", newest.trigger.label());
                let trig_x = inner_x + header_w + 8.0 * ui_scale;
                d.draw_text(
                    &trig_text,
                    trig_x as i32,
                    ny as i32,
                    font_xs,
                    Color::new(tr, tg, tb, 210),
                );

                // Status & time-ago, right-aligned
                let status_glyph = if newest.success { "OK" } else { "FAIL" };
                let status_col = if newest.success {
                    Color::new(120, 230, 160, 220)
                } else {
                    Color::new(255, 110, 100, 230)
                };
                let ago = fmt_ago(newest.timestamp);
                let right_str = format!("{}  {}", status_glyph, ago);
                let right_w = d.measure_text(&right_str, font_xs) as f32;
                d.draw_text(
                    &right_str,
                    (inner_x + inner_w - right_w) as i32,
                    ny as i32,
                    font_xs,
                    status_col,
                );

                ny += font_xs as f32 + 4.0 * ui_scale;

                // Summary line — full width, bright
                let glyph_w = (font_xs as f32 * 0.55).max(4.0);
                let max_chars_sum = (inner_w / glyph_w) as usize;
                let summary_trunc: String = if newest.summary.chars().count() > max_chars_sum {
                    let mut s: String = newest
                        .summary
                        .chars()
                        .take(max_chars_sum.saturating_sub(1))
                        .collect();
                    s.push_str("...");
                    s
                } else {
                    newest.summary.clone()
                };
                d.draw_text(
                    &summary_trunc,
                    inner_x as i32,
                    ny as i32,
                    font_xs,
                    Color::new(220, 230, 240, 215),
                );

                ny += font_xs as f32 + 3.0 * ui_scale;

                // Details line — dim, italic-feel via lower alpha
                let details_src = if newest.details.is_empty() {
                    "(no further detail)".to_string()
                } else {
                    newest.details.replace('\n', " ")
                };
                let details_trunc: String = if details_src.chars().count() > max_chars_sum {
                    let mut s: String = details_src
                        .chars()
                        .take(max_chars_sum.saturating_sub(1))
                        .collect();
                    s.push_str("...");
                    s
                } else {
                    details_src
                };
                let det_col = if newest.details.is_empty() {
                    Color::new(120, 130, 145, 130)
                } else {
                    Color::new(160, 175, 195, 175)
                };
                d.draw_text(&details_trunc, inner_x as i32, ny as i32, font_xs, det_col);

                ny += font_xs as f32 + 2.0 * ui_scale + div_pad;

                if history_count > 0 {
                    d.draw_line_ex(
                        rvec2(inner_x, ny - 4.0 * ui_scale),
                        rvec2(inner_x + inner_w, ny - 4.0 * ui_scale),
                        1.0,
                        col_divider,
                    );
                }
            }

            // ── 4. History rows: prior impulses (skip newest) ──
            // Format: "Xs ago  ICON  LABEL  -  summary..."
            // Time-ago column fixed width so the labels stay aligned.
            let glyph_w = (font_xs as f32 * 0.55).max(4.0);
            let time_col_w = 36.0 * ui_scale;
            let label_col_w = 88.0 * ui_scale;
            let sum_x = inner_x + time_col_w + label_col_w;
            let sum_avail = inner_w - time_col_w - label_col_w;
            let max_chars_h = (sum_avail / glyph_w).max(8.0) as usize;

            // newest is last; iterate older -> exclude index = len-1
            let mut shown = 0usize;
            for entry in action_log_snap.iter().rev().skip(1) {
                if shown >= history_count {
                    break;
                }
                let (kr, kg, kb) = entry.kind.color();
                let fade = (1.0 - shown as f32 * 0.18).max(0.40);

                // time-ago
                let ago = fmt_ago(entry.timestamp);
                d.draw_text(
                    &ago,
                    inner_x as i32,
                    ny as i32,
                    font_xs,
                    Color::new(150, 165, 185, (170.0 * fade) as u8),
                );

                // success/fail micro-indicator (small dot left of label)
                let s_dot_x = inner_x + time_col_w - 6.0 * ui_scale;
                let s_dot_y = ny + font_xs as f32 * 0.5;
                if entry.success {
                    d.draw_circle(
                        s_dot_x as i32,
                        s_dot_y as i32,
                        1.5 * ui_scale,
                        Color::new(120, 220, 160, (180.0 * fade) as u8),
                    );
                } else {
                    d.draw_circle(
                        s_dot_x as i32,
                        s_dot_y as i32,
                        1.5 * ui_scale,
                        Color::new(255, 100, 90, (200.0 * fade) as u8),
                    );
                }

                // icon + label
                let label_str = format!("{} {}", entry.kind.icon(), entry.kind.label());
                d.draw_text(
                    &label_str,
                    (inner_x + time_col_w) as i32,
                    ny as i32,
                    font_xs,
                    Color::new(kr, kg, kb, (190.0 * fade) as u8),
                );

                // summary (truncated)
                let trunc: String = if entry.summary.chars().count() > max_chars_h {
                    let mut s: String = entry
                        .summary
                        .chars()
                        .take(max_chars_h.saturating_sub(1))
                        .collect();
                    s.push_str("...");
                    s
                } else {
                    entry.summary.clone()
                };
                d.draw_text(
                    &trunc,
                    sum_x as i32,
                    ny as i32,
                    font_xs,
                    Color::new(150, 165, 185, (155.0 * fade) as u8),
                );

                ny += row_h_a;
                shown += 1;
            }

            if history_count > 0 {
                ny += div_pad - 4.0 * ui_scale;
                d.draw_line_ex(
                    rvec2(inner_x, ny),
                    rvec2(inner_x + inner_w, ny),
                    1.0,
                    col_divider,
                );
                ny += 4.0 * ui_scale;
            }

            // ── 5. Footer: success-rate + next impulse estimate ──
            // Estimate next-impulse cadence based on mood; show countdown.
            let base_secs: f32 = match mood {
                Mood::Serene => 120.0,
                Mood::Alert => 75.0,
                Mood::Stressed => 50.0,
                Mood::Critical => 30.0,
            };

            // Success rate over visible log
            let total = action_log_snap.len() as f32;
            let succ = action_log_snap.iter().filter(|e| e.success).count() as f32;
            let rate_pct = if total > 0.0 {
                (succ / total * 100.0) as i32
            } else {
                100
            };
            let (sr_r, sr_g, sr_b) = if rate_pct >= 80 {
                (120u8, 220, 160)
            } else if rate_pct >= 50 {
                (255, 200, 120)
            } else {
                (255, 110, 100)
            };

            let footer_y = ny;
            let health_str = format!(
                "HEALTH {}%  ({} ok / {} fired)",
                rate_pct, succ as i32, total as i32
            );
            d.draw_text(
                &health_str,
                inner_x as i32,
                footer_y as i32,
                font_xs,
                Color::new(sr_r, sr_g, sr_b, 195),
            );

            // Right side: next impulse estimate
            let next_str = if let Some(last) = action_log_snap.last() {
                let elapsed = now_inst
                    .saturating_duration_since(last.timestamp)
                    .as_secs_f32();
                let remaining = (base_secs - elapsed).max(0.0);
                if remaining < 5.0 {
                    "NEXT  imminent".to_string()
                } else if remaining < 60.0 {
                    format!("NEXT  ~{}s", remaining as i32)
                } else {
                    format!("NEXT  ~{}m", (remaining / 60.0) as i32)
                }
            } else {
                "NEXT  warming...".to_string()
            };
            let next_w = d.measure_text(&next_str, font_xs) as f32;
            d.draw_text(
                &next_str,
                (inner_x + inner_w - next_w) as i32,
                footer_y as i32,
                font_xs,
                Color::new(180, 200, 220, 175),
            );
        }

        // ══════════════════════════════════════════════════════════
        //  8. Cognitive Stream — minimal bottom subtitle bar
        //  Constrained to the horizontal slot between the bottom-left
        //  (entropy) and bottom-right (nerve) panels so it never
        //  collides with them. Falls back to centered if either side
        //  panel is hidden.
        // ══════════════════════════════════════════════════════════
        {
            let margin = (wf * 0.03) as i32;
            // Estimate bottom-side panel widths so we can carve out a safe slot.
            let bl_w_est = 188.0 * ui_scale;
            let br_w_est = if SHOW_NERVE_PANEL && !action_log_snap.is_empty() {
                312.0 * ui_scale
            } else {
                0.0
            };
            let side_gutter = 18.0 * ui_scale;
            let safe_left = pad as f32 + bl_w_est + side_gutter;
            let safe_right = wf
                - pad as f32
                - (if br_w_est > 0.0 {
                    br_w_est + side_gutter
                } else {
                    0.0
                });
            let safe_w = (safe_right - safe_left).max(360.0);
            // Prefer 60% of screen, but cap to safe slot. Center within that slot.
            let stream_w_f = (wf * 0.60).min(safe_w);
            let slot_center = (safe_left + safe_right) / 2.0;
            let stream_x_f = slot_center - stream_w_f / 2.0;
            let stream_x = stream_x_f as i32;
            let stream_w = stream_w_f as i32;
            let inner_pad = (wf * 0.012).max(8.0) as i32;

            let line_h = (font_md as f32 * 1.4) as i32;
            // 1 active line + up to 3 faded history entries
            let visible_rows = 1 + cognitive_log.len().min(3) as i32;
            let content_h = visible_rows * line_h + inner_pad * 2;
            let panel_bottom = hf as i32 - margin - (hf * 0.04) as i32; // above footer
            let panel_top = panel_bottom - content_h;

            // Frosted glass background — very transparent, no border
            let panel_rect = rrect(
                stream_x_f - inner_pad as f32,
                panel_top as f32,
                stream_w_f + inner_pad as f32 * 2.0,
                content_h as f32,
            );
            d.draw_rectangle_rounded(panel_rect, 0.06, 8, Color::new(8, 12, 20, 90));

            // Subtle top edge glow
            d.draw_line_ex(
                rvec2(stream_x_f, panel_top as f32),
                rvec2(stream_x_f + stream_w_f, panel_top as f32),
                1.0,
                Color::new(80, 180, 255, 20),
            );

            // Status dot (tiny, left side) — replaces verbose header
            let dot_r = 3.0 * ui_scale;
            let dot_x = stream_x_f + dot_r + 4.0;
            let dot_y = panel_top as f32 + inner_pad as f32 + dot_r;
            if is_thinking {
                let pulse = ((t * 2.5).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
                let da = (100.0 + pulse * 155.0) as u8;
                d.draw_circle(
                    dot_x as i32,
                    dot_y as i32,
                    dot_r,
                    Color::new(80, 220, 255, da),
                );
            } else {
                d.draw_circle(
                    dot_x as i32,
                    dot_y as i32,
                    dot_r,
                    Color::new(60, 80, 100, 60),
                );
            }

            let text_x = stream_x + inner_pad + (dot_r * 3.0) as i32;
            let text_area_w = stream_w - inner_pad * 2 - (dot_r * 3.0) as i32;
            let glyph_w = ((font_md as f32 * 0.6) as i32).max(1);
            let max_chars = (text_area_w / glyph_w).max(20) as usize;

            let mut y_cursor = panel_top + inner_pad;

            // Active typewriter line
            if !tw_done {
                let blink = if !tw_done && ((t * 2.5) as u32 % 2 == 0) {
                    "|"
                } else {
                    ""
                };
                let col = if tw_is_system {
                    Color::new(140, 160, 180, 160)
                } else if tw_is_monologue {
                    Color::new(80, 220, 255, 220)
                } else {
                    Color::new(200, 210, 220, 200)
                };
                let display_with_cursor = format!("{}{}", tw_display, blink);
                let wrapped = wrap_lines(&display_with_cursor, max_chars);
                for line in wrapped.iter().take(2) {
                    // max 2 visual lines for active thought
                    if aberration_timer > 0.0 {
                        let ab = (aberration_timer * 3.0).min(1.0);
                        let off = (ab * 2.0) as i32;
                        d.draw_text(
                            line,
                            text_x + off,
                            y_cursor,
                            font_md,
                            Color::new(255, 80, 80, (30.0 * ab) as u8),
                        );
                        d.draw_text(
                            line,
                            text_x - off,
                            y_cursor,
                            font_md,
                            Color::new(80, 80, 255, (30.0 * ab) as u8),
                        );
                    }
                    d.draw_text(line, text_x, y_cursor, font_md, col);
                    y_cursor += line_h;
                }
            } else if is_thinking {
                let dots = match ((t * 2.0) as i32).rem_euclid(4) {
                    0 => "",
                    1 => ".",
                    2 => "..",
                    _ => "...",
                };
                let think_line = format!("AURORA is thinking{}", dots);
                let col = Color::new(
                    90,
                    210,
                    255,
                    (150.0 + 70.0 * ((t * 3.0).sin() * 0.5 + 0.5)) as u8,
                );
                d.draw_text(&think_line, text_x, y_cursor, font_md, col);
                y_cursor += line_h;

                let subline = "prefill + reasoning in flight";
                d.draw_text(
                    subline,
                    text_x,
                    y_cursor,
                    font_sm,
                    Color::new(120, 150, 170, 140),
                );
                y_cursor += (line_h as f32 * 0.85) as i32;
            }

            // Recent log — up to 3 entries, fading with age
            for entry in cognitive_log.iter().rev().take(3) {
                let age = t - entry.born_at;
                let fade = (1.0 - (age / 45.0).min(1.0)).max(0.0);
                if fade < 0.05 {
                    continue;
                }
                let base_a = (120.0 * fade) as u8;
                let col = if entry.is_system {
                    Color::new(100, 120, 140, base_a / 2)
                } else if entry.is_monologue {
                    Color::new(80, 200, 240, base_a)
                } else {
                    Color::new(160, 170, 180, base_a)
                };
                // Single line only for history entries — truncate to fit
                let display_text = if entry.text.chars().count() > max_chars {
                    let take = max_chars.saturating_sub(1);
                    let truncated: String = entry.text.chars().take(take).collect();
                    format!("{}...", truncated)
                } else {
                    entry.text.clone()
                };
                d.draw_text(&display_text, text_x, y_cursor, font_sm, col);
                y_cursor += (line_h as f32 * 0.85) as i32;
            }
        }

        // ══════════════════════════════════════════════════════════
        //  9. Footer — minimal, centered
        // ══════════════════════════════════════════════════════════
        {
            let footer = "AURORA // [F] FULLSCREEN  [ESC] TERMINATE";
            let fw = footer.len() as f32 * font_xs as f32 * 0.52;
            let footer_x = ((wf - fw) / 2.0).max(pad as f32) as i32;
            d.draw_text(
                footer,
                footer_x,
                (hf * 0.97) as i32,
                font_xs,
                Color::new(80, 100, 130, 80),
            );
        }
    }

    // Render loop exited — signal all tasks to stop
    shutdown.store(true, Ordering::Relaxed);
    eprintln!("[aura] render loop ended, shutting down");
    // Final event drain: apply anything still queued from in-flight
    // producer tasks so subsequent persistence hooks see the latest state.
    if let Ok(mut t) = telem.write() {
        let mut drained = 0usize;
        while let Ok(event) = telemetry_event_rx.try_recv() {
            t.apply_event(event);
            drained += 1;
            if drained >= 4096 {
                break;
            }
        }
        if drained > 0 {
            eprintln!("[aura] shutdown drain: applied {} pending event(s)", drained);
        }
    }
    // Final flush of synaptic memory so the most recent thoughts are
    // preserved across the restart boundary.
    if enable_synaptic_web {
        match synaptic_web.save_to_disk(&synapse_save_path) {
            Ok(n) => eprintln!(
                "[aura] synaptic memory final flush: {} neurons → {:?}",
                n, synapse_save_path
            ),
            Err(e) => eprintln!("[aura] synaptic memory final flush failed: {}", e),
        }
    }
    // Give tasks a moment to notice the flag, then exit
    sleep(Duration::from_millis(200)).await;
    std::process::exit(0);
}

// ═══════════════════════════════════════════════════════════════
fn setup_hesitation_and_correction(
    t: f32,
    char_count: usize,
    hesitate_at: &mut Option<usize>,
    hesitate_cooldown: &mut f32,
    sc_phase: &mut u8,
    sc_will_happen: &mut bool,
    sc_trigger_at: &mut usize,
    sc_chars_left: &mut usize,
) {
    let seed = (t * 1000.0) as u32;
    *hesitate_cooldown = 0.0;
    if hash_f(seed) > 0.4 && char_count > 10 {
        let frac = 0.4 + hash_f(seed.wrapping_add(1)) * 0.3;
        *hesitate_at = Some((char_count as f32 * frac) as usize);
    } else {
        *hesitate_at = None;
    }
    *sc_phase = 0;
    if hash_f(seed.wrapping_add(2)) > 0.65 && char_count > 20 {
        *sc_will_happen = true;
        let frac = 0.25 + hash_f(seed.wrapping_add(3)) * 0.2;
        *sc_trigger_at = (char_count as f32 * frac) as usize;
        *sc_chars_left = 3 + (hash_f(seed.wrapping_add(4)) * 4.0) as usize;
    } else {
        *sc_will_happen = false;
    }
}
