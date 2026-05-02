//! Telemetry subsystem: system sampling, entropy computation, and status formatting.
//!
//! Contains the `EntropyEngine` for computing composite system entropy from
//! rolling telemetry windows, plus utility functions for disk/load/file sampling.

use std::collections::VecDeque;

use crate::core::Mood;

// ═══════════════════════════════════════════════════════════════
//  Entropy Engine — real system entropy from telemetry signals
// ═══════════════════════════════════════════════════════════════

pub const ENTROPY_WINDOW: usize = 40; // ~10 seconds at 250ms polling

pub struct EntropyEngine {
    cpu_history: VecDeque<f32>,
    mem_history: VecDeque<f32>,
    proc_history: VecDeque<u32>,
    net_rx_history: VecDeque<u64>,
    net_tx_history: VecDeque<u64>,
    load_history: VecDeque<f32>,
    file_churn_history: VecDeque<f32>,
    pub entropy: f32,
    pub entropy_prev: f32,
    pub entropy_smoothed: f32,
    pub entropy_trend: f32,
    pub peak_entropy: f32,
    pub components: [f32; 5], // cpu_var, mem_delta, proc+file_churn, net_burst, load_dev
}

impl EntropyEngine {
    pub fn new() -> Self {
        Self {
            cpu_history: VecDeque::with_capacity(ENTROPY_WINDOW + 1),
            mem_history: VecDeque::with_capacity(ENTROPY_WINDOW + 1),
            proc_history: VecDeque::with_capacity(ENTROPY_WINDOW + 1),
            net_rx_history: VecDeque::with_capacity(ENTROPY_WINDOW + 1),
            net_tx_history: VecDeque::with_capacity(ENTROPY_WINDOW + 1),
            load_history: VecDeque::with_capacity(ENTROPY_WINDOW + 1),
            file_churn_history: VecDeque::with_capacity(ENTROPY_WINDOW + 1),
            entropy: 0.0,
            entropy_prev: 0.0,
            entropy_smoothed: 0.0,
            entropy_trend: 0.0,
            peak_entropy: 0.0,
            components: [0.0; 5],
        }
    }

    pub fn push(
        &mut self,
        cpu: f32,
        mem: f32,
        procs: u32,
        net_rx: u64,
        net_tx: u64,
        load: f32,
        file_churn_rate: f32,
    ) {
        Self::ring_push(&mut self.cpu_history, cpu);
        Self::ring_push(&mut self.mem_history, mem);
        Self::ring_push_u32(&mut self.proc_history, procs);
        Self::ring_push_u64(&mut self.net_rx_history, net_rx);
        Self::ring_push_u64(&mut self.net_tx_history, net_tx);
        Self::ring_push(&mut self.load_history, load);
        Self::ring_push(&mut self.file_churn_history, file_churn_rate.max(0.0));

        if self.cpu_history.len() < 4 {
            return;
        } // need minimum samples

        // Component 0: CPU variance (jittery CPU = high entropy)
        let cpu_var = Self::variance_f32(&self.cpu_history);
        // Normalize: stddev of 0-5% is calm, 15%+ is chaotic
        let cpu_norm = (cpu_var.sqrt() / 15.0).clamp(0.0, 1.0);

        // Component 1: Memory rate-of-change (rapid swings = high entropy)
        let mem_delta = Self::max_delta_f32(&self.mem_history);
        // Normalize: 0-2% delta is calm, 10%+ is chaotic
        let mem_norm = (mem_delta / 10.0).clamp(0.0, 1.0);

        // Component 2: Process + file churn (processes and file system activity)
        let proc_churn = Self::max_delta_u32(&self.proc_history);
        let file_churn = Self::max_f32(&self.file_churn_history);
        // Normalize: 0-5 churn is calm, 50+ is chaotic
        let proc_norm = (proc_churn as f32 / 50.0).clamp(0.0, 1.0);
        // Normalize: 0-2 file events/s is calm, 20+ events/s is chaotic
        let file_norm = (file_churn / 20.0).clamp(0.0, 1.0);
        let churn_norm = (proc_norm * 0.55 + file_norm * 0.45).clamp(0.0, 1.0);

        // Component 3: Network burstiness (coefficient of variation of deltas)
        let net_burst = Self::net_burstiness(&self.net_rx_history, &self.net_tx_history);
        // Already 0-1 from the function
        let net_norm = net_burst.clamp(0.0, 1.0);

        // Component 4: Load average deviation from rolling mean
        let load_dev = Self::deviation_from_mean(&self.load_history);
        // Normalize: 0-0.5 deviation is calm, 2.0+ is chaotic
        let load_norm = (load_dev / 2.0).clamp(0.0, 1.0);

        // Weighted composite — CPU and memory are primary signals
        let raw = cpu_norm * 0.30
            + mem_norm * 0.20
            + churn_norm * 0.15
            + net_norm * 0.20
            + load_norm * 0.15;

        self.entropy_prev = self.entropy;
        self.entropy = raw.clamp(0.0, 1.0);

        // Exponential smoothing (τ ≈ 2 seconds at 250ms sampling)
        self.entropy_smoothed += (self.entropy - self.entropy_smoothed) * 0.12;

        // Trend: positive = rising entropy, negative = falling
        self.entropy_trend = self.entropy_smoothed - self.entropy_prev;

        if self.entropy_smoothed > self.peak_entropy {
            self.peak_entropy = self.entropy_smoothed;
        }

        self.components = [cpu_norm, mem_norm, churn_norm, net_norm, load_norm];
    }

