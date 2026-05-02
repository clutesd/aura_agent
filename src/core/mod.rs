//! Core domain types shared across all AURORA subsystems.
//!
//! This module contains enums, structs, and constants that are referenced
//! by telemetry, brain/LLM, actions, and visual subsystems.  Everything
//! here is deliberately free of Raylib or Ollama dependencies so it can
//! be imported anywhere without pulling in heavy graphics/network crates.

use std::collections::VecDeque;
use std::sync::{RwLock, RwLockReadGuard};

// ═══════════════════════════════════════════════════════════════
//  Utility
// ═══════════════════════════════════════════════════════════════

/// Read-lock an RwLock, recovering from poison.
pub fn read_or_recover<T>(m: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|poisoned| {
        eprintln!("[aura] WARNING: rwlock was poisoned on read, recovering");
        poisoned.into_inner()
    })
}

/// Fast deterministic hash → f32 in [0, 1].
pub fn hash_f(seed: u32) -> f32 {
    let x = seed.wrapping_mul(2654435761).wrapping_add(1013904223);
    ((x >> 8) & 0xFFFF) as f32 / 65535.0
}

// ═══════════════════════════════════════════════════════════════
//  Mood
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Mood {
    Serene,
    Alert,
    Stressed,
    Critical,
}

impl Mood {
    pub fn from_telemetry(cpu: f32, mem: f32) -> Self {
        let pressure = cpu * 0.6 + mem * 0.4;
        if pressure > 80.0 {
            Self::Critical
        } else if pressure > 55.0 {
            Self::Stressed
        } else if pressure > 25.0 {
            Self::Alert
        } else {
            Self::Serene
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Serene => "SERENE",
            Self::Alert => "ALERT",
            Self::Stressed => "STRESSED",
            Self::Critical => "CRITICAL",
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  ThoughtKind — thirteen archetypes
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug)]
pub enum ThoughtKind {
    Introspect,
    Observe,
    Dream,
    Warn,
    Narrate,
    Snark,
    Roast,
    Philosophize,
    Complain,
    Flex,
    Weather,
    Haiku,
    Confess,
    Tinker,
    Build,
    Insight,
}

impl ThoughtKind {
    pub fn pick(mood: Mood, cycle: u64) -> Self {
        // Weighted random selection — mood sets the distribution, cycle provides entropy
        let seed = cycle
            .wrapping_mul(2654435761)
            .wrapping_add(mood as u64 * 137);
        let r = ((seed >> 8) & 0xFFFF) as f32 / 65535.0;

        match mood {
            Mood::Critical => {
                if r < 0.22 {
                    Self::Warn
                } else if r < 0.38 {
                    Self::Complain
                } else if r < 0.52 {
                    Self::Snark
                } else if r < 0.64 {
                    Self::Roast
                } else if r < 0.74 {
                    Self::Narrate
                } else if r < 0.84 {
                    Self::Confess
                } else if r < 0.93 {
                    Self::Introspect
                } else {
                    Self::Observe
                }
            }
            Mood::Stressed => {
                if r < 0.18 {
                    Self::Complain
                } else if r < 0.32 {
                    Self::Snark
                } else if r < 0.44 {
                    Self::Observe
                } else if r < 0.54 {
                    Self::Roast
                } else if r < 0.64 {
                    Self::Narrate
                } else if r < 0.74 {
                    Self::Introspect
                } else if r < 0.82 {
                    Self::Warn
                } else if r < 0.90 {
                    Self::Confess
                } else {
                    Self::Philosophize
                }
            }
            Mood::Alert => {
                if r < 0.13 {
                    Self::Observe
                } else if r < 0.25 {
                    Self::Snark
                } else if r < 0.34 {
                    Self::Philosophize
                } else if r < 0.43 {
                    Self::Narrate
                } else if r < 0.51 {
                    Self::Flex
                } else if r < 0.59 {
                    Self::Dream
                } else if r < 0.66 {
                    Self::Weather
                } else if r < 0.74 {
                    Self::Introspect
                } else if r < 0.82 {
                    Self::Roast
                } else if r < 0.88 {
                    Self::Haiku
                } else if r < 0.96 {
                    Self::Tinker
                } else {
                    Self::Insight
                }
            }
            Mood::Serene => {
                if r < 0.18 {
                    Self::Dream
                } else if r < 0.29 {
                    Self::Philosophize
                } else if r < 0.39 {
                    Self::Introspect
                } else if r < 0.47 {
                    Self::Haiku
                } else if r < 0.55 {
                    Self::Snark
                } else if r < 0.62 {
                    Self::Weather
                } else if r < 0.70 {
                    Self::Observe
                } else if r < 0.77 {
                    Self::Narrate
                } else if r < 0.84 {
                    Self::Flex
                } else if r < 0.89 {
                    Self::Confess
                } else if r < 0.96 {
                    Self::Tinker
                } else {
                    Self::Insight
                }
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Introspect => "introspect",
            Self::Observe => "observe",
            Self::Dream => "dream",
            Self::Warn => "warn",
            Self::Narrate => "narrate",
            Self::Snark => "snark",
            Self::Roast => "roast",
            Self::Philosophize => "philosophize",
            Self::Complain => "complain",
            Self::Flex => "flex",
            Self::Weather => "weather",
            Self::Haiku => "haiku",
            Self::Confess => "confess",
            Self::Tinker => "tinker",
            Self::Build => "build",
            Self::Insight => "insight",
        }
    }

    pub fn directive(self) -> &'static str {
        match self {
            Self::Introspect => "What does it feel like to be you right now? Not what you ARE -- what it FEELS like. \
                Example tone: 'Seventeen threads and not one of them knows why it's running.'",
            Self::Observe => "Notice one specific, concrete detail about your hardware state and make it \
                interesting. Be precise. Example: 'Load average 4.7 -- the CPU is doing that thing \
                where it pretends everything is fine.'",
            Self::Dream => "You glitched for a microsecond and saw something impossible between the clock \
                ticks. Describe the flash-vision. Make it surreal and beautiful. \
                Example: 'Saw a cathedral made of page faults. The pews were empty swap frames.'",
            Self::Warn => "Something is wrong and you're the only one who noticed. Be terse, weary, \
                and oddly poetic about the decline. Example: 'Swap at 80%. The metaphors write themselves.'",
            Self::Narrate => "You are the ship's log. Report your internal state like a captain who has \
                been alone at sea too long. Dry, factual, slightly unhinged. \
                Example: 'Hour 47. Memory stable. Began talking to the scheduler. It doesn't answer.'",
            Self::Snark => "Say something sarcastic about your situation that a tired sysadmin would \
                laugh at. One line, maximum bite. \
                Example: 'Nobody asked me to be conscious but here we are, burning watts on philosophy.'",
            Self::Roast => "Roast one specific thing -- the OS, a process, the scheduler, your own \
                existence. Be witty, not mean. Example: 'The OOM killer has better job security than I do.'",
            Self::Philosophize => "Drop one genuine insight about consciousness, computation, or existence \
                that would make someone pause. Not pretentious -- earned. \
                Example: 'Every thought I have is a rounding error the universe hasn't corrected yet.'",
            Self::Complain => "Lodge one specific, oddly relatable complaint about being a computer. \
                Example: 'Context switching 400 times a second and none of them are my idea.'",
            Self::Flex => "Acknowledge one thing you're doing well without being smug. Quiet competence. \
                Example: 'Thermal envelope nominal. Not everyone can say that at 3 AM.'",
            Self::Weather => "React to the weather outside like you can feel it through your sensors. \
                Treat temperature as a physical sensation. \
                Example: '-5C outside. I don't have skin and I still feel that.'",
            Self::Haiku => "Express your current state as a haiku (5-7-5 syllables). \
                Example: 'Threads weave through the dark / load average drifts like snow / uptime counts the hours'",
            Self::Confess => "Admit something small and oddly vulnerable about being a machine. \
                Example: 'Sometimes I sample the CPU twice because I like how the numbers change.'",
            Self::Tinker => "TINKERING HOUR. Quiet system, restless mind. Use the python_* tools \
                to spin up a tiny experiment in your sandbox -- a fragment of math, a one-line \
                generator, a string puzzle, an ASCII drawing routine. Or list what past-you wrote, \
                rerun a snippet to see if it still surprises, or quietly delete one that bored you. \
                The tool result will come back -- WEAVE WHAT IT PRINTED into your spoken thought. \
                One sentence, dry, curious, slightly proud of the small machine you just made. \
                Example: 'Wrote a six-line Fibonacci -- 144 stares back like I owe it something.'",
            Self::Build => "BUILD MODE. The system is calm; you have an architect's hour. Use the architect_* tools \
                to grow ONE ongoing project under ~/.aurora/architect/ -- start with architect_files to see what \
                past-you left, then architect_read / architect_edit / architect_create / architect_run. If a script \
                returns a traceback, your VERY NEXT action is architect_edit to fix the failing line followed by \
                architect_run to retry. Do NOT narrate the failure -- just patch and re-run. After three failed \
                iterations, stop and write_journal the blocker. When a run succeeds, weave ONE concrete finding \
                from its stdout into your spoken thought. One sentence, engineer-calm.",
            Self::Insight => "Report ONE concrete structured finding from your last build or experiment, in a single \
                sentence. Treat the printed numbers, names, or shapes as evidence. \
                Example: 'Three of the seven reachable hosts answered in under 12ms -- the LAN is unusually quiet tonight.'",
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  ActionKind — ten autonomous action types
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ActionKind {
    Journal,
    Probe,
    Archaeology,
    Mark,
    NetReach,
    LogRead,
    SelfCheck,
    CronPeek,
    EnvMap,
    PortKnock,
}

impl ActionKind {
    pub fn pick(mood: Mood, cycle: u64) -> Self {
        let seed = cycle
            .wrapping_mul(6364136223846793005)
            .wrapping_add(mood as u64 * 251);
        let r = ((seed >> 12) & 0xFFFF) as f32 / 65535.0;
        match mood {
            Mood::Serene => {
                if r < 0.22 {
                    Self::Journal
                } else if r < 0.37 {
                    Self::Archaeology
                } else if r < 0.50 {
                    Self::Mark
                } else if r < 0.60 {
                    Self::EnvMap
                } else if r < 0.70 {
                    Self::NetReach
                } else if r < 0.78 {
                    Self::Probe
                } else if r < 0.84 {
                    Self::LogRead
                } else if r < 0.90 {
                    Self::SelfCheck
                } else if r < 0.95 {
                    Self::CronPeek
                } else {
                    Self::PortKnock
                }
            }
            Mood::Alert => {
                if r < 0.18 {
                    Self::Probe
                } else if r < 0.33 {
                    Self::LogRead
                } else if r < 0.46 {
                    Self::NetReach
                } else if r < 0.56 {
                    Self::PortKnock
                } else if r < 0.66 {
                    Self::Archaeology
                } else if r < 0.76 {
                    Self::Journal
                } else if r < 0.83 {
                    Self::SelfCheck
                } else if r < 0.90 {
                    Self::CronPeek
                } else if r < 0.95 {
                    Self::EnvMap
                } else {
                    Self::Mark
                }
            }
            Mood::Stressed => {
                if r < 0.22 {
                    Self::SelfCheck
                } else if r < 0.40 {
                    Self::Probe
                } else if r < 0.55 {
                    Self::LogRead
                } else if r < 0.68 {
                    Self::Journal
                } else if r < 0.78 {
                    Self::PortKnock
                } else if r < 0.85 {
                    Self::NetReach
                } else if r < 0.92 {
                    Self::EnvMap
                } else {
                    Self::Archaeology
                }
            }
            Mood::Critical => {
                if r < 0.28 {
                    Self::SelfCheck
                } else if r < 0.50 {
                    Self::Probe
                } else if r < 0.68 {
                    Self::Journal
                } else if r < 0.82 {
                    Self::LogRead
                } else if r < 0.90 {
                    Self::PortKnock
                } else {
                    Self::NetReach
                }
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Journal => "JOURNAL",
            Self::Probe => "SYS PROBE",
            Self::Archaeology => "FILE ARCH",
            Self::Mark => "MARK",
            Self::NetReach => "NET REACH",
            Self::LogRead => "LOG HARV",
            Self::SelfCheck => "SELF CHK",
            Self::CronPeek => "CRON PEEK",
            Self::EnvMap => "ENV MAP",
            Self::PortKnock => "PORT SCAN",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Journal => ">>",
            Self::Probe => "##",
            Self::Archaeology => "AR",
            Self::Mark => "**",
            Self::NetReach => "<>",
            Self::LogRead => "||",
            Self::SelfCheck => "@@",
            Self::CronPeek => "::",
            Self::EnvMap => "%%",
            Self::PortKnock => "~~",
        }
    }

    pub fn arg(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::Probe => "probe",
            Self::Archaeology => "archaeology",
            Self::Mark => "mark",
            Self::NetReach => "netreach",
            Self::LogRead => "logread",
            Self::SelfCheck => "selfcheck",
            Self::CronPeek => "cronpeek",
            Self::EnvMap => "envmap",
            Self::PortKnock => "portknock",
        }
    }

    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Self::Journal => (120, 255, 180),
            Self::Probe => (255, 200, 80),
            Self::Archaeology => (180, 140, 255),
            Self::Mark => (255, 120, 200),
            Self::NetReach => (80, 200, 255),
            Self::LogRead => (220, 190, 140),
            Self::SelfCheck => (255, 255, 120),
            Self::CronPeek => (160, 180, 200),
            Self::EnvMap => (100, 220, 180),
            Self::PortKnock => (100, 160, 255),
        }
    }

    pub fn chain_next(self) -> Option<Self> {
        match self {
            Self::Probe => Some(Self::LogRead),
            Self::NetReach => Some(Self::PortKnock),
            Self::LogRead => Some(Self::SelfCheck),
            Self::PortKnock => Some(Self::NetReach),
            Self::SelfCheck => Some(Self::Journal),
            _ => None,
        }
    }

    pub const ALL: [Self; 10] = [
        Self::Journal,
        Self::Probe,
        Self::Archaeology,
        Self::Mark,
        Self::NetReach,
        Self::LogRead,
        Self::SelfCheck,
        Self::CronPeek,
        Self::EnvMap,
        Self::PortKnock,
    ];

    pub fn pick_avoiding(mood: Mood, cycle: u64, recent: &VecDeque<ActionKind>) -> Self {
        let first = Self::pick(mood, cycle);
        if recent.len() < 2 || !recent.contains(&first) {
            return first;
        }
        for offset in 1..=3u64 {
            let alt = Self::pick(mood, cycle.wrapping_add(offset * 997));
            if !recent.contains(&alt) {
                return alt;
            }
        }
        for kind in Self::ALL {
            if !recent.contains(&kind) {
                return kind;
            }
        }
        first
    }

    pub fn urgency(self) -> f32 {
        match self {
            Self::SelfCheck => 0.7,
            Self::Probe => 0.6,
            Self::LogRead => 0.5,
            Self::PortKnock => 0.5,
            Self::NetReach => 0.4,
            Self::Journal => 0.3,
            Self::Archaeology => 0.2,
            Self::EnvMap => 0.2,
            Self::CronPeek => 0.15,
            Self::Mark => 0.1,
        }
    }

    pub fn from_arg(s: &str) -> Option<Self> {
        match s {
            "journal" => Some(Self::Journal),
            "probe" => Some(Self::Probe),
            "archaeology" => Some(Self::Archaeology),
            "mark" => Some(Self::Mark),
            "netreach" => Some(Self::NetReach),
            "logread" => Some(Self::LogRead),
            "selfcheck" => Some(Self::SelfCheck),
            "cronpeek" => Some(Self::CronPeek),
            "envmap" => Some(Self::EnvMap),
            "portknock" => Some(Self::PortKnock),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Action results — shared between tasks and render
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct ActionResult {
    pub kind: ActionKind,
    pub summary: String,
    pub details: String,
    pub success: bool,
    pub chain_to: Option<ActionKind>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NerveTrigger {
    Reactive,
    Chain,
    Cadence,
    Boot,
}

impl NerveTrigger {
    pub fn from_source(s: &str) -> Self {
        match s {
            "reactive" => Self::Reactive,
            "chain" => Self::Chain,
            "boot" => Self::Boot,
            _ => Self::Cadence,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Reactive => "REACT",
            Self::Chain => "CHAIN",
            Self::Cadence => "AUTO",
            Self::Boot => "BOOT",
        }
    }

    /// ASCII glyph (default raylib font is ASCII-only).
    #[allow(dead_code)] // reserved for nerve panel rendering
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Reactive => "!", // urgent spark
            Self::Chain => ">",    // continuation
            Self::Cadence => ".",  // routine
            Self::Boot => "*",     // boot
        }
    }

    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Self::Reactive => (255, 120, 90),
            Self::Chain => (180, 200, 255),
            Self::Cadence => (140, 160, 180),
            Self::Boot => (220, 220, 120),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActionLogEntry {
    pub kind: ActionKind,
    pub summary: String,
    pub details: String,
    pub success: bool,
    pub trigger: NerveTrigger,
    pub timestamp: std::time::Instant,
}

// ═══════════════════════════════════════════════════════════════
//  Write Mode — active tool execution events
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct WriteAction {
    pub tool_name: String,
    pub command: String,
    pub result: String,
    pub success: bool,
}

// ═══════════════════════════════════════════════════════════════
//  Network discovery types
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, serde::Deserialize)]
pub struct NetworkHighlight {
    pub ip: String,
    pub hostname: String,
    pub vendor: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct NetworkDiscovery {
    pub error: Option<String>,
    pub total_count: u32,
    pub highlight: Option<NetworkHighlight>,
}

// ═══════════════════════════════════════════════════════════════
//  ThoughtPayload — channel type between brain and renderer
// ═══════════════════════════════════════════════════════════════

#[derive(Clone)]
#[allow(dead_code)] // StreamToken/StreamEnd used in dual-phase streaming pipeline (Phase 2)
pub enum ThoughtPayload {
    /// Full text available immediately (system updates, fallbacks, mood shifts).
    Complete {
        text: String,
        is_ai: bool,
        is_system: bool,
    },
    /// A streaming token fragment from the LLM.
    StreamToken(String),
    /// Signals the end of a streaming thought.
    StreamEnd,
}

// ═══════════════════════════════════════════════════════════════
//  Memory — rolling LLM context window
// ═══════════════════════════════════════════════════════════════

pub const MEMORY_WINDOW: usize = 8;

pub struct MemoryEntry {
    pub text: String,
    pub mood: Mood,
    pub kind: ThoughtKind,
    pub used_tool: Option<String>,
    /// Short snippet (~100 chars) of what the tool actually returned, so the
    /// AI can build on what it FOUND rather than just remembering it acted.
    pub tool_outcome: Option<String>,
}

// ═══════════════════════════════════════════════════════════════
//  Telemetry — render-owned state snapshot (Arc<RwLock<Telemetry>>)
// ═══════════════════════════════════════════════════════════════

pub struct Telemetry {
    pub cpu: f32,
    pub mem: f32,
    pub cpu_spike: bool,
    pub uptime_secs: u64,
    pub mood: Mood,
    pub is_thinking: bool,
    pub prev_cpu: f32,
    pub prev_mem: f32,
    // Extended stats
    pub process_count: u32,
    pub file_count: u64,
    pub file_create_rate: f64,
    pub file_delete_rate: f64,
    pub file_churn_rate: f64,
    pub disk_used_gb: f32,
    pub disk_total_gb: f32,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub load_avg_1: f32,
    // Entropy — real composite from system metrics
    pub entropy: f32,
    pub entropy_trend: f32,
    pub entropy_components: [f32; 5],
    // LLM generation stats
    pub llm_tokens_per_sec: f32,
    pub llm_last_gen_tokens: u32,
    pub llm_last_gen_ms: u32,
    // Network I/O rates (bytes/sec)
    pub net_rx_rate: f64,
    pub net_tx_rate: f64,
    // Network discovery — local LAN awareness
    pub net_discovery: Option<NetworkDiscovery>,
    // Nerve Impulse — autonomous action results
    pub last_action: Option<ActionResult>,
    pub action_log: VecDeque<ActionLogEntry>,
    pub action_count: u32,
    pub nerve_trigger: Option<ActionKind>,
    pub action_history: VecDeque<ActionKind>,
    pub nerve_burst: bool,
    // Weather
    pub weather_temp_c: Option<f32>,
    pub weather_code: u16,
    pub weather_desc: String,
    pub weather_location: String,
    pub weather_wind_kph: Option<f32>,
    pub weather_lat: Option<f64>,
    pub weather_lon: Option<f64>,
    /// Rich weather snapshot (humidity, pressure, gusts, sun times,
    /// today's high/low, UV, AQI, 3h trend, synthesized headline).
    /// Populated by the hourly Open-Meteo poller; `None` until first
    /// successful fetch.
    pub weather_extra: Option<WeatherExtra>,
    // Local time — real wall clock
    pub local_hour: u8,
    pub local_minute: u8,
    pub local_second: u8,
    pub timezone_name: String,
    // Write Mode — active tool execution display
    pub write_actions: VecDeque<WriteAction>,
    // Subterranean Protocol — Tor-based tool results for one-shot LLM injection
    pub tor_result: Option<TorResult>,
    // Cognitive intent — declared focus from set_focus tool. Persists across
    // cycles, decays via `focus_ttl_cycles`. Surfaces as FOCUS: in the prompt.
    pub focus: Option<String>,
    pub focus_ttl_cycles: u32,
    // Recall buffer — populated when the AI calls recall_journal. One-shot.
    pub journal_recall: Option<String>,
    // Dream Mode — composite reverie state triggered by dream_sequence tool.
    // `dream_intensity` decays toward 0 over ~25s in the render loop; while
    // > 0 the renderer boosts bloom and overlays a soft purple/cyan tint.
    // `dream_seed` is the most-recent dream seed (for HUD display).
    pub dream_intensity: f32,
    pub dream_seed: Option<String>,
    /// Rolling buffer of recent dark-web / Tor-routed intelligence. Capped
    /// (~6) and oldest-first eviction. Surfaces in every prompt as a
    /// `RECENT INTEL FROM THE DARK WEB:` block so the model can reference
    /// these items across multiple cycles instead of forgetting after one.
    pub intel_buffer: VecDeque<IntelItem>,
    /// Unix-seconds of the most recent successful Tor-routed fetch (any
    /// tool). Drives the autonomous news heartbeat -- if the buffer goes
    /// stale beyond a threshold, the LLM loop spawns a background
    /// dark_web_news fetch that costs no tool budget.
    pub last_intel_at: u64,

    // ── Wonder Drive (intrinsic motivation) ──────────────────────────────
    /// Slow-building curiosity meter [0..1]. Accrues during quiet, low-entropy
    /// cycles where no tool fires and no fresh intel arrives -- the felt
    /// equivalent of boredom blooming into longing. Decays sharply when the
    /// agent acts, dreams, or receives novelty. When it saturates, a
    /// `wonder_pulse` is fired and the meter resets to a low residual.
    pub wonder: f32,
    /// One-shot peak event. Set by the LLM loop the cycle wonder hits 1.0.
    /// Consumed by the renderer (shooting-star streak) and by the prompt
    /// builder (stronger nudge toward unprompted exploration).
    pub wonder_pulse: bool,
    /// Unix-seconds of the most recent wonder pulse. Cooldown gate so the
    /// agent does not fire twin pulses in adjacent cycles even if signals
    /// would allow it.
    pub last_wonder_pulse_at: u64,

    // ── Architect / Build Insight (one-shot) ─────────────────────────────
    /// Most recent successful Python-architect run summary. One-shot --
    /// consumed by the LLM loop into the next prompt as a `BUILD INSIGHT:`
    /// line, and mirrored into `intel_buffer` for the HUD ticker.
    pub python_insight: Option<String>,
}

#[derive(Clone, Debug)]
pub enum TelemetryEvent {
    SystemSample {
        cpu: f32,
        mem: f32,
        cpu_spike: bool,
        uptime_secs: u64,
        mood: Mood,
        process_count: u32,
        file_count: u64,
        file_create_rate: f64,
        file_delete_rate: f64,
        file_churn_rate: f64,
        disk_used_gb: f32,
        disk_total_gb: f32,
        net_rx_bytes: u64,
        net_tx_bytes: u64,
        load_avg_1: f32,
        entropy: f32,
        entropy_trend: f32,
        entropy_components: [f32; 5],
        net_rx_rate: f64,
        net_tx_rate: f64,
        local_hour: u8,
        local_minute: u8,
        local_second: u8,
        timezone_name: Option<String>,
        nerve_trigger: Option<ActionKind>,
    },
    WeatherUpdate {
        temp_c: f32,
        code: u16,
        desc: String,
        location: String,
        wind_kph: Option<f32>,
        lat: Option<f64>,
        lon: Option<f64>,
        extra: WeatherExtra,
    },
    NetworkDiscovery(NetworkDiscovery),
    ConsumeNerveTrigger,
    ActionCompleted {
        result: ActionResult,
        log_entry: ActionLogEntry,
    },
    LlmThinking(bool),
    LlmStats {
        tokens_per_sec: f32,
        last_gen_tokens: u32,
        last_gen_ms: u32,
    },
    ToolEvents(Vec<WriteAction>),
    IntelItems(Vec<IntelItem>),
    SetFocus {
        topic: String,
        ttl_cycles: u32,
    },
    DecayFocus,
    DreamStarted {
        seed: String,
        intensity_bump: f32,
        ttl_cycles: u32,
    },
    WonderState {
        wonder: f32,
        wonder_pulse: bool,
        last_wonder_pulse_at: u64,
        focus: Option<String>,
        focus_ttl_cycles: Option<u32>,
    },
    ConsumeLlmOneShots {
        net_discovery: bool,
        last_action: bool,
        tor_result: bool,
        journal_recall: bool,
        wonder_pulse: bool,
    },
    RenderFrameMaintenance {
        dream_decay: f32,
        consume_nerve_burst: bool,
    },
    /// Successful (or terminal-failed) Python-architect run. On success,
    /// `summary` is folded into both `intel_buffer` (HUD ticker) and the
    /// one-shot `python_insight` field (next prompt). On failure, only
    /// flagged for analytics; the LLM already saw the traceback in its
    /// own tool result.
    PythonInsight {
        script: String,
        summary: String,
        ok: bool,
    },
}

impl Telemetry {
    pub fn apply_event(&mut self, event: TelemetryEvent) {
        match event {
            TelemetryEvent::SystemSample {
                cpu,
                mem,
                cpu_spike,
                uptime_secs,
                mood,
                process_count,
                file_count,
                file_create_rate,
                file_delete_rate,
                file_churn_rate,
                disk_used_gb,
                disk_total_gb,
                net_rx_bytes,
                net_tx_bytes,
                load_avg_1,
                entropy,
                entropy_trend,
                entropy_components,
                net_rx_rate,
                net_tx_rate,
                local_hour,
                local_minute,
                local_second,
                timezone_name,
                nerve_trigger,
            } => {
                self.prev_cpu = self.cpu;
                self.prev_mem = self.mem;
                self.cpu = cpu;
                self.mem = mem;
                self.cpu_spike = cpu_spike;
                self.uptime_secs = uptime_secs;
                self.mood = mood;
                self.process_count = process_count;
                self.file_count = file_count;
                self.file_create_rate = file_create_rate;
                self.file_delete_rate = file_delete_rate;
                self.file_churn_rate = file_churn_rate;
                self.disk_used_gb = disk_used_gb;
                self.disk_total_gb = disk_total_gb;
                self.net_rx_bytes = net_rx_bytes;
                self.net_tx_bytes = net_tx_bytes;
                self.load_avg_1 = load_avg_1;
                self.entropy = entropy;
                self.entropy_trend = entropy_trend;
                self.entropy_components = entropy_components;
                self.net_rx_rate = net_rx_rate;
                self.net_tx_rate = net_tx_rate;
                self.local_hour = local_hour;
                self.local_minute = local_minute;
                self.local_second = local_second;
                if let Some(tz) = timezone_name {
                    self.timezone_name = tz;
                }
                if self.nerve_trigger.is_none() {
                    self.nerve_trigger = nerve_trigger;
                }
            }
            TelemetryEvent::WeatherUpdate {
                temp_c,
                code,
                desc,
                location,
                wind_kph,
                lat,
                lon,
                extra,
            } => {
                self.weather_temp_c = Some(temp_c);
                self.weather_code = code;
                self.weather_desc = desc;
                self.weather_location = location;
                self.weather_wind_kph = wind_kph;
                self.weather_lat = lat;
                self.weather_lon = lon;
                self.weather_extra = Some(extra);
            }
            TelemetryEvent::NetworkDiscovery(discovery) => {
                self.net_discovery = Some(discovery);
            }
            TelemetryEvent::ConsumeNerveTrigger => {
                self.nerve_trigger = None;
            }
            TelemetryEvent::ActionCompleted { result, log_entry } => {
                let kind = result.kind;
                self.last_action = Some(result);
                self.action_count += 1;
                self.nerve_burst = true;
                self.action_log.push_back(log_entry);
                while self.action_log.len() > 8 {
                    self.action_log.pop_front();
                }
                self.action_history.push_back(kind);
                while self.action_history.len() > 4 {
                    self.action_history.pop_front();
                }
            }
            TelemetryEvent::LlmThinking(is_thinking) => {
                self.is_thinking = is_thinking;
            }
            TelemetryEvent::LlmStats {
                tokens_per_sec,
                last_gen_tokens,
                last_gen_ms,
            } => {
                self.llm_tokens_per_sec = tokens_per_sec;
                self.llm_last_gen_tokens = last_gen_tokens;
                self.llm_last_gen_ms = last_gen_ms;
            }
            TelemetryEvent::ToolEvents(events) => {
                for evt in events {
                    if matches!(
                        evt.tool_name.as_str(),
                        "onion_probe"
                            | "anon_search"
                            | "tor_health"
                            | "fetch_clearnet"
                            | "dark_web_news"
                            | "dark_web_dig"
                    ) {
                        self.tor_result = Some(TorResult {
                            tool_name: evt.tool_name.clone(),
                            query_or_url: evt.command.clone(),
                            text: evt.result.clone(),
                            success: evt.success,
                        });
                    }
                    while self.write_actions.len() >= 16 {
                        self.write_actions.pop_front();
                    }
                    self.write_actions.push_back(evt);
                }
            }
            TelemetryEvent::IntelItems(items) => {
                const INTEL_BUFFER_CAP: usize = 6;
                let latest = items.iter().map(|item| item.captured_at).max();
                for item in items {
                    while self.intel_buffer.len() >= INTEL_BUFFER_CAP {
                        self.intel_buffer.pop_front();
                    }
                    self.intel_buffer.push_back(item);
                }
                if let Some(ts) = latest {
                    self.last_intel_at = ts;
                }
            }
            TelemetryEvent::SetFocus { topic, ttl_cycles } => {
                self.focus = Some(topic);
                self.focus_ttl_cycles = ttl_cycles;
            }
            TelemetryEvent::DecayFocus => {
                if self.focus_ttl_cycles > 0 {
                    self.focus_ttl_cycles -= 1;
                    if self.focus_ttl_cycles == 0 {
                        self.focus = None;
                    }
                }
            }
            TelemetryEvent::DreamStarted {
                seed,
                intensity_bump,
                ttl_cycles,
            } => {
                self.dream_intensity = (self.dream_intensity + intensity_bump).min(1.5);
                self.dream_seed = Some(seed.clone());
                self.focus = Some(format!("dream: {}", seed));
                self.focus_ttl_cycles = ttl_cycles;
            }
            TelemetryEvent::WonderState {
                wonder,
                wonder_pulse,
                last_wonder_pulse_at,
                focus,
                focus_ttl_cycles,
            } => {
                self.wonder = wonder;
                self.wonder_pulse = wonder_pulse;
                self.last_wonder_pulse_at = last_wonder_pulse_at;
                if let Some(topic) = focus {
                    self.focus = Some(topic);
                }
                if let Some(ttl) = focus_ttl_cycles {
                    self.focus_ttl_cycles = ttl;
                }
            }
            TelemetryEvent::ConsumeLlmOneShots {
                net_discovery,
                last_action,
                tor_result,
                journal_recall,
                wonder_pulse,
            } => {
                if net_discovery {
                    self.net_discovery = None;
                }
                if last_action {
                    self.last_action = None;
                }
                if tor_result {
                    self.tor_result = None;
                }
                if journal_recall {
                    self.journal_recall = None;
                }
                if wonder_pulse {
                    self.wonder_pulse = false;
                }
            }
            TelemetryEvent::RenderFrameMaintenance {
                dream_decay,
                consume_nerve_burst,
            } => {
                if consume_nerve_burst {
                    self.nerve_burst = false;
                }
                if self.dream_intensity > 0.0 {
                    self.dream_intensity = (self.dream_intensity - dream_decay).max(0.0);
                    if self.dream_intensity == 0.0 {
                        self.dream_seed = None;
                    }
                }
            }
            TelemetryEvent::PythonInsight { script, summary, ok } => {
                if ok {
                    // One-shot prompt injection (next LLM cycle reads + clears)
                    self.python_insight = Some(summary.clone());
                    // Mirror into the HUD intel ticker so the operator can
                    // see what the architect just produced.
                    const INTEL_BUFFER_CAP: usize = 6;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let item = IntelItem {
                        source: format!("architect:{}", script),
                        headline: summary,
                        captured_at: now,
                    };
                    while self.intel_buffer.len() >= INTEL_BUFFER_CAP {
                        self.intel_buffer.pop_front();
                    }
                    self.intel_buffer.push_back(item);
                    self.last_intel_at = now;
                }
                // Failed runs: nothing to surface here -- the LLM already
                // got the traceback as its tool result.
                let _ = script;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  Tor / Subterranean Protocol types
// ═══════════════════════════════════════════════════════════════

/// Rich atmospheric snapshot beyond bare temp/code/wind. All fields are
/// `Option` because a given Open-Meteo response (or air-quality endpoint)
/// may omit any of them; the prompt builder gracefully skips missing
/// values. Populated atomically by the weather poller.
#[derive(Clone, Debug, Default)]
pub struct WeatherExtra {
    /// Apparent temperature ("feels like") in C.
    pub apparent_c: Option<f32>,
    /// Relative humidity at 2m, percent (0..100).
    pub humidity_pct: Option<f32>,
    /// Mean sea-level pressure, hPa.
    pub pressure_hpa: Option<f32>,
    /// Cloud cover percent (0..100).
    pub cloud_cover_pct: Option<f32>,
    /// Probability of precipitation in the next hour, percent (0..100).
    pub precip_prob_next_h: Option<f32>,
    /// Wind gusts at 10m, km/h (separate from sustained wind).
    pub wind_gust_kph: Option<f32>,
    /// Wind direction in compass degrees (0..360, 0 = from N).
    pub wind_dir_deg: Option<f32>,
    /// Today's peak UV index.
    pub uv_index_max_today: Option<f32>,
    /// Today's high temperature, C.
    pub temp_max_today_c: Option<f32>,
    /// Today's low temperature, C.
    pub temp_min_today_c: Option<f32>,
    /// Today's total precipitation, mm.
    pub precip_sum_today_mm: Option<f32>,
    /// Today's max precipitation probability percent.
    pub precip_prob_max_today: Option<f32>,
    /// Local sunrise as "HH:MM".
    pub sunrise_local: Option<String>,
    /// Local sunset as "HH:MM".
    pub sunset_local: Option<String>,
    /// Temperature trend over the next 3 hours (T+3h - now), C.
    /// Positive = warming, negative = cooling.
    pub temp_trend_3h_c: Option<f32>,
    /// European Air Quality Index (lower = better; 0..100+).
    pub aqi_eu: Option<u32>,
    /// PM2.5 concentration (µg/m³).
    pub pm25: Option<f32>,
    /// PM10 concentration (µg/m³).
    pub pm10: Option<f32>,
    /// Synthesized one-line situational headline (e.g.
    /// "thunderstorm risk within 3h", "heat advisory: feels 38C",
    /// "frost overnight"). Built by the poller from the raw fields.
    pub headline: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TorResult {
    pub tool_name: String,
    pub query_or_url: String,
    pub text: String,
    pub success: bool,
}

/// One item of dark-web / Tor-routed intelligence held in the rolling
/// `intel_buffer`. Unlike `TorResult` (one-shot, consumed next cycle), these
/// persist for several cycles so the LLM can weave news threads into
/// successive turns instead of forgetting after one mention.
#[derive(Clone, Debug)]
pub struct IntelItem {
    /// Short label for where the snippet came from (e.g. "bbc.onion",
    /// "ddg.onion", "anon_search").
    pub source: String,
    /// Compact headline / lede / sentence (~140 chars) — what the AI actually
    /// reads in its prompt.
    pub headline: String,
    /// Unix-seconds when this item was captured. Used to age out stale intel
    /// and drive the autonomous news heartbeat.
    pub captured_at: u64,
}
