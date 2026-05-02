#![allow(dead_code)] // Phase 3-6 scaffolding — see /memories/repo/aura_agent_architecture.md

//! Spectral FFT Bridge — Hann-windowed FFT with perceptual Mel-scale binning.
//!
//! Takes the existing telemetry-driven band data from SpectralAnalyzer
//! and processes it through a proper FFT pipeline:
//!   1. Generate synthetic time-domain signal from telemetry
//!   2. Apply Hann window
//!   3. Compute FFT via simple radix-2 Cooley-Tukey (no external crate)
//!   4. Bin into Mel-scale perceptual frequency bands
//!   5. Output normalized magnitudes for GPU consumption (UBO-ready)
//!
//! This replaces the direct sinusoidal synthesis with frequency-domain analysis
//! that naturally produces richer harmonic content and authentic spectral shapes.

/// FFT size — must be power of 2. 256 gives 128 frequency bins,
/// which is plenty for 64-band Mel-scale output.
pub const FFT_SIZE: usize = 256;
pub const FFT_HALF: usize = FFT_SIZE / 2;

/// Number of Mel-scale output bands.
pub const MEL_BANDS: usize = 64;

pub struct SpectralFFT {
    /// Time-domain sample buffer (ring buffer)
    samples: [f32; FFT_SIZE],
    write_head: usize,
    /// Hann window coefficients (precomputed)
    window: [f32; FFT_SIZE],
    /// Mel-scale bin boundaries (precomputed)
    mel_bins: [usize; MEL_BANDS + 1],
    /// Output: normalized Mel-scale magnitudes
    pub mel_magnitudes: [f32; MEL_BANDS],
    /// Output: smoothed magnitudes (for rendering)
    pub mel_smoothed: [f32; MEL_BANDS],
    /// Peak hold values for each band
    pub mel_peaks: [f32; MEL_BANDS],
}

impl SpectralFFT {
    pub fn new() -> Self {
        // Precompute Hann window
        let mut window = [0.0f32; FFT_SIZE];
        for i in 0..FFT_SIZE {
            let n = i as f32 / FFT_SIZE as f32;
            window[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * n).cos());
        }

        // Precompute Mel-scale bin boundaries
        // Mel scale: m = 2595 * log10(1 + f/700)
        // Inverse: f = 700 * (10^(m/2595) - 1)
        let sample_rate = 256.0; // Virtual sample rate (= FFT_SIZE)
        let f_max = sample_rate / 2.0; // Nyquist
        let mel_min = 0.0_f32;
        let mel_max = 2595.0_f32 * (1.0_f32 + f_max / 700.0).log10();