    fn ring_push(buf: &mut VecDeque<f32>, val: f32) {
        if buf.len() >= ENTROPY_WINDOW {
            buf.pop_front();
        }
        buf.push_back(val);
    }
    fn ring_push_u32(buf: &mut VecDeque<u32>, val: u32) {
        if buf.len() >= ENTROPY_WINDOW {
            buf.pop_front();
        }
        buf.push_back(val);
    }
    fn ring_push_u64(buf: &mut VecDeque<u64>, val: u64) {
        if buf.len() >= ENTROPY_WINDOW {
            buf.pop_front();
        }
        buf.push_back(val);
    }

    fn variance_f32(buf: &VecDeque<f32>) -> f32 {
        if buf.len() < 2 {
            return 0.0;
        }
        let n = buf.len() as f32;
        let mean = buf.iter().sum::<f32>() / n;
        buf.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n
    }

    fn max_delta_f32(buf: &VecDeque<f32>) -> f32 {
        if buf.len() < 2 {
            return 0.0;
        }
        buf.iter()
            .zip(buf.iter().skip(1))
            .map(|(a, b)| (b - a).abs())
            .fold(0.0_f32, f32::max)
    }

    fn max_delta_u32(buf: &VecDeque<u32>) -> u32 {
        if buf.len() < 2 {
            return 0;
        }
        buf.iter()
            .zip(buf.iter().skip(1))
            .map(|(a, b)| (*b as i64 - *a as i64).unsigned_abs() as u32)
            .max()
            .unwrap_or(0)
    }

    fn max_f32(buf: &VecDeque<f32>) -> f32 {
        buf.iter().copied().fold(0.0_f32, f32::max)
    }

    fn net_burstiness(rx: &VecDeque<u64>, tx: &VecDeque<u64>) -> f32 {
        // Compute coefficient of variation of combined throughput deltas
        if rx.len() < 3 {
            return 0.0;
        }
        let deltas: Vec<f64> = rx
            .iter()
            .zip(rx.iter().skip(1))
            .zip(tx.iter().zip(tx.iter().skip(1)))
            .map(|((rx0, rx1), (tx0, tx1))| {
                let d_rx = rx1.saturating_sub(*rx0) as f64;
                let d_tx = tx1.saturating_sub(*tx0) as f64;
                d_rx + d_tx
            })
            .collect();
        if deltas.is_empty() {
            return 0.0;
        }
        let n = deltas.len() as f64;
        let mean = deltas.iter().sum::<f64>() / n;
        if mean < 1.0 {
            return 0.0;
        } // negligible traffic
        let var = deltas.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
        let cv = var.sqrt() / mean; // coefficient of variation
        (cv as f32).clamp(0.0, 1.0)
    }

