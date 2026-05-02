use super::noise::{fbm2d, fbm2d_warped};
use crate::core::hash_f;

// ══════════════════════════════════════════════════════════════════
//  Spectral Waveform Analyzer — living frequency visualizer
// ══════════════════════════════════════════════════════════════════
pub const SPEC_BANDS: usize = 64;

pub struct SpectralAnalyzer {
    pub bands: [f32; SPEC_BANDS],
    pub targets: [f32; SPEC_BANDS],
    pub peaks: [f32; SPEC_BANDS],
    pub phases: [f32; SPEC_BANDS],
    pub pulse_energy: f32,
    /// Per-band fBM displacement (normalised, -1..1 range, spec_h-scaled at render)
    pub fbm_offsets: [f32; SPEC_BANDS],
    /// Adaptive signal strength per band — smoothed with asymmetric attack/decay
    /// (VibeDetector-style: fast attack when signal arrives, slow decay when it fades)
    pub signal_strength: [f32; SPEC_BANDS],
}

impl SpectralAnalyzer {
    pub fn new() -> Self {
        let mut phases = [0.0f32; SPEC_BANDS];
        for i in 0..SPEC_BANDS {
            phases[i] = hash_f(i as u32 * 7 + 13) * std::f32::consts::TAU;
        }
        Self {
            bands: [0.0; SPEC_BANDS],
            targets: [0.0; SPEC_BANDS],
            peaks: [0.0; SPEC_BANDS],
            phases,
            pulse_energy: 0.0,
            fbm_offsets: [0.0; SPEC_BANDS],
            signal_strength: [0.0; SPEC_BANDS],
        }
    }

    pub fn trigger_burst(&mut self) {
        self.pulse_energy = 1.0;
    }