        let mut mel_bins = [0usize; MEL_BANDS + 1];
        for i in 0..=MEL_BANDS {
            let mel = mel_min + (mel_max - mel_min) * i as f32 / MEL_BANDS as f32;
            let freq = 700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0);
            let bin = (freq / sample_rate * FFT_SIZE as f32).round() as usize;
            mel_bins[i] = bin.min(FFT_HALF);
        }

        Self {
            samples: [0.0; FFT_SIZE],
            write_head: 0,
            window,
            mel_bins,
            mel_magnitudes: [0.0; MEL_BANDS],
            mel_smoothed: [0.0; MEL_BANDS],
            mel_peaks: [0.0; MEL_BANDS],
        }
    }

    /// Feed a new sample into the ring buffer.
    /// Call this each frame with the synthesized telemetry signal.
    pub fn push_sample(&mut self, sample: f32) {
        self.samples[self.write_head] = sample;
        self.write_head = (self.write_head + 1) % FFT_SIZE;
    }

    /// Push multiple samples (one per telemetry parameter).
    /// Synthesizes a composite signal from system telemetry.
    pub fn push_telemetry(
        &mut self,
        t: f32,
        cpu: f32,
        mem: f32,
        net: f32,
        entropy: f32,
        load: f32,
    ) {
        // Synthesize a multi-frequency signal from telemetry
        // Each parameter drives different frequency components
        let cpu_n = cpu / 100.0;
        let mem_n = mem / 100.0;
        let net_n = net.min(1.0);
        let load_n = (load / 8.0).min(1.0);

        // Mix multiple frequencies driven by telemetry values
        let signal = 0.0
            + cpu_n * (t * 2.0).sin() * 0.3       // Low freq: CPU
            + cpu_n * (t * 5.0).sin() * 0.15       // Sub-harmonic
            + mem_n * (t * 8.0).sin() * 0.2        // Mid freq: Memory
            + load_n * (t * 12.0).sin() * 0.15     // Mid-high: Load
            + net_n * (t * 20.0).sin() * 0.15      // High freq: Network
            + entropy * (t * 35.0).sin() * 0.1     // Very high: Entropy
            + (t * 1.0).sin() * 0.05; // Ambient baseline

        self.push_sample(signal);
    }

    /// Run the FFT and update Mel-scale output bands.
    /// Call once per frame after pushing samples.
    pub fn analyze(&mut self, dt: f32) {
        // Read samples from ring buffer in order, apply Hann window
        let mut real = [0.0f32; FFT_SIZE];
        let mut imag = [0.0f32; FFT_SIZE];
        for i in 0..FFT_SIZE {
            let src = (self.write_head + i) % FFT_SIZE;
            real[i] = self.samples[src] * self.window[i];
        }

        // In-place radix-2 Cooley-Tukey FFT
        fft_radix2(&mut real, &mut imag);

        // Compute magnitudes and bin into Mel bands
        let mut fft_mag = [0.0f32; FFT_HALF];
        for i in 0..FFT_HALF {
            fft_mag[i] = (real[i] * real[i] + imag[i] * imag[i]).sqrt();
        }

        // Mel-scale binning: average FFT bins within each Mel band
        for b in 0..MEL_BANDS {
            let lo = self.mel_bins[b];
            let hi = self.mel_bins[b + 1].max(lo + 1);
            let mut sum = 0.0f32;
            let mut count = 0;
            for k in lo..hi.min(FFT_HALF) {
                sum += fft_mag[k];
                count += 1;
            }
            self.mel_magnitudes[b] = if count > 0 { sum / count as f32 } else { 0.0 };
        }

        // Normalize magnitudes to 0..1 range
        let max_mag = self
            .mel_magnitudes
            .iter()
            .cloned()
            .fold(0.0f32, f32::max)
            .max(0.001);
        for b in 0..MEL_BANDS {
            self.mel_magnitudes[b] /= max_mag;
        }

        // Smooth with asymmetric attack/decay
        for b in 0..MEL_BANDS {
            let target = self.mel_magnitudes[b];
            if target > self.mel_smoothed[b] {
                // Fast attack
                self.mel_smoothed[b] += (target - self.mel_smoothed[b]) * dt * 15.0;
            } else {
                // Slow decay
                self.mel_smoothed[b] += (target - self.mel_smoothed[b]) * dt * 3.0;
            }
            self.mel_smoothed[b] = self.mel_smoothed[b].clamp(0.0, 1.0);

            // Peak hold
            if self.mel_smoothed[b] > self.mel_peaks[b] {
                self.mel_peaks[b] = self.mel_smoothed[b];
            } else {
                self.mel_peaks[b] = (self.mel_peaks[b] - dt * 0.3).max(0.0);
            }
        }
    }

    /// Get the Mel-smoothed bands as a flat array suitable for GPU UBO upload.
    pub fn as_ubo_data(&self) -> &[f32; MEL_BANDS] {
        &self.mel_smoothed
    }
}

// ═══════════════════════════════════════════════════════════════
//  Radix-2 Cooley-Tukey FFT (in-place, no external crate)
// ═══════════════════════════════════════════════════════════════

fn fft_radix2(real: &mut [f32; FFT_SIZE], imag: &mut [f32; FFT_SIZE]) {
    let n = FFT_SIZE;

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 0..n {
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
        let mut m = n >> 1;
        while m >= 1 && j >= m {
            j -= m;
            m >>= 1;
        }
        j += m;
    }

    // Butterfly passes
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let angle = -2.0 * std::f32::consts::PI / len as f32;
        let wn_re = angle.cos();
        let wn_im = angle.sin();

        let mut start = 0;
        while start < n {
            let mut w_re = 1.0f32;
            let mut w_im = 0.0f32;
            for k in 0..half {
                let even = start + k;
                let odd = even + half;

                let t_re = w_re * real[odd] - w_im * imag[odd];
                let t_im = w_re * imag[odd] + w_im * real[odd];

                real[odd] = real[even] - t_re;
                imag[odd] = imag[even] - t_im;
                real[even] += t_re;
                imag[even] += t_im;

                let new_w_re = w_re * wn_re - w_im * wn_im;
                let new_w_im = w_re * wn_im + w_im * wn_re;
                w_re = new_w_re;
                w_im = new_w_im;
            }
            start += len;
        }
        len <<= 1;
    }
}