    fn deviation_from_mean(buf: &VecDeque<f32>) -> f32 {
        if buf.len() < 2 {
            return 0.0;
        }
        let n = buf.len() as f32;
        let mean = buf.iter().sum::<f32>() / n;
        let latest = *buf.back().unwrap_or(&0.0);
        (latest - mean).abs()
    }
}

// ═══════════════════════════════════════════════════════════════
//  System sampling utilities
// ═══════════════════════════════════════════════════════════════

/// Recursively count files under `root`, descending at most `max_depth` levels.
/// Returns 0 on any I/O error — this is best-effort telemetry.
pub fn count_files_fast(root: &str, max_depth: u32) -> u64 {
    fn walk(dir: &std::path::Path, depth: u32, max: u32) -> u64 {
        if depth > max {
            return 0;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut n = 0u64;
        for e in entries.flatten() {
            let ft = match e.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_file() {
                n += 1;
            } else if ft.is_dir() {
                // Skip hidden directories and common large dirs
                let name = e.file_name();
                let name_s = name.to_string_lossy();
                if !name_s.starts_with('.') && name_s != "node_modules" && name_s != "target" {
                    n += walk(&e.path(), depth + 1, max);
                }
            }
        }
        n
    }
    walk(std::path::Path::new(root), 0, max_depth)
}

/// Disk usage in GB for the filesystem containing `path`. Uses libc::statvfs.
pub fn disk_usage_gb(path: &str) -> (f32, f32) {
    use std::ffi::CString;
    let Ok(cpath) = CString::new(path) else {
        return (0.0, 0.0);
    };
    unsafe {
        let mut buf: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(cpath.as_ptr(), &mut buf) == 0 {
            let blk = buf.f_frsize as f64;
            let total = buf.f_blocks as f64 * blk / 1_073_741_824.0;
            let free = buf.f_bavail as f64 * blk / 1_073_741_824.0;
            ((total - free) as f32, total as f32)
        } else {
            (0.0, 0.0)
        }
    }
}

/// 1-minute load average from /proc/loadavg (Linux only).
pub fn load_average_1() -> f32 {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f32>().ok())
        })
        .unwrap_or(0.0)
}

// ═══════════════════════════════════════════════════════════════
//  Status formatting
// ═══════════════════════════════════════════════════════════════

pub fn build_system_update(
    cpu: f32,
    mem: f32,
    uptime: u64,
    mood: Mood,
    proc_count: u32,
    file_count: u64,
    disk_used: f32,
    load_avg: f32,
    weather_temp: Option<f32>,
    weather_desc: &str,
) -> String {
    let load_tag = if cpu > 75.0 {
        "CRITICAL"
    } else if cpu > 50.0 {
        "ELEVATED"
    } else if cpu > 25.0 {
        "ACTIVE"
    } else {
        "NOMINAL"
    };
    let mem_tag = if mem > 80.0 {
        "HIGH"
    } else if mem > 50.0 {
        "MODERATE"
    } else {
        "OK"
    };
    let (hr, mn, sc) = (uptime / 3600, (uptime % 3600) / 60, uptime % 60);
    let wx = if let Some(temp) = weather_temp {
        format!(" | WX {}C {}", temp as i32, weather_desc)
    } else {
        String::new()
    };
    format!(
        "SYS // {load_tag} CPU {cpu:.0}% | MEM {mem_tag} {mem:.0}% | PRC {proc_count} | F {file_count} | DSK {disk_used:.0}G | LA {load_avg:.1}{wx} | T+{hr:02}:{mn:02}:{sc:02} | {}",
        mood.label()
    )
}