    pub fn update(
        &mut self,
        dt: f32,
        t: f32,
        cpu: f32,
        mem: f32,
        entropy: f32,
        entropy_components: [f32; 5],
        net_rx_rate: f64,
        net_tx_rate: f64,
        load_avg: f32,
        is_thinking: bool,
    ) {
        self.pulse_energy = (self.pulse_energy - dt * 0.6).max(0.0);

        let cpu_n = cpu / 100.0;
        let mem_n = mem / 100.0;
        let net_n = ((net_rx_rate + net_tx_rate) / 10_000_000.0).min(1.0) as f32;
        let load_n = (load_avg / 8.0).min(1.0);

        for i in 0..SPEC_BANDS {
            let f = i as f32 / SPEC_BANDS as f32;
            let phase = self.phases[i];

            // Each frequency range is driven by different telemetry signals
            let base = if i < 16 {
                // Bass (0-15): CPU — slow, powerful oscillation
                let wave = (t * (0.3 + f * 0.5) + phase).sin() * 0.5 + 0.5;
                cpu_n * wave * 0.6 + entropy_components[0] * 0.3 + load_n * 0.1
            } else if i < 32 {
                // Low-mid (16-31): Memory + Load — medium rhythm
                let wave = (t * (0.8 + f * 1.2) + phase).sin() * 0.5 + 0.5;
                mem_n * wave * 0.5 + load_n * 0.2 + entropy_components[1] * 0.2
            } else if i < 48 {
                // High-mid (32-47): Network — bursty, reactive
                let wave = (t * (1.5 + f * 2.0) + phase).sin() * 0.5 + 0.5;
                net_n * wave * 0.5 + entropy_components[3] * 0.3 + cpu_n * 0.1
            } else {
                // Treble (48-63): Entropy + Process churn — chaotic
                let wave = (t * (2.0 + f * 3.0) + phase).sin() * 0.5 + 0.5;
                entropy * wave * 0.5 + entropy_components[2] * 0.3 + entropy_components[4] * 0.2
            };

            // LLM thinking adds shimmer across all bands
            let think_boost = if is_thinking {
                let wave = (t * (3.0 + f * 4.0) + phase).sin() * 0.5 + 0.5;
                wave * 0.2
            } else {
                0.0
            };

            // Thought burst — dramatic spike, centered on mid-frequencies
            let burst = self.pulse_energy * (1.0 - (f - 0.5).abs() * 1.8).max(0.0);

            // Ambient floor — always some gentle life
            let ambient = (t * (0.15 + f * 0.3) + phase * 2.0).sin() * 0.04 + 0.06;

            self.targets[i] = (base + think_boost + burst + ambient).clamp(0.0, 1.0);

            // Smooth interpolation — higher bands respond faster
            let speed = 4.0 + f * 8.0;
            self.bands[i] += (self.targets[i] - self.bands[i]) * dt * speed;

            // Peak hold with slow decay
            if self.bands[i] > self.peaks[i] {
                self.peaks[i] = self.bands[i];
            } else {
                self.peaks[i] = (self.peaks[i] - dt * 0.25).max(0.0);
            }

            // ── Adaptive signal strength (VibeDetector-style) ────
            // Fast attack (signal arrives quickly), slow decay (noise
            // wakes up gradually as audio fades — no jarring switch).
            let immediate = self.bands[i].clamp(0.0, 1.0);
            let attack_rate = 12.0_f32; // snaps to loud signal quickly
            let decay_rate = 0.8_f32; // slow fade lets noise breathe in
            if immediate > self.signal_strength[i] {
                self.signal_strength[i] += (immediate - self.signal_strength[i]) * dt * attack_rate;
            } else {
                self.signal_strength[i] += (immediate - self.signal_strength[i]) * dt * decay_rate;
            }
            self.signal_strength[i] = self.signal_strength[i].clamp(0.0, 1.0);
        }

        // ── fBM procedural displacement (Simplex + domain warping) ──
        // Organic "aurora ribbon" breathing using 4-octave Simplex fBM
        // with nested domain warping for liquid-like motion.
        //
        // Travelling wave: noise(x * frequency + t * speed) creates
        // directional drift so hills/valleys propagate across the curve.
        //
        // Signal-to-noise lerp: Y_final = lerp(Y_noise, Y_audio, signal_strength)
        // When signal is weak → noise dominates (no flatline).
        // When signal is loud → raw telemetry data wins.
        //
        // fBM params: 4 octaves, lacunarity 2.0, gain 0.5 (standard 1/f noise)
        let drift_speed = 0.15_f32; // travelling wave drift rate
        let fbm_freq = 2.5_f32; // base spatial frequency
        let fbm_amp = 0.22_f32; // max displacement (fraction of spec_h)
        let warp_blend = 0.35_f32; // domain warping mix (0=plain fBM, 1=full warp)

        for i in 0..SPEC_BANDS {
            let x_norm = i as f32 / SPEC_BANDS as f32; // 0..1

            // Travelling wave phase: x*frequency + t*speed
            // Creates directional "aurora" drift instead of static jitter
            let sample_x = x_norm * fbm_freq + t * drift_speed;
            let sample_y = t * 0.08; // slow vertical evolution axis

            // Standard 4-octave fBM (1/f noise: lacunarity=2.0, gain=0.5)
            let plain_noise = fbm2d(sample_x, sample_y, 4, 2.0, 0.5, fbm_amp, 1.0);

            // Domain-warped fBM — nested noise for liquid/swirling texture
            let warped_noise = fbm2d_warped(sample_x, sample_y, 4, 2.0, 0.5, fbm_amp, 1.0);

            // Blend plain and warped noise (warping adds complexity at some cost)
            let noise_val = plain_noise * (1.0 - warp_blend) + warped_noise * warp_blend;

            // Signal-to-noise lerp using adaptive signal_strength:
            //   Y_final = lerp(Y_noise, 0.0, signal_strength)
            //           = Y_noise * (1.0 - signal_strength)
            // Cubic ease so noise gracefully fades rather than linearly vanishing
            let sig = self.signal_strength[i];
            let noise_weight = (1.0 - sig) * (1.0 - sig); // quadratic ease-out
            self.fbm_offsets[i] = noise_val * noise_weight;
        }
    }
}
