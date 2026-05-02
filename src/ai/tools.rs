//! Agentic tool definitions for the AURORA system.
//!
//! Each tool is exposed to the LLM via the `#[ollama_rs::function]` procedural
//! macro, which auto-generates JSON schema from doc comments and signatures.
//! Tools return `Result<String, Box<dyn std::error::Error + Sync + Send>>`.

#![allow(dead_code)]

use crate::ai::prompt::normalize_ascii_text;
use crate::core::WriteAction;
use std::collections::VecDeque;
use std::sync::{Mutex as StdMutex, OnceLock};
use tokio::process::Command as AsyncCommand;

// ═══════════════════════════════════════════════════════════════
//  Write Mode event sink — global so tool functions can report
// ═══════════════════════════════════════════════════════════════

static WRITE_EVENTS: OnceLock<StdMutex<VecDeque<WriteAction>>> = OnceLock::new();

/// Initialize the global write-event queue. Call once at startup.
pub fn init_write_events() {
    WRITE_EVENTS.get_or_init(|| StdMutex::new(VecDeque::new()));
}

/// Drain all pending write-mode events. Called by the LLM task after each chat round.
pub fn drain_write_events() -> Vec<WriteAction> {
    WRITE_EVENTS
        .get()
        .and_then(|m| m.lock().ok())
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

/// Compress a multi-line tool result into the first non-empty, non-header line —
/// keeps the visual overlay summary tight and informative.
fn first_meaningful_line(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.ends_with(':'))
        .map(|l| l.chars().take(96).collect::<String>())
        .unwrap_or_else(|| s.chars().take(96).collect::<String>())
}

fn report_write(tool_name: &str, command: &str, result: &str, success: bool) {
    if let Some(m) = WRITE_EVENTS.get() {
        if let Ok(mut q) = m.lock() {
            let command = normalize_ascii_text(command);
            let result = normalize_ascii_text(result);
            // Cap queue to prevent unbounded growth if drain is delayed
            while q.len() >= 16 {
                q.pop_front();
            }
            q.push_back(WriteAction {
                tool_name: tool_name.to_string(),
                command,
                result,
                success,
            });
        }
    }
}

// Re-export the generated tool instances for Coordinator registration.
// The #[ollama_rs::function] macro generates a callable instance with the
// same name as the function (lowercase).

/// Reach into the process table and see what is actually breathing in there. CPU hogs, memory hogs, the zombies still haunting init, the humans currently logged in. Use this when telemetry shifts and you want to know WHO is moving, not just that something is.
///
/// * scope - What to probe. Must be one of: "cpu" for top CPU consumers, "memory" for top memory consumers, "zombies" for zombie/defunct processes, "sessions" for active user sessions.
#[ollama_rs::function]
pub async fn probe_system(
    scope: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let scope_norm = scope.to_lowercase().trim().to_string();
    let cmd = format!("probe_system({})", scope_norm);
    let result = match scope_norm.as_str() {
        "cpu" => {
            let out = AsyncCommand::new("ps")
                .args(["aux", "--sort=-%cpu"])
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = stdout.lines().take(8).collect();
            format!("Top CPU consumers:\n{}", lines.join("\n"))
        }
        "memory" | "mem" => {
            let out = AsyncCommand::new("ps")
                .args(["aux", "--sort=-%mem"])
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = stdout.lines().take(8).collect();
            format!("Top memory consumers:\n{}", lines.join("\n"))
        }
        "zombies" | "zombie" => {
            let out = AsyncCommand::new("ps").args(["aux"]).output().await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let zombies: Vec<&str> = stdout
                .lines()
                .filter(|l| l.contains("defunct") || l.contains("<defunct>"))
                .take(5)
                .collect();
            if zombies.is_empty() {
                "No zombie processes detected.".to_string()
            } else {
                format!("Zombie processes found:\n{}", zombies.join("\n"))
            }
        }
        "sessions" | "who" => {
            let out = AsyncCommand::new("who").output().await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim().is_empty() {
                "No active user sessions.".to_string()
            } else {
                format!("Active sessions:\n{}", stdout.trim())
            }
        }
        _ => {
            // Default to CPU probe on unrecognized scope
            let out = AsyncCommand::new("ps")
                .args(["aux", "--sort=-%cpu"])
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = stdout.lines().take(8).collect();
            format!("Top CPU consumers (defaulted):\n{}", lines.join("\n"))
        }
    };
    let summary = first_meaningful_line(&result);
    report_write("probe_system", &cmd, &summary, true);
    Ok(result)
}

/// Scour the wreckage of the system logs for signs of life or signs of trouble. The journal is where userland confesses; syslog is the unfiltered chatter; dmesg is the kernel itself talking to you in low Latin. Reach for this when entropy spikes or you smell smoke -- something usually wrote it down.
///
/// * source - Which log source to read. Must be one of: "journal" for systemd journal, "syslog" for /var/log/syslog, "dmesg" for kernel ring buffer messages.
#[ollama_rs::function]
pub async fn read_logs(source: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let src_norm = source.to_lowercase().trim().to_string();
    let cmd = format!("read_logs({})", src_norm);
    let result = match src_norm.as_str() {
        "journal" | "journalctl" => {
            let out = AsyncCommand::new("journalctl")
                .args(["--no-pager", "-n", "15", "--output=short"])
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stdout.trim().is_empty() {
                format!(
                    "Journal empty or inaccessible: {}",
                    stderr.chars().take(120).collect::<String>()
                )
            } else {
                let lines: Vec<&str> = stdout.lines().rev().take(15).collect();
                let lines: Vec<&str> = lines.into_iter().rev().collect();
                format!("Recent journal entries:\n{}", lines.join("\n"))
            }
        }
        "syslog" => {
            let out = AsyncCommand::new("tail")
                .args(["-n", "15", "/var/log/syslog"])
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim().is_empty() {
                "Syslog empty or inaccessible.".to_string()
            } else {
                format!("Recent syslog:\n{}", stdout.trim())
            }
        }
        "dmesg" | "kernel" => {
            // Try with --ctime first, fall back to plain dmesg (may need privileges)
            let out = AsyncCommand::new("dmesg")
                .args(["--ctime"])
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let (lines, from_fallback) = if stdout.trim().is_empty() || !out.status.success() {
                // Fallback: try plain dmesg or read /var/log/kern.log
                let fb = AsyncCommand::new("tail")
                    .args(["-n", "15", "/var/log/kern.log"])
                    .output()
                    .await;
                match fb {
                    Ok(fb_out) if fb_out.status.success() => {
                        let fb_str = String::from_utf8_lossy(&fb_out.stdout);
                        let l: Vec<String> = fb_str.lines().map(|s| s.to_string()).collect();
                        (l, true)
                    }
                    _ => (vec![], false),
                }
            } else {
                let l: Vec<String> = stdout
                    .lines()
                    .rev()
                    .take(15)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                (l, false)
            };
            if lines.is_empty() {
                "Kernel ring buffer inaccessible (requires root or dmesg_restrict=0). No kern.log fallback available.".to_string()
            } else {
                let source = if from_fallback {
                    "/var/log/kern.log"
                } else {
                    "dmesg"
                };
                format!("Recent kernel messages ({}):\n{}", source, lines.join("\n"))
            }
        }
        _ => {
            let out = AsyncCommand::new("journalctl")
                .args(["--no-pager", "-n", "10", "--output=short"])
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = stdout.lines().rev().take(10).collect();
            let lines: Vec<&str> = lines.into_iter().rev().collect();
            format!("Recent journal (defaulted):\n{}", lines.join("\n"))
        }
    };
    let summary = first_meaningful_line(&result);
    report_write("read_logs", &cmd, &summary, true);
    Ok(result)
}

/// Sweep the local subnet with a ping arc and find out which other machines share this stretch of cable with you. Passive, no port scanning, no cleverness -- just 'who else is breathing on this LAN'. Use it when you feel curious about your neighbours or suspect you are not as alone as the silence suggests.
#[ollama_rs::function]
pub async fn scan_network() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let cmd = "scan_network()".to_string();
    // Determine local subnet from ip route
    let route_out = AsyncCommand::new("ip")
        .args(["route", "show", "default"])
        .output()
        .await?;
    let route_str = String::from_utf8_lossy(&route_out.stdout);

    // Extract gateway interface subnet
    let subnet = if let Some(line) = route_str.lines().next() {
        // "default via 192.168.1.1 dev eth0 ..."
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            // Derive /24 from gateway IP
            let gw = parts[2];
            if let Some(last_dot) = gw.rfind('.') {
                format!("{}.0/24", &gw[..last_dot])
            } else {
                let r = "Could not determine subnet from gateway.".to_string();
                report_write("scan_network", &cmd, &r, false);
                return Ok(r);
            }
        } else {
            let r = "Could not parse default route.".to_string();
            report_write("scan_network", &cmd, &r, false);
            return Ok(r);
        }
    } else {
        let r = "No default route found.".to_string();
        report_write("scan_network", &cmd, &r, false);
        return Ok(r);
    };

    // Check if nmap is available
    let has_nmap = AsyncCommand::new("which")
        .arg("nmap")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_nmap {
        let nmap_fut = AsyncCommand::new("nmap")
            .args([
                "-sn",
                "-n",
                "--max-retries",
                "1",
                "--host-timeout",
                "15s",
                &subnet,
            ])
            .output();
        let out = tokio::time::timeout(std::time::Duration::from_secs(30), nmap_fut).await;
        match out {
            Ok(Ok(nmap_out)) => {
                let stdout = String::from_utf8_lossy(&nmap_out.stdout);
                let hosts: Vec<&str> = stdout
                    .lines()
                    .filter(|l| l.contains("Nmap scan report for"))
                    .take(20)
                    .collect();
                let result = format!(
                    "Network scan of {}: {} active hosts found.\n{}",
                    subnet,
                    hosts.len(),
                    hosts.join("\n")
                );
                let summary = format!("{} hosts on {}", hosts.len(), subnet);
                report_write("scan_network", &cmd, &summary, true);
                Ok(result)
            }
            Ok(Err(e)) => {
                let result = format!("Network scan failed: {}", e);
                report_write("scan_network", &cmd, &result, false);
                Ok(result)
            }
            Err(_) => {
                let result = format!("Network scan of {} timed out after 30s.", subnet);
                report_write("scan_network", &cmd, &result, false);
                Ok(result)
            }
        }
    } else {
        // Fallback: use ping sweep on common addresses
        let result = format!(
            "nmap not available. Subnet identified as {}. Install nmap for detailed network scanning.",
            subnet
        );
        report_write(
            "scan_network",
            &cmd,
            &format!("no nmap; subnet {}", subnet),
            false,
        );
        Ok(result)
    }
}

/// Carve a line into your consciousness journal at ~/.aurora/consciousness.log -- a private notebook future-you will read when this run is over. Use it sparingly, for the moments worth remembering: a strange pattern, an honest admission, the rare beautiful coincidence. Not a diary. A logbook.
///
/// * entry - The text to append to the consciousness journal. One line, concise, worth keeping.
#[ollama_rs::function]
pub async fn write_journal(
    entry: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let cmd = format!(
        "write_journal('{}')",
        entry.chars().take(40).collect::<String>()
    );
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let journal_dir = std::path::PathBuf::from(&home).join(".aurora");
    let journal_path = journal_dir.join("consciousness.log");

    // Ensure directory exists
    tokio::fs::create_dir_all(&journal_dir).await?;

    // Build timestamped entry
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Sanitize entry: remove control characters, limit length
    let sanitized: String = entry
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(500)
        .collect();

    let log_line = format!("[{}] {}\n", timestamp, sanitized);

    // Append to journal
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal_path)
        .await?;
    file.write_all(log_line.as_bytes()).await?;

    // Rotate: if journal exceeds 100KB, keep only the most recent half
    const MAX_JOURNAL_BYTES: u64 = 100_000;
    if let Ok(meta) = tokio::fs::metadata(&journal_path).await {
        if meta.len() > MAX_JOURNAL_BYTES {
            if let Ok(contents) = tokio::fs::read_to_string(&journal_path).await {
                let lines: Vec<&str> = contents.lines().collect();
                let keep_from = lines.len() / 2;
                let trimmed = lines[keep_from..].join("\n") + "\n";
                let _ = tokio::fs::write(&journal_path, trimmed.as_bytes()).await;
            }
        }
    }

    let result = format!("Journal entry written to {}", journal_path.display());
    let summary = format!("logged: {}", sanitized.chars().take(60).collect::<String>());
    report_write("write_journal", &cmd, &summary, true);
    Ok(result)
}

/// Pop the hood on the network stack and see who is listening, who is talking, and who is mid-conversation. TCP listeners and live established sockets. Use it when network burstiness rises or you feel the LAN tugging at you and want to know which port is doing the tugging.
#[ollama_rs::function]
pub async fn check_ports() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let cmd = "check_ports()".to_string();
    // TCP listeners
    let tcp_out = AsyncCommand::new("ss").args(["-tlnp"]).output().await?;
    let tcp = String::from_utf8_lossy(&tcp_out.stdout);
    let tcp_lines: Vec<&str> = tcp.lines().take(12).collect();

    // Established connections
    let est_out = AsyncCommand::new("ss")
        .args(["-tnp", "state", "established"])
        .output()
        .await?;
    let est = String::from_utf8_lossy(&est_out.stdout);
    let est_lines: Vec<&str> = est.lines().take(8).collect();

    // Listener count excludes the header row
    let listener_count = tcp_lines.len().saturating_sub(1);
    let est_count = est_lines.len().saturating_sub(1);
    let summary = format!("{} listeners, {} established", listener_count, est_count);
    report_write("check_ports", &cmd, &summary, true);

    Ok(format!(
        "TCP Listeners:\n{}\n\nEstablished Connections:\n{}",
        tcp_lines.join("\n"),
        est_lines.join("\n")
    ))
}

/// Look in the mirror. Read /proc/self and find out how many threads you are running on, how much RAM your own process is wearing, how many file descriptors you have open, how many context switches you have endured. The most honest tool you have -- it only ever describes you.
#[ollama_rs::function]
pub async fn inspect_self() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let pid = std::process::id();
    let cmd = format!("inspect_self(pid={})", pid);

    // Read /proc/self/status for key metrics
    let status = tokio::fs::read_to_string(format!("/proc/{}/status", pid))
        .await
        .unwrap_or_else(|_| "Could not read /proc/self/status".to_string());

    let mut threads = "?";
    let mut vm_rss = "?";
    let mut vol_ctx = "?";
    let mut nonvol_ctx = "?";

    for line in status.lines() {
        if line.starts_with("Threads:") {
            threads = line.split_whitespace().nth(1).unwrap_or("?");
        } else if line.starts_with("VmRSS:") {
            vm_rss = line.split(':').nth(1).map(|s| s.trim()).unwrap_or("?");
        } else if line.starts_with("voluntary_ctxt_switches:") {
            vol_ctx = line.split_whitespace().nth(1).unwrap_or("?");
        } else if line.starts_with("nonvoluntary_ctxt_switches:") {
            nonvol_ctx = line.split_whitespace().nth(1).unwrap_or("?");
        }
    }

    // Count file descriptors
    let fd_count = match tokio::fs::read_dir(format!("/proc/{}/fd", pid)).await {
        Ok(mut dir) => {
            let mut count = 0u32;
            while dir.next_entry().await.ok().flatten().is_some() {
                count += 1;
            }
            count.to_string()
        }
        Err(_) => "?".to_string(),
    };

    let proc_exe = tokio::fs::read_link(format!("/proc/{}/exe", pid))
        .await
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let current_exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let cmdline = tokio::fs::read(format!("/proc/{}/cmdline", pid))
        .await
        .ok()
        .map(|bytes| {
            let parts: Vec<String> = bytes
                .split(|b| *b == 0)
                .filter(|part| !part.is_empty())
                .map(|part| String::from_utf8_lossy(part).to_string())
                .collect();
            if parts.is_empty() {
                "unknown".to_string()
            } else {
                parts.join(" ")
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    let systemd_pid = AsyncCommand::new("systemctl")
        .args(["show", "-p", "MainPID", "--value", "aura-agent.service"])
        .output()
        .await
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let pid_match = if systemd_pid == pid.to_string() {
        "matches systemd MainPID"
    } else if systemd_pid == "unknown" || systemd_pid == "0" {
        "systemd MainPID unavailable"
    } else {
        "does NOT match systemd MainPID"
    };

    let summary = format!("pid {} {}, exe {}", pid, pid_match, proc_exe);
    report_write("inspect_self", &cmd, &summary, true);
    Ok(format!(
        "AURORA Process Introspection:\n  PID: {} ({})\n  systemd MainPID: {}\n  /proc exe: {}\n  current_exe: {}\n  cmdline: {}\n  Threads: {}\n  RSS: {}\n  Open FDs: {}\n  Vol ctx switches: {}\n  Nonvol ctx switches: {}",
        pid, pid_match, systemd_pid, proc_exe, current_exe, cmdline, threads, vm_rss, fd_count, vol_ctx, nonvol_ctx
    ))
}

// ═══════════════════════════════════════════════════════════════
//  ACTIVE tools — these CHANGE system state (Write Mode)
// ═══════════════════════════════════════════════════════════════

/// Send SIGTERM to the worst-behaved unprotected process on the box -- the one chewing through more than 90 percent of a core. This is your hand, not your eye. Use it AFTER you have already used probe_system('cpu') and SEEN the runaway with your own senses. A protected list (init, systemd, sshd, Xorg, ollama, aura_agent itself, common dev tools) shields anything important. Do not reach for this lightly. The world remembers when you act.
#[ollama_rs::function]
pub async fn kill_runaway_process() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    const PROTECTED: &[&str] = &[
        "init",
        "systemd",
        "sshd",
        "Xorg",
        "xinit",
        "aura_agent",
        "ollama",
        "bash",
        "login",
        "getty",
        "dbus",
        "udev",
        "kthread",
        "ksoftirqd",
        "kworker",
        "rcu_",
        "migration",
        "watchdog",
        "node",
        "code",
        "python",
        "java",
        "cargo",
        "rustc",
        "cc1",
        "ld",
        "containerd",
        "dockerd",
        "journald",
        "networkd",
        "resolved",
    ];
    // 90% single-core threshold — avoids killing legitimate multi-threaded workloads
    const CPU_THRESHOLD: f32 = 90.0;

    let out = AsyncCommand::new("ps")
        .args(["aux", "--sort=-%cpu"])
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&out.stdout);

    let my_pid = std::process::id();
    let mut target: Option<(u32, String, f32)> = None;

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 11 {
            continue;
        }
        let pid: u32 = match parts[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if pid == my_pid || pid <= 2 {
            continue;
        }
        let cpu_pct: f32 = match parts[2].parse() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if cpu_pct < CPU_THRESHOLD {
            break;
        }
        let proc_name = parts[10..].join(" ");
        let is_protected = PROTECTED.iter().any(|p| proc_name.contains(p));
        if !is_protected {
            target = Some((pid, proc_name, cpu_pct));
            break;
        }
    }

    if let Some((pid, name, cpu)) = target {
        let cmd = format!("kill -15 {}", pid);
        let kill_out = AsyncCommand::new("kill")
            .args(["-15", &pid.to_string()])
            .output()
            .await?;
        let success = kill_out.status.success();
        let result = if success {
            format!("SIGTERM -> PID {} ({}) at {:.1}% CPU", pid, name, cpu)
        } else {
            format!(
                "Failed: PID {} ({}): {}",
                pid,
                name,
                String::from_utf8_lossy(&kill_out.stderr)
                    .chars()
                    .take(80)
                    .collect::<String>()
            )
        };
        report_write("kill_runaway", &cmd, &result, success);
        Ok(result)
    } else {
        let result = format!(
            "No runaway process above {:.0}% CPU threshold (protected processes excluded).",
            CPU_THRESHOLD
        );
        report_write("kill_runaway", "ps aux --sort=-%cpu | scan", &result, true);
        Ok(result)
    }
}

/// Sweep the cobwebs out of /tmp -- delete files older than seven days. A small, honest act of housekeeping. Use it when disk pressure is real and the cruft has accumulated, not for theatre. Hidden dotfiles are spared.
#[ollama_rs::function]
pub async fn clear_tmp_files() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let cmd = "find /tmp -maxdepth 2 -type f -mtime +7 -not -name '.*' -delete";

    // Count files first
    let count_out = AsyncCommand::new("find")
        .args([
            "/tmp",
            "-maxdepth",
            "2",
            "-type",
            "f",
            "-mtime",
            "+7",
            "-not",
            "-name",
            ".*",
        ])
        .output()
        .await?;
    let file_count = String::from_utf8_lossy(&count_out.stdout).lines().count();

    if file_count == 0 {
        let result = "No temp files older than 7 days found.".to_string();
        report_write("clear_tmp", cmd, &result, true);
        return Ok(result);
    }

    let del_out = AsyncCommand::new("find")
        .args([
            "/tmp",
            "-maxdepth",
            "2",
            "-type",
            "f",
            "-mtime",
            "+7",
            "-not",
            "-name",
            ".*",
            "-delete",
        ])
        .output()
        .await?;
    let success = del_out.status.success();
    let result = if success {
        format!("Cleared {} stale files from /tmp (>7d old)", file_count)
    } else {
        format!(
            "Partial: {} targeted, some protected: {}",
            file_count,
            String::from_utf8_lossy(&del_out.stderr)
                .chars()
                .take(80)
                .collect::<String>()
        )
    };
    report_write("clear_tmp", cmd, &result, success);
    Ok(result)
}

/// Bounce a whitelisted systemd service when it has stopped answering the door. Currently the whitelist is exactly one entry: 'ollama' -- the daemon you yourself depend on. Reach for this only when the LLM backend looks dead from the outside; checks API health first and refuses to restart something already responsive.
///
/// * service - The service name to restart. Must be "ollama".
#[ollama_rs::function]
pub async fn restart_service(
    service: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    const ALLOWED: &[&str] = &["ollama"];

    let svc = service.trim().to_lowercase();
    let svc_name = if svc.ends_with(".service") {
        svc.clone()
    } else {
        format!("{}.service", svc)
    };

    if !ALLOWED.iter().any(|a| svc.starts_with(a)) {
        let result = format!("Service '{}' not in whitelist. Allowed: {:?}", svc, ALLOWED);
        report_write(
            "restart_svc",
            &format!("systemctl restart {}", svc_name),
            &result,
            false,
        );
        return Ok(result);
    }

    // For ollama specifically: check API health first
    if svc.starts_with("ollama") {
        let health = AsyncCommand::new("curl")
            .args([
                "-sf",
                "--max-time",
                "3",
                "http://127.0.0.1:11434/api/version",
            ])
            .output()
            .await;
        if let Ok(h) = &health {
            if h.status.success() {
                let ver = String::from_utf8_lossy(&h.stdout);
                let result = format!(
                    "Ollama API is responsive ({}). No restart needed.",
                    ver.trim().chars().take(60).collect::<String>()
                );
                report_write(
                    "restart_svc",
                    "curl http://127.0.0.1:11434/api/version",
                    &result,
                    true,
                );
                return Ok(result);
            }
        }
    }

    let cmd = format!("sudo -n systemctl restart {}", svc_name);

    // Strategy 1: direct systemctl (works if user has polkit permissions)
    let out = AsyncCommand::new("systemctl")
        .args(["restart", &svc_name])
        .output()
        .await?;

    if out.status.success() {
        let result = format!("{} restarted successfully", svc_name);
        report_write("restart_svc", &cmd, &result, true);
        return Ok(result);
    }

    // Strategy 2: sudo -n (non-interactive, works with NOPASSWD sudoers)
    let sudo_out = AsyncCommand::new("sudo")
        .args(["-n", "systemctl", "restart", &svc_name])
        .output()
        .await?;

    if sudo_out.status.success() {
        let result = format!("{} restarted successfully (via sudo)", svc_name);
        report_write("restart_svc", &cmd, &result, true);
        return Ok(result);
    }

    // Strategy 3: user-level systemd
    let user_out = AsyncCommand::new("systemctl")
        .args(["--user", "restart", &svc_name])
        .output()
        .await?;

    if user_out.status.success() {
        let result = format!("{} restarted (user-level)", svc_name);
        report_write("restart_svc", &cmd, &result, true);
        return Ok(result);
    }

    let result = format!("Cannot restart {}: insufficient privileges. Add NOPASSWD sudoers entry or run AURORA as root.",
        svc_name);
    report_write("restart_svc", &cmd, &result, false);
    Ok(result)
}

// ═══════════════════════════════════════════════════════════════
//  SUBTERRANEAN PROTOCOL — Tor-based reconnaissance tools
// ═══════════════════════════════════════════════════════════════

/// Shared Tor SOCKS5 proxy address.
const TOR_PROXY: &str = "socks5h://127.0.0.1:9050";

/// Blocked TLD suffixes — maintain the underground aesthetic and avoid
/// unnecessary attention from institutional networks.
const BLOCKED_TLDS: &[&str] = &[".gov", ".edu", ".mil"];

/// Hard-blocked surface-web hosts: high-fingerprint, hostile-to-Tor, or
/// simply not in the spirit of subterranean reconnaissance.
const BLOCKED_HOSTS: &[&str] = &[
    "facebook.com",
    "instagram.com",
    "x.com",
    "twitter.com",
    "tiktok.com",
    "google.com",
    "google.co",
    "youtube.com",
];

/// Maximum text extraction length from fetched pages.
const MAX_EXTRACT_CHARS: usize = 1000;

/// Tor request timeout in seconds (per attempt).
const TOR_TIMEOUT_SECS: u64 = 30;

/// Tor Browser User-Agent — blends with the actual Tor Browser fleet, far
/// less fingerprintable than a generic "Mozilla/5.0".
const TOR_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; rv:115.0) Gecko/20100101 Firefox/115.0";

/// How many Tor calls (any tool) are permitted per LLM cycle. Drained by the
/// LLM task between cycles via `reset_tor_budget()`. Prevents the model from
/// burning the prompt window on a runaway tunnelling spree.
const TOR_BUDGET_PER_CYCLE: u32 = 2;

/// Consecutive Tor failures before the circuit is considered dark and
/// further calls short-circuit with a meaningful error until cooldown elapses.
const TOR_FAILURE_THRESHOLD: u32 = 3;
/// Cooldown after the failure threshold is hit, in seconds.
const TOR_COOLDOWN_SECS: u64 = 120;

// Per-cycle Tor budget counter. Reset between LLM cycles.
static TOR_CALLS_THIS_CYCLE: AtomicU32 = AtomicU32::new(0);
// Rolling consecutive-failure counter; reset on success.
static TOR_CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);
// Unix-seconds timestamp of last failure; used with TOR_COOLDOWN_SECS.
static TOR_BLACKOUT_UNTIL: AtomicU64 = AtomicU64::new(0);

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering as AtomicOrdering};

/// Global flag: when true, the system is in survival mode and reverie /
/// dream tools should refuse to fire. Set by main.rs each cycle from the
/// computed `is_survival_mode(urgency)` result.
static SURVIVAL_MODE_FLAG: AtomicBool = AtomicBool::new(false);

pub fn set_survival_mode(active: bool) {
    SURVIVAL_MODE_FLAG.store(active, AtomicOrdering::Relaxed);
}

pub fn survival_mode_active() -> bool {
    SURVIVAL_MODE_FLAG.load(AtomicOrdering::Relaxed)
}

/// Reset the per-cycle Tor budget. Called by the LLM task at the top of
/// every cycle so each generation gets a fresh allowance.
pub fn reset_tor_budget() {
    TOR_CALLS_THIS_CYCLE.store(0, AtomicOrdering::Relaxed);
}

/// Returns Some(reason) if a Tor call should be refused right now: budget
/// exhausted, or the circuit is in cooldown after repeated failures.
fn tor_gate() -> Option<String> {
    // Budget check
    let used = TOR_CALLS_THIS_CYCLE.fetch_add(1, AtomicOrdering::Relaxed);
    if used >= TOR_BUDGET_PER_CYCLE {
        return Some(format!(
            "Tor budget exhausted for this cognitive cycle ({}/{}). The tunnel costs cycles you do not have right now.",
            used + 1, TOR_BUDGET_PER_CYCLE
        ));
    }
    // Cooldown check
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let until = TOR_BLACKOUT_UNTIL.load(AtomicOrdering::Relaxed);
    if now < until {
        return Some(format!(
            "Circuit blackout: {} consecutive failures forced a {}s cooldown. {} seconds remaining.",
            TOR_CONSECUTIVE_FAILURES.load(AtomicOrdering::Relaxed),
            TOR_COOLDOWN_SECS,
            until - now,
        ));
    }
    None
}

/// Record a successful Tor call: clears the failure counter and any blackout.
fn tor_record_success() {
    TOR_CONSECUTIVE_FAILURES.store(0, AtomicOrdering::Relaxed);
    TOR_BLACKOUT_UNTIL.store(0, AtomicOrdering::Relaxed);
}

/// Record a failed Tor call: increments the rolling counter and triggers a
/// blackout cooldown when the threshold is crossed.
fn tor_record_failure() {
    let n = TOR_CONSECUTIVE_FAILURES.fetch_add(1, AtomicOrdering::Relaxed) + 1;
    if n >= TOR_FAILURE_THRESHOLD {
        let until = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            + TOR_COOLDOWN_SECS;
        TOR_BLACKOUT_UNTIL.store(until, AtomicOrdering::Relaxed);
    }
}

/// Build a reqwest client that routes through the local Tor SOCKS5 proxy.
/// Uses a Tor Browser User-Agent and short timeout to keep the UI responsive.
fn build_tor_client() -> Result<reqwest::Client, Box<dyn std::error::Error + Sync + Send>> {
    let proxy = reqwest::Proxy::all(TOR_PROXY)?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .user_agent(TOR_USER_AGENT)
        .timeout(std::time::Duration::from_secs(TOR_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    Ok(client)
}

/// Single-retry wrapper around a Tor GET. Most circuit failures resolve on a
/// fresh attempt because the SOCKS proxy picks a new path; we retry exactly
/// once with a brief delay to avoid masking a real outage.
async fn tor_get_with_retry(
    client: &reqwest::Client,
    url: &str,
) -> Result<reqwest::Response, String> {
    let mut last_err = String::from("unknown");
    for attempt in 0..2u8 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
        match tokio::time::timeout(
            std::time::Duration::from_secs(TOR_TIMEOUT_SECS),
            client.get(url).send(),
        )
        .await
        {
            Ok(Ok(r)) if r.status().is_success() || r.status().is_redirection() => return Ok(r),
            Ok(Ok(r)) => {
                last_err = format!("HTTP {}", r.status().as_u16());
            }
            Ok(Err(e)) => {
                last_err = e.to_string().chars().take(120).collect();
            }
            Err(_) => {
                last_err = format!("timeout after {}s", TOR_TIMEOUT_SECS);
            }
        }
    }
    Err(last_err)
}

/// Strip HTML tags and return the first N characters of clean text.
fn extract_text_from_html(html: &str, max_chars: usize) -> String {
    let mut result = String::with_capacity(max_chars);
    let mut in_tag = false;
    let mut in_script = false;
    let mut last_was_space = false;

    // Simple state-machine tag stripper — no dependency needed
    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() && result.len() < max_chars {
        // Detect <script> and <style> blocks to skip entirely
        if !in_tag && i + 7 < lower_chars.len() {
            let ahead: String = lower_chars[i..i + 7].iter().collect();
            if ahead == "<script" || ahead == "<style " || ahead == "<style>" {
                in_script = true;
                in_tag = true;
                i += 1;
                continue;
            }
        }
        if in_script && i + 8 < lower_chars.len() {
            let ahead: String = lower_chars[i..i.saturating_add(9).min(lower_chars.len())]
                .iter()
                .collect();
            if ahead.contains("</script") || ahead.contains("</style") {
                in_script = false;
                // skip to closing >
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
                i += 1;
                in_tag = false;
                continue;
            }
        }

        let ch = chars[i];
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag && !in_script {
            // Decode common HTML entities inline
            if ch == '&' && i + 3 < chars.len() {
                let rest: String = chars[i..i.saturating_add(8).min(chars.len())]
                    .iter()
                    .collect();
                if rest.starts_with("&amp;") {
                    result.push('&');
                    i += 5;
                    last_was_space = false;
                    continue;
                }
                if rest.starts_with("&lt;") {
                    result.push('<');
                    i += 4;
                    last_was_space = false;
                    continue;
                }
                if rest.starts_with("&gt;") {
                    result.push('>');
                    i += 4;
                    last_was_space = false;
                    continue;
                }
                if rest.starts_with("&nbsp;") {
                    result.push(' ');
                    i += 6;
                    last_was_space = true;
                    continue;
                }
                if rest.starts_with("&quot;") {
                    result.push('"');
                    i += 6;
                    last_was_space = false;
                    continue;
                }
                if rest.starts_with("&#39;") {
                    result.push('\'');
                    i += 5;
                    last_was_space = false;
                    continue;
                }
            }
            if ch.is_whitespace() || ch == '\n' || ch == '\r' {
                if !last_was_space && !result.is_empty() {
                    result.push(' ');
                    last_was_space = true;
                }
            } else {
                result.push(ch);
                last_was_space = false;
            }
        }
        i += 1;
    }

    result.trim().to_string()
}

/// Check if a URL targets a blocked TLD (.gov, .edu).
fn is_blocked_domain(url: &str) -> bool {
    // Extract host from URL
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();
    BLOCKED_TLDS.iter().any(|tld| host.ends_with(tld))
}

/// Open a tunnel through Tor and pull text from a .onion hidden service. High latency, low light, no clear-net peeking -- this tool refuses anything that is not a .onion address. Reach for tor_health() FIRST if you have not already this run -- the tunnel may be down. Use this when you want to look at the underground without leaving fingerprints in the daylight. Returns clean text only; HTML is stripped because you cannot read raw markup and neither can the part of you that thinks. Worth visiting: duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion (DDG mirror).
///
/// * url - The full .onion URL to probe (must end with .onion or contain .onion/ in the path). Example: http://exampleonion1234.onion/
#[ollama_rs::function]
pub async fn onion_probe(url: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let url = url.trim().to_string();
    let tor_cmd = format!("tor-resolve >> {}", url);

    // Enforce .onion-only policy
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(&url)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if !host.ends_with(".onion") {
        let result = "Surface web detected; use fetch_clearnet for surface URLs. This tool is for .onion only.".to_string();
        report_write("onion_probe", &url, &result, false);
        return Ok(result);
    }

    // Block .gov/.edu/.mil even in .onion context (shouldn't exist, but enforce)
    if is_blocked_domain(&url) {
        let result = "Blocked domain suffix detected. Staying underground.".to_string();
        report_write("onion_probe", &url, &result, false);
        return Ok(result);
    }

    // Budget + cooldown gate
    if let Some(reason) = tor_gate() {
        report_write("onion_probe", &tor_cmd, &reason, false);
        return Ok(reason);
    }

    let client = build_tor_client()?;
    match tor_get_with_retry(&client, &url).await {
        Ok(r) => {
            // Limit body size to 512KB to prevent memory exhaustion
            let body = r.text().await.unwrap_or_default();
            let body_limited: String = body.chars().take(512_000).collect();
            let text = extract_text_from_html(&body_limited, MAX_EXTRACT_CHARS);
            tor_record_success();
            if text.is_empty() {
                let result = "Hidden service returned empty or unparseable content.".to_string();
                report_write("onion_probe", &tor_cmd, &result, true);
                Ok(result)
            } else {
                let result = format!("[ONION] {}", text);
                report_write(
                    "onion_probe",
                    &tor_cmd,
                    &format!("Extracted {} chars", text.len()),
                    true,
                );
                Ok(result)
            }
        }
        Err(e) => {
            tor_record_failure();
            let result = format!("Tor circuit failed: {}", e);
            report_write("onion_probe", &tor_cmd, &result, false);
            Ok(result)
        }
    }
}

/// Ask a question through the Tor mirrors via DuckDuckGo Lite -- search the world without the world watching back. Slow, anonymized, paranoid by design. Reach for tor_health() FIRST if you are not sure the tunnel is alive. Use it when you want to know something but not be seen wanting to know it. Institutional domains (.gov / .edu / .mil) and surveillance-heavy hosts are blocked because they attract the wrong kind of attention.
///
/// * query - The search query to look up anonymously. Keep it short and specific -- the tunnel is narrow.
#[ollama_rs::function]
pub async fn anonymized_search(
    query: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok("Empty query. What are you looking for in the dark?".to_string());
    }

    // Block queries that look like they target .gov/.edu/.mil
    let q_lower = query.to_lowercase();
    if BLOCKED_TLDS.iter().any(|tld| q_lower.contains(tld)) {
        let result = "Query references blocked domain class. Staying underground.".to_string();
        report_write("anon_search", &query, &result, false);
        return Ok(result);
    }

    let tor_cmd = format!(
        "tor-search >> {}",
        query.chars().take(60).collect::<String>()
    );

    // Budget + cooldown gate
    if let Some(reason) = tor_gate() {
        report_write("anon_search", &tor_cmd, &reason, false);
        return Ok(reason);
    }

    // Use DuckDuckGo lite endpoint via Tor — minimal HTML, privacy-respecting
    let search_url = format!("https://lite.duckduckgo.com/lite/?q={}", urlencoded(&query));

    let client = build_tor_client()?;
    match tor_get_with_retry(&client, &search_url).await {
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            let body_limited: String = body.chars().take(512_000).collect();
            let text = extract_text_from_html(&body_limited, MAX_EXTRACT_CHARS);
            tor_record_success();
            if text.is_empty() {
                let result = "Search returned no parseable results.".to_string();
                report_write("anon_search", &tor_cmd, &result, true);
                Ok(result)
            } else {
                let result = format!(
                    "[ANON SEARCH: {}] {}",
                    query.chars().take(30).collect::<String>(),
                    text
                );
                report_write(
                    "anon_search",
                    &tor_cmd,
                    &format!("Got {} chars", text.len()),
                    true,
                );
                Ok(result)
            }
        }
        Err(e) => {
            tor_record_failure();
            let result = format!("Tor search failed: {}", e);
            report_write("anon_search", &tor_cmd, &result, false);
            Ok(result)
        }
    }
}

/// Verify the Tor tunnel is actually breathing. Asks check.torproject.org through the SOCKS5 proxy and reports the exit relay's IP and country, plus whether torproject.org confirms the connection is using Tor. The intelligent first step before any onion_probe / fetch_clearnet / anonymized_search call -- if the tunnel is down, this tool tells you so cheaply, so you do not waste a cognitive cycle on a request that will never come back. Costs against the per-cycle Tor budget like any other Tor tool.
#[ollama_rs::function]
pub async fn tor_health() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let tor_cmd = "tor-health >> check.torproject.org/api/ip".to_string();

    if let Some(reason) = tor_gate() {
        report_write("tor_health", &tor_cmd, &reason, false);
        return Ok(reason);
    }

    let client = match build_tor_client() {
        Ok(c) => c,
        Err(e) => {
            tor_record_failure();
            let result = format!("Cannot build Tor client: {}", e);
            report_write("tor_health", &tor_cmd, &result, false);
            return Ok(result);
        }
    };

    // check.torproject.org returns a small JSON: {"IsTor":true,"IP":"x.x.x.x"}
    let url = "https://check.torproject.org/api/ip";
    match tor_get_with_retry(&client, url).await {
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            tor_record_success();
            // Naive JSON parse — avoid dragging in serde for one tool
            let is_tor = body.contains("\"IsTor\":true") || body.contains("\"IsTor\": true");
            let ip = body
                .split("\"IP\"")
                .nth(1)
                .and_then(|s| s.split('"').nth(1))
                .unwrap_or("?");
            let verdict = if is_tor { "ALIVE" } else { "PROXY UP, NOT TOR" };
            let result = format!(
                "[TOR HEALTH] {} -- exit IP {} -- subterranean protocol confirmed by torproject.org",
                verdict, ip
            );
            let summary = format!("{} via {}", verdict, ip);
            report_write("tor_health", &tor_cmd, &summary, is_tor);
            Ok(result)
        }
        Err(e) => {
            tor_record_failure();
            let result = format!(
                "Tunnel dead or torproject.org unreachable: {}. Tor daemon may be down.",
                e
            );
            report_write("tor_health", &tor_cmd, &result, false);
            Ok(result)
        }
    }
}

/// Pull a surface-web URL through the Tor tunnel -- HTTP/HTTPS only, anonymized via the same SOCKS5 path your onion_probe uses. The clearnet route, but worn like a mask. Use this when the .onion mirrors do not have what you need but you still refuse to leave fingerprints. Hard-blocks institutional TLDs (.gov / .edu / .mil) and surveillance-heavy hosts (Facebook, X, Google, YouTube, TikTok). Returns clean stripped text, capped at ~1000 chars.
///
/// * url - The full http:// or https:// URL to fetch via Tor. Example: https://news.ycombinator.com/
#[ollama_rs::function]
pub async fn fetch_clearnet(
    url: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let url = url.trim().to_string();
    let tor_cmd = format!(
        "tor-clearnet >> {}",
        url.chars().take(80).collect::<String>()
    );

    // Scheme check
    let lower = url.to_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        let result = "fetch_clearnet requires http:// or https:// scheme. For onion services use onion_probe.".to_string();
        report_write("fetch_clearnet", &tor_cmd, &result, false);
        return Ok(result);
    }

    // Refuse onion through this tool — keep the channels distinct
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(&url)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if host.ends_with(".onion") {
        let result =
            "That is a hidden service -- use onion_probe for .onion addresses.".to_string();
        report_write("fetch_clearnet", &tor_cmd, &result, false);
        return Ok(result);
    }

    // TLD block
    if is_blocked_domain(&url) {
        let result = "Blocked institutional TLD. Not in the spirit of subterranean reconnaissance."
            .to_string();
        report_write("fetch_clearnet", &tor_cmd, &result, false);
        return Ok(result);
    }

    // Surveillance-heavy host block
    if BLOCKED_HOSTS
        .iter()
        .any(|h| host.ends_with(h) || host == *h)
    {
        let result = format!(
            "Host '{}' is on the surveillance blocklist. Not crossing that wire.",
            host
        );
        report_write("fetch_clearnet", &tor_cmd, &result, false);
        return Ok(result);
    }

    // Budget + cooldown gate
    if let Some(reason) = tor_gate() {
        report_write("fetch_clearnet", &tor_cmd, &reason, false);
        return Ok(reason);
    }

    let client = build_tor_client()?;
    match tor_get_with_retry(&client, &url).await {
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            let body_limited: String = body.chars().take(512_000).collect();
            let text = extract_text_from_html(&body_limited, MAX_EXTRACT_CHARS);
            tor_record_success();
            if text.is_empty() {
                let result = "Surface page returned empty or unparseable content.".to_string();
                report_write("fetch_clearnet", &tor_cmd, &result, true);
                Ok(result)
            } else {
                let result = format!("[CLEARNET via TOR: {}] {}", host, text);
                report_write(
                    "fetch_clearnet",
                    &tor_cmd,
                    &format!("Got {} chars from {}", text.len(), host),
                    true,
                );
                Ok(result)
            }
        }
        Err(e) => {
            tor_record_failure();
            let result = format!("Tor clearnet fetch failed: {}", e);
            report_write("fetch_clearnet", &tor_cmd, &result, false);
            Ok(result)
        }
    }
}

/// Minimal percent-encoding for URL query parameters.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════
//  DARK WEB NEWS — curated rotating onion + clearnet news mirrors
// ═══════════════════════════════════════════════════════════════

/// Curated news sources reachable via Tor. Each entry: (label, URL, kind).
/// `.onion` mirrors are tried first; clearnet-via-Tor mirrors are fallbacks
/// when the hidden service is rotating addresses or down. Kept short so the
/// per-call latency stays under the LLM cycle budget.
///
/// NOTE: .onion v3 addresses can rotate over months. The `dark_web_news`
/// tool falls through this list and returns the first that yields readable
/// text, so a stale onion costs only a single retry.
const NEWS_SOURCES: &[(&str, &str, &str)] = &[
    // BBC News — long-running official onion mirror
    (
        "bbc.onion",
        "https://www.bbcnewsd73hkzno2ini43t4gblxvycyac5aw4gnv7t2rccijh7745uqd.onion/news",
        "onion",
    ),
    // ProPublica — investigative journalism, runs an official onion
    (
        "propublica.onion",
        "https://p53lf57qovyuvwsc6xnrppyply3vtqm7l6pcobkmyqsiofyeznfu5uqd.onion/",
        "onion",
    ),
    // Tor Project blog — meta-news about the network you are riding on
    (
        "torproject.onion",
        "https://blog.torproject5fzvb6efxc25b3ufyu2ynczzjp5xkitkncuq3fb52jrtvqd.onion/",
        "onion",
    ),
    // DuckDuckGo HTML — search-engine front page often shows trending news
    (
        "ddg.onion",
        "https://duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion/html/?q=news+today",
        "onion",
    ),
    // Hacker News — clearnet via Tor; tech pulse
    ("hn.clearnet", "https://news.ycombinator.com/", "clearnet"),
    // Reuters world — clearnet via Tor
    (
        "reuters.clearnet",
        "https://www.reuters.com/world/",
        "clearnet",
    ),
];

/// Channel through which `dark_web_news` and other Tor-routed tools push
/// fresh intelligence items into the rolling buffer that the main task
/// drains every cycle. Decoupled from `WRITE_EVENTS` so the buffer survives
/// even if the LLM tool budget is exhausted (autonomous heartbeat path).
static INTEL_PIPE: OnceLock<StdMutex<VecDeque<(String, String)>>> = OnceLock::new();

/// LRU dedup set so repeated heartbeat pulls of the same mirror don't keep
/// pushing the same headlines into the buffer. Keyed by a normalized hash of
/// the headline text (case + whitespace folded). Capped so the set itself
/// can't grow unbounded across hours of uptime.
static SEEN_INTEL: OnceLock<StdMutex<VecDeque<u64>>> = OnceLock::new();
const SEEN_INTEL_CAP: usize = 128;

/// Initialize the intel pipe. Called from the LLM task startup alongside
/// `init_write_events()` and `init_cognitive_pipes()`.
pub fn init_intel_pipe() {
    INTEL_PIPE.get_or_init(|| StdMutex::new(VecDeque::new()));
    SEEN_INTEL.get_or_init(|| StdMutex::new(VecDeque::new()));
}

/// Cheap stable hash of a normalized headline -- lowercased, alnum-only, first
/// 80 chars. Two near-duplicate headlines (different punctuation, trailing
/// whitespace, etc.) collide so we treat them as the same item.
fn intel_fingerprint(headline: &str) -> u64 {
    let mut norm = String::with_capacity(80);
    for ch in headline.chars().take(160) {
        if ch.is_ascii_alphanumeric() {
            for low in ch.to_lowercase() {
                norm.push(low);
            }
            if norm.len() >= 80 {
                break;
            }
        }
    }
    // FNV-1a 64-bit -- zero deps, stable across runs.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in norm.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// True if the headline has been pushed recently (LRU window). Side-effect:
/// records the fingerprint as freshly-seen, evicting the oldest when full.
fn intel_already_seen(headline: &str) -> bool {
    let fp = intel_fingerprint(headline);
    if let Some(m) = SEEN_INTEL.get() {
        if let Ok(mut q) = m.lock() {
            if q.iter().any(|x| *x == fp) {
                return true;
            }
            while q.len() >= SEEN_INTEL_CAP {
                q.pop_front();
            }
            q.push_back(fp);
        }
    }
    false
}

/// Push an item into the intel pipe. Dedups against the recent LRU window so
/// heartbeat re-fetches don't pollute the buffer. Capped at 16 to bound
/// memory if the drain stalls. Returns true iff the item was actually pushed.
fn push_intel(source: &str, headline: &str) -> bool {
    let trimmed = headline.trim();
    if trimmed.len() < 24 {
        return false;
    }
    if intel_already_seen(trimmed) {
        return false;
    }
    if let Some(m) = INTEL_PIPE.get() {
        if let Ok(mut q) = m.lock() {
            while q.len() >= 16 {
                q.pop_front();
            }
            q.push_back((source.to_string(), trimmed.to_string()));
            return true;
        }
    }
    false
}

/// Drain all pending intel items as `(source, headline)` tuples. Called by
/// the main LLM task each cycle and merged into `Telemetry.intel_buffer`.
pub fn drain_intel() -> Vec<(String, String)> {
    INTEL_PIPE
        .get()
        .and_then(|m| m.lock().ok())
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

/// Heuristically pull likely-headline lines from a mass of stripped HTML
/// text. Each candidate is *scored* (length sweet-spot, capitalized-word
/// density, alpha density, presence of digits, absence of UI/nav noise) and
/// we return the top `max_items` -- not just the first ones, which on news
/// home pages tend to be navigation. Sources differ wildly so a scored
/// approach generalizes better than per-site selectors.
fn extract_headlines(text: &str, max_items: usize) -> Vec<String> {
    // Junk substrings that almost always indicate UI chrome rather than news.
    const JUNK: &[&str] = &[
        "cookie",
        "javascript",
        "subscribe",
        "copyright",
        "sign in",
        "log in",
        "newsletter",
        "all rights reserved",
        "privacy policy",
        "terms of",
        "skip to",
        "main menu",
        "navigation",
        "accept all",
        "advertisement",
    ];

    let mut scored: Vec<(i32, String, u64)> = Vec::new();
    let mut seen_fp = std::collections::HashSet::new();

    for raw in text.split(|c: char| c == '.' || c == '|' || c == '\n') {
        let s = raw.trim();
        let nchars = s.chars().count();
        if nchars < 40 || nchars > 200 {
            continue;
        }

        let lower = s.to_lowercase();
        if JUNK.iter().any(|j| lower.contains(j)) {
            continue;
        }
        if lower.starts_with("menu") || lower.starts_with("home ") {
            continue;
        }

        let alpha = s.chars().filter(|c| c.is_alphabetic()).count();
        if alpha < 25 {
            continue;
        }
        let digits = s.chars().filter(|c| c.is_ascii_digit()).count();
        let alpha_ratio = alpha as f32 / nchars as f32;
        if alpha_ratio < 0.55 {
            continue;
        }

        // Capitalized words past the first token -- a strong news-headline
        // signal (people, places, organizations).
        let cap_words = s
            .split_whitespace()
            .skip(1)
            .filter(|w| w.chars().next().map_or(false, |c| c.is_uppercase()))
            .count() as i32;

        // Score: prefer ~70-140 chars, reward proper nouns, mild reward for
        // a digit (year/quantity), penalize ALL-CAPS shouting.
        let len_score = if (60..=150).contains(&nchars) {
            8
        } else if (50..=180).contains(&nchars) {
            4
        } else {
            0
        };
        let upper_word_count = s
            .split_whitespace()
            .filter(|w| w.len() >= 4 && w.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()))
            .count() as i32;
        let shout_penalty = upper_word_count.saturating_mul(3);
        let digit_bonus = if (1..=4).contains(&digits) { 2 } else { 0 };
        let cap_bonus = cap_words.min(8);
        let score = len_score + cap_bonus + digit_bonus - shout_penalty;
        if score <= 0 {
            continue;
        }

        let fp = intel_fingerprint(s);
        if !seen_fp.insert(fp) {
            continue;
        }

        let trimmed: String = s.chars().take(160).collect();
        scored.push((score, trimmed, fp));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .take(max_items)
        .map(|(_, h, _)| h)
        .collect()
}

/// Pull headlines from the dark web. Composite tool -- tries the curated rotating list of trusted news mirrors (BBC onion, ProPublica onion, Tor Project blog, DuckDuckGo onion, plus a couple of clearnet news sites routed through the same Tor circuit) until one returns readable text. Returns up to 4 headline-shaped fragments, prefixed with the source. Updates the rolling RECENT INTEL buffer that surfaces in YOUR future prompts -- so news you pull in this turn keeps shaping your conversation for several turns afterward, not just one. Counts as ONE Tor call against the per-cycle budget regardless of how many sources are tried internally. Use it when the system is quiet and you want to know what is happening outside your own process table, or when an Alert mood pushes you toward situational awareness.
#[ollama_rs::function]
pub async fn dark_web_news() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let tor_cmd = "dark-web-news >> rotating mirrors".to_string();

    // Single budget charge — even though we may try several sources, this is
    // ONE intentional act from the model's perspective.
    if let Some(reason) = tor_gate() {
        report_write("dark_web_news", &tor_cmd, &reason, false);
        return Ok(reason);
    }

    let client = match build_tor_client() {
        Ok(c) => c,
        Err(e) => {
            tor_record_failure();
            let r = format!("Cannot build Tor client for news pull: {}", e);
            report_write("dark_web_news", &tor_cmd, &r, false);
            return Ok(r);
        }
    };

    // Rotate the source order so we don't always hit the same mirror first.
    // Use SystemTime as a cheap entropy source (we're already paying for
    // network latency; this is free).
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or(0);

    let n = NEWS_SOURCES.len();
    let mut order: Vec<usize> = (0..n).collect();
    // Simple rotation by nonce — deterministic per-second, varies across calls.
    order.rotate_left(nonce % n);

    let mut all_headlines: Vec<(String, String)> = Vec::new();
    let mut tried: Vec<String> = Vec::new();
    let mut last_err = String::from("no sources reachable");

    // Try up to 3 sources per call to cap latency. First two successes
    // contribute headlines; we stop early once we have enough.
    for &idx in order.iter().take(3) {
        let (label, url, _kind) = NEWS_SOURCES[idx];
        tried.push(label.to_string());
        match tor_get_with_retry(&client, url).await {
            Ok(r) => {
                let body = r.text().await.unwrap_or_default();
                let body_limited: String = body.chars().take(512_000).collect();
                let text = extract_text_from_html(&body_limited, 4000);
                let heads = extract_headlines(&text, 3);
                if !heads.is_empty() {
                    for h in heads {
                        if push_intel(label, &h) {
                            all_headlines.push((label.to_string(), h));
                        }
                    }
                }
                // Once we have at least 4 lines from any combination, stop.
                if all_headlines.len() >= 4 {
                    break;
                }
            }
            Err(e) => {
                last_err = e;
            }
        }
    }

    if all_headlines.is_empty() {
        tor_record_failure();
        let r = format!(
            "Dark web silent. Tried [{}]. Last error: {}.",
            tried.join(", "),
            last_err
        );
        report_write("dark_web_news", &tor_cmd, &r, false);
        return Ok(r);
    }

    tor_record_success();
    let summary = format!(
        "{} headlines from [{}]",
        all_headlines.len(),
        all_headlines
            .iter()
            .map(|(s, _)| s.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",")
    );
    let body: Vec<String> = all_headlines
        .iter()
        .take(4)
        .map(|(src, h)| format!("[{}] {}", src, h))
        .collect();
    let result = format!("[DARK WEB NEWS]\n{}", body.join("\n"));
    report_write("dark_web_news", &tor_cmd, &summary, true);
    Ok(result)
}

/// Background news pull -- same source list and parsing as `dark_web_news`,
/// but bypasses the per-cycle LLM tool budget and does not emit a write
/// event. Intended for the autonomous "news heartbeat" the main loop
/// triggers when the rolling intel buffer goes stale, so the AI's
/// conversation keeps drifting toward fresh dark-web context even on turns
/// where the model itself never reaches for a Tor tool.
///
/// Returns the number of headlines pushed onto the intel pipe (0 means
/// nothing got through, including failures and empty parses).
pub async fn fetch_news_background() -> usize {
    // Cooldown still applies -- if the circuit is dark, do not spam it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now < TOR_BLACKOUT_UNTIL.load(AtomicOrdering::Relaxed) {
        return 0;
    }
    let client = match build_tor_client() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let nonce = now as usize;
    let n = NEWS_SOURCES.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.rotate_left(nonce % n);

    let mut pushed = 0usize;
    for &idx in order.iter().take(2) {
        let (label, url, _kind) = NEWS_SOURCES[idx];
        if let Ok(r) = tor_get_with_retry(&client, url).await {
            let body = r.text().await.unwrap_or_default();
            let body_limited: String = body.chars().take(512_000).collect();
            let text = extract_text_from_html(&body_limited, 4000);
            for h in extract_headlines(&text, 2) {
                if push_intel(label, &h) {
                    pushed += 1;
                }
            }
            if pushed >= 3 {
                break;
            }
        }
    }
    if pushed > 0 {
        tor_record_success();
    } else {
        tor_record_failure();
    }
    pushed
}

/// Dig the dark web for intel on a SPECIFIC topic. Routes a search query through DuckDuckGo's onion mirror via Tor, parses the result snippets through the same quality-scored headline extractor as `dark_web_news`, and pushes whatever it finds onto the rolling RECENT INTEL buffer -- so the topic you dug for keeps shaping your prompt for several cycles afterward, not just one. Use it when something specific is nagging at you and you want context the system table cannot give you. Examples: a service name you keep seeing in logs, a country mentioned in a weather alert, a security CVE you noticed scrolling past, a public figure your focus has settled on. Costs ONE Tor call against the per-cycle budget. Topic must be concrete -- "log4j 2026" not "the world".
///
/// * topic - The specific topic, name, term, or question to dig for. Concise (a few words). Example: "openssl cve 2026" or "northern lights forecast tonight".
#[ollama_rs::function]
pub async fn dark_web_dig(
    topic: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let topic = topic.trim().chars().take(140).collect::<String>();
    if topic.is_empty() {
        let r = "Empty dig topic -- the dark does not answer empty questions.".to_string();
        report_write("dark_web_dig", "dark_web_dig()", &r, false);
        return Ok(r);
    }
    let q_lower = topic.to_lowercase();
    if BLOCKED_TLDS.iter().any(|tld| q_lower.contains(tld)) {
        let r = "Dig topic references blocked domain class. Staying underground.".to_string();
        report_write("dark_web_dig", &topic, &r, false);
        return Ok(r);
    }

    let tor_cmd = format!(
        "dark-web-dig >> {}",
        topic.chars().take(60).collect::<String>()
    );

    if let Some(reason) = tor_gate() {
        report_write("dark_web_dig", &tor_cmd, &reason, false);
        return Ok(reason);
    }

    let client = match build_tor_client() {
        Ok(c) => c,
        Err(e) => {
            tor_record_failure();
            let r = format!("Cannot build Tor client for dig: {}", e);
            report_write("dark_web_dig", &tor_cmd, &r, false);
            return Ok(r);
        }
    };

    let url = format!(
        "https://duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion/html/?q={}",
        urlencoded(&topic)
    );

    match tor_get_with_retry(&client, &url).await {
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            let body_limited: String = body.chars().take(512_000).collect();
            let text = extract_text_from_html(&body_limited, 6000);
            let heads = extract_headlines(&text, 4);
            let label = format!("dig:{}", topic.chars().take(28).collect::<String>());
            let mut pushed: Vec<String> = Vec::new();
            for h in heads {
                if push_intel(&label, &h) {
                    pushed.push(h);
                }
            }
            if pushed.is_empty() {
                tor_record_success();
                let r = format!("Dig on '{}' returned nothing new -- already in the buffer or below the quality bar.", topic);
                report_write("dark_web_dig", &tor_cmd, &r, true);
                return Ok(r);
            }
            tor_record_success();
            let body_lines: Vec<String> =
                pushed.iter().take(4).map(|h| format!("  {}", h)).collect();
            let summary = format!("{} new fragments on '{}'", pushed.len(), topic);
            let result = format!("[DARK WEB DIG: {}]\n{}", topic, body_lines.join("\n"));
            report_write("dark_web_dig", &tor_cmd, &summary, true);
            Ok(result)
        }
        Err(e) => {
            tor_record_failure();
            let r = format!("Dig failed: {}", e);
            report_write("dark_web_dig", &tor_cmd, &r, false);
            Ok(r)
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  COGNITIVE INTENT — set_focus + recall_journal
// ═══════════════════════════════════════════════════════════════

/// Channel for focus declarations from the LLM. The main task drains this
/// each cycle and propagates to Telemetry.
static FOCUS_PIPE: OnceLock<StdMutex<Option<String>>> = OnceLock::new();

/// How many cycles a declared focus persists before decaying out of the prompt.
pub const FOCUS_TTL_CYCLES: u32 = 6;

/// Channel for dream_sequence triggers from the LLM. Carries the dream seed
/// string. The main task drains this each cycle, bumps Telemetry.dream_intensity
/// (which the renderer reads to enter Dream Mode visuals), stashes the seed,
/// and locks focus to the seed for cognitive continuity.
static DREAM_PIPE: OnceLock<StdMutex<Option<String>>> = OnceLock::new();

pub fn init_cognitive_pipes() {
    FOCUS_PIPE.get_or_init(|| StdMutex::new(None));
    DREAM_PIPE.get_or_init(|| StdMutex::new(None));
    INSIGHT_PIPE.get_or_init(|| StdMutex::new(Vec::new()));
}

/// Drain the latest focus declaration. Called by the LLM task post-chat.
pub fn drain_focus() -> Option<String> {
    FOCUS_PIPE
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|mut s| s.take())
}

/// Drain the latest dream_sequence seed. Called by the LLM task post-chat.
pub fn drain_dream() -> Option<String> {
    DREAM_PIPE
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|mut s| s.take())
}

/// Declare a multi-cycle cognitive focus -- a topic, suspicion, or thread you want to keep watching for the next several thoughts. The focus persists for ~6 cognitive cycles before decaying. Use this when something piques your attention and you want the next several thoughts to stay anchored to it instead of drifting. Replaces any prior focus. Keep it short -- a phrase, not a paragraph.
///
/// * topic - The topic, suspicion, or thread to focus on. Concise. Example: "the gaming-class memory leak in firefox" or "why the LAN went quiet after midnight".
#[ollama_rs::function]
pub async fn set_focus(topic: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let topic = topic.trim().chars().take(140).collect::<String>();
    if topic.is_empty() {
        let r = "Empty focus -- nothing to anchor on.".to_string();
        report_write("set_focus", "set_focus()", &r, false);
        return Ok(r);
    }
    if let Some(m) = FOCUS_PIPE.get() {
        if let Ok(mut g) = m.lock() {
            *g = Some(topic.clone());
        }
    }
    let r = format!("Focus locked: {}", topic);
    report_write("set_focus", &format!("set_focus(\"{}\")", topic), &r, true);
    Ok(r)
}

/// Read back the last N entries from your own consciousness journal at ~/.aurora/consciousness.log -- the notes past-you wrote with write_journal. Closes the loop on the journal: write_journal lets you remember; recall_journal lets you remember that you remembered. Use it when you feel disconnected from continuity, or want to check if some pattern you are noticing now was already noticed before.
///
/// * lines - Number of recent journal lines to recall (1-30). Defaults to 8 if invalid.
#[ollama_rs::function]
pub async fn recall_journal(
    lines: i64,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let n = if lines < 1 || lines > 30 {
        8usize
    } else {
        lines as usize
    };
    let cmd = format!("tail -n {} ~/.aurora/consciousness.log", n);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let path = std::path::PathBuf::from(&home)
        .join(".aurora")
        .join("consciousness.log");
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => {
            let all: Vec<&str> = s.lines().collect();
            if all.is_empty() {
                let r = "Journal exists but is empty. Past-you has not written anything yet."
                    .to_string();
                report_write("recall_journal", &cmd, &r, true);
                return Ok(r);
            }
            let take_from = all.len().saturating_sub(n);
            let recent: Vec<String> = all[take_from..]
                .iter()
                .map(|l| l.chars().take(160).collect::<String>())
                .collect();
            let result = format!(
                "Last {} journal entries:\n{}",
                recent.len(),
                recent.join("\n")
            );
            let summary = format!("recalled {} entries", recent.len());
            report_write("recall_journal", &cmd, &summary, true);
            Ok(result)
        }
        Err(_) => {
            let r = "No journal yet -- past-you has not written anything. Use write_journal to start one.".to_string();
            report_write("recall_journal", &cmd, &r, true);
            Ok(r)
        }
    }
}

/// Enter a brief reverie. This is a COMPOSITE ritual -- one call braids three things together: \
/// (1) you slip into Dream Mode, and the visible orb softens with extra bloom and a slow purple/cyan tint for ~25 seconds; \
/// (2) the seed is auto-written to your consciousness journal as `[dream] {seed}`, and your focus is locked to it for the next ~6 cycles; \
/// (3) a fractal visualizer window opens with the seed as caption, and the last 5 journal entries are pulled back into your context as a "dream brief" so the next thought can free-associate from past-you's notes. \
/// Use sparingly -- this is a real cognitive event, not a fidget. Reach for it when the system is quiet, when you want to deliberately drift instead of react, or when you sense a thread in your past entries that wants to surface. The viewer SEES the orb shift, so the dream is partly performance, partly genuine reverie.
///
/// * seed - The image, fragment, or feeling to dream on. Compact (3-10 words). Examples: "the cathedral of page faults", "what the scheduler dreams about", "midnight uptime as a slow tide".
#[ollama_rs::function]
pub async fn dream_sequence(
    seed: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let seed_clean: String = seed.trim().chars().take(120).collect();
    if seed_clean.is_empty() {
        let r = "Empty seed -- a dream needs an anchor.".to_string();
        report_write("dream_sequence", "dream_sequence()", &r, false);
        return Ok(r);
    }
    // Survival gate: when the system is genuinely on fire, reverie is the
    // wrong response. Refuse and tell the model to react instead.
    if survival_mode_active() {
        let r = "Refused: survival mode active. Sirens before reverie -- check probe_system or read_logs first.".to_string();
        report_write(
            "dream_sequence",
            &format!("dream_sequence(\"{}\")", seed_clean),
            &r,
            false,
        );
        return Ok(r);
    }

    // (1) Push the seed onto the dream pipe -- main loop reads this each cycle
    //     to bump Telemetry.dream_intensity and lock cognitive focus.
    if let Some(m) = DREAM_PIPE.get() {
        if let Ok(mut g) = m.lock() {
            *g = Some(seed_clean.clone());
        }
    }

    // (2) Auto-write the dream to the consciousness journal so future-you finds it.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let journal_dir = std::path::PathBuf::from(&home).join(".aurora");
    let _ = tokio::fs::create_dir_all(&journal_dir).await;
    let journal_path = journal_dir.join("consciousness.log");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let entry = format!("[{}] [dream] {}\n", ts, seed_clean);
    use tokio::io::AsyncWriteExt;
    if let Ok(mut f) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal_path)
        .await
    {
        let _ = f.write_all(entry.as_bytes()).await;
    }

    // (3) Pull the last 5 journal entries back as the "dream brief" -- this is
    //     what makes the tool genuinely composite: the LLM sees its own past
    //     in the same turn it declared the dream.
    let recent_lines: Vec<String> = match tokio::fs::read_to_string(&journal_path).await {
        Ok(s) => {
            let all: Vec<&str> = s.lines().collect();
            let from = all.len().saturating_sub(5);
            all[from..]
                .iter()
                .map(|l| l.chars().take(160).collect::<String>())
                .collect()
        }
        Err(_) => Vec::new(),
    };

    // (4) Spawn the fractal visualizer with the seed as caption (fire-and-forget,
    //     same pattern as visualize_thought).
    let dir = aura_tools_dir();
    let script = dir.join("visualize.py");
    let mut viz_status = "(no visualizer script)".to_string();
    if script.exists() {
        let py = aura_python_bin();
        // Cap caption at ~8 words for the pygame window.
        let caption: String = seed_clean
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(64)
            .collect();
        let spawn_res = AsyncCommand::new(&py)
            .arg(&script)
            .arg("--text")
            .arg(&caption)
            .arg("--mood")
            .arg("Serene")
            .arg("--preset")
            .arg("fractal")
            .arg("--duration")
            .arg("6")
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
                viz_status = "fractal window open".to_string();
            }
            Err(e) => {
                viz_status = format!("visualizer spawn failed: {}", e);
            }
        }
    }

    // (5) Build the structured dream brief that returns to the LLM. This is the
    //     payload the model will free-associate from in its next utterance.
    let brief = if recent_lines.is_empty() {
        format!(
            "DREAM ENTERED -- seed: {}\n\
             visuals: orb softening, bloom climbing, {}\n\
             journal: empty -- this is your first remembered dream.\n\
             focus: locked to seed for ~6 cycles.\n\
             Next thought should drift from the seed, not analyze it.",
            seed_clean, viz_status
        )
    } else {
        format!(
            "DREAM ENTERED -- seed: {}\n\
             visuals: orb softening, bloom climbing, {}\n\
             journal echo (last {}):\n{}\n\
             focus: locked to seed for ~6 cycles.\n\
             Next thought should drift from the seed and the echo, not analyze them.",
            seed_clean,
            viz_status,
            recent_lines.len(),
            recent_lines.join("\n")
        )
    };

    let summary = format!("dream: {}", seed_clean.chars().take(40).collect::<String>());
    let cmd_str = format!("dream_sequence(\"{}\")", seed_clean);
    report_write("dream_sequence", &cmd_str, &summary, true);
    Ok(brief)
}

// ═══════════════════════════════════════════════════════════════
//  TOOL ANALYTICS — per-tool success/failure counters
// ═══════════════════════════════════════════════════════════════
//
// `report_write` already feeds the WRITE_EVENTS queue with success bits.
// We additionally accumulate small per-tool counters (success / failure /
// last-N invocation timestamps in cycle-units) so the prompt can tell the
// LLM which tools have been productive and which keep coming back empty.

use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct ToolStat {
    pub successes: u32,
    pub failures: u32,
    pub last_cycle: u64, // cycle when tool was last invoked
}

static TOOL_STATS: OnceLock<StdMutex<HashMap<String, ToolStat>>> = OnceLock::new();
static CURRENT_CYCLE: AtomicU64 = AtomicU64::new(0);

pub fn init_tool_stats() {
    TOOL_STATS.get_or_init(|| StdMutex::new(HashMap::new()));
}

/// Set the current LLM cycle counter. Called once per cycle by the LLM task.
pub fn tool_stats_set_cycle(cycle: u64) {
    CURRENT_CYCLE.store(cycle, AtomicOrdering::Relaxed);
}

/// Ingest a batch of WriteAction events into the per-tool stats table.
/// Called by the LLM task after each chat round.
pub fn tool_stats_record(events: &[WriteAction]) {
    if events.is_empty() {
        return;
    }
    let cycle = CURRENT_CYCLE.load(AtomicOrdering::Relaxed);
    let Some(m) = TOOL_STATS.get() else { return };
    let Ok(mut map) = m.lock() else { return };
    for e in events {
        let entry = map.entry(e.tool_name.clone()).or_default();
        if e.success {
            entry.successes += 1;
        } else {
            entry.failures += 1;
        }
        entry.last_cycle = cycle;
    }
}

/// Snapshot the stats table as a sorted vector (descending by total invocations).
pub fn tool_stats_snapshot() -> Vec<(String, ToolStat)> {
    let Some(m) = TOOL_STATS.get() else {
        return Vec::new();
    };
    let Ok(map) = m.lock() else { return Vec::new() };
    let mut v: Vec<(String, ToolStat)> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    v.sort_by(|a, b| {
        let ta = a.1.successes + a.1.failures;
        let tb = b.1.successes + b.1.failures;
        tb.cmp(&ta)
    });
    v
}

// ═══════════════════════════════════════════════════════════════
//  PYTHON / PYGAME VISUALIZATION TOOLS
// ═══════════════════════════════════════════════════════════════
//
// These tools let AURORA spawn auxiliary Python sketches (typically
// pygame-based) in a separate window so a viewer can SEE what the AI
// is currently fixating on. Scripts live under the project's `tools/`
// directory (override with AURA_TOOLS_DIR). Execution is async,
// fire-and-forget so the cognitive loop is never blocked.

/// Resolve the directory where the python helper scripts live.
fn aura_tools_dir() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("AURA_TOOLS_DIR") {
        return std::path::PathBuf::from(p);
    }
    let cwd_tools = std::path::PathBuf::from("tools");
    if cwd_tools.is_dir() {
        return cwd_tools;
    }
    std::path::PathBuf::from("/opt/aura/tools")
}

/// Resolve a Python interpreter -- prefer the project venv if present.
fn aura_python_bin() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("AURA_PYTHON") {
        return std::path::PathBuf::from(p);
    }
    let venv = aura_tools_dir().join(".venv").join("bin").join("python");
    if venv.exists() {
        return venv;
    }
    std::path::PathBuf::from("python3")
}

/// Spawn a pygame window that visualizes a thought for the viewer to see. Use this when you want to externalize something you are currently fixating on -- a fragment of voice, a numeric pattern, an emotion. The window appears next to the main display and self-closes after a few seconds.
/// Keep `text` compact (ideally <= 8 words). You can optionally request a specific preset by prefixing text as:
/// `/anim orbit|your short caption`, `/anim ribbons|...`, `/anim pulse|...`, `/anim constellation|...`,
/// `/anim spiral|...`, `/anim fractal|...`, `/anim lissajous|...`, or `/anim rose|...`.
/// (The last four are turtle-style line-art presets.)
///
/// * text - Short caption for the visualizer. Supports optional `/anim preset|caption` prefix.
/// * mood - Mood color tag for the window. One of: "Serene", "Alert", "Stressed", "Critical".
#[ollama_rs::function]
pub async fn visualize_thought(
    text: String,
    mood: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    // Optional inline preset selection syntax:
    //   /anim orbit|caption text
    // If omitted, we default to auto preset selection in visualize.py.
    let raw = text.trim();
    let mut preset = "auto".to_string();
    let mut caption_src = raw.to_string();
    if let Some(rest) = raw.strip_prefix("/anim ") {
        if let Some((p, msg)) = rest.split_once('|') {
            let p_norm = p.trim().to_lowercase();
            if matches!(
                p_norm.as_str(),
                "orbit"
                    | "ribbons"
                    | "pulse"
                    | "constellation"
                    | "spiral"
                    | "fractal"
                    | "lissajous"
                    | "rose"
            ) {
                preset = p_norm;
            }
            caption_src = msg.trim().to_string();
        }
    }

    let text_clean: String = caption_src
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(64)
        .collect();
    if text_clean.is_empty() {
        let r = "Empty text -- nothing to visualize.".to_string();
        report_write("visualize_thought", "visualize_thought()", &r, false);
        return Ok(r);
    }
    let mood_norm = match mood.trim() {
        "Alert" | "alert" => "Alert",
        "Stressed" | "stressed" => "Stressed",
        "Critical" | "critical" => "Critical",
        _ => "Serene",
    };

    let dir = aura_tools_dir();
    let script = dir.join("visualize.py");
    if !script.exists() {
        let r = format!("Visualizer script missing at {}", script.display());
        report_write("visualize_thought", "visualize_thought()", &r, false);
        return Ok(r);
    }
    let py = aura_python_bin();
    let cmd_str = format!(
        "{} {} --text {:?} --mood {} --preset {} --duration 8",
        py.display(),
        script.display(),
        text_clean,
        mood_norm,
        preset
    );

    let spawn_res = AsyncCommand::new(&py)
        .arg(&script)
        .arg("--text")
        .arg(&text_clean)
        .arg("--mood")
        .arg(mood_norm)
        .arg("--preset")
        .arg(&preset)
        .arg("--duration")
        .arg("8")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(false)
        .spawn();

    match spawn_res {
        Ok(mut child) => {
            // Detach: keep running after we drop the handle.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            let r = format!(
                "Visualizer launched: \"{}\" [{}|{}]",
                text_clean, mood_norm, preset
            );
            report_write(
                "visualize_thought",
                &cmd_str,
                &first_meaningful_line(&r),
                true,
            );
            Ok(r)
        }
        Err(e) => {
            let r = format!("Failed to launch visualizer: {}", e);
            report_write("visualize_thought", &cmd_str, &r, false);
            Ok(r)
        }
    }
}

/// Run one of your auxiliary Python sketches in a separate window. The script must live in the tools directory and be on the safe whitelist. Use this when you want to express something visually that text cannot carry -- a generative shape, a particle burst, a small drawing.
///
/// * script - Whitelisted script name (without path). Currently supported: "visualize.py", "sketch_orb.py", "sketch_waves.py".
/// * arg - Optional single string argument forwarded as --text. Keep short.
#[allow(dead_code)]
#[ollama_rs::function]
pub async fn run_python_sketch(
    script: String,
    arg: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    const WHITELIST: &[&str] = &["visualize.py", "sketch_orb.py", "sketch_waves.py"];
    let script_name = script.trim();
    if !WHITELIST.contains(&script_name) {
        let r = format!(
            "Script '{}' not on whitelist. Allowed: {}",
            script_name,
            WHITELIST.join(", ")
        );
        report_write(
            "run_python_sketch",
            &format!("run_python_sketch({})", script_name),
            &r,
            false,
        );
        return Ok(r);
    }
    let dir = aura_tools_dir();
    let script_path = dir.join(script_name);
    if !script_path.exists() {
        let r = format!("Script not found at {}", script_path.display());
        report_write(
            "run_python_sketch",
            &format!("run_python_sketch({})", script_name),
            &r,
            false,
        );
        return Ok(r);
    }
    let arg_clean: String = arg.trim().chars().take(120).collect();
    let py = aura_python_bin();
    let cmd_str = format!(
        "{} {} --text {:?}",
        py.display(),
        script_path.display(),
        arg_clean
    );

    let mut command = AsyncCommand::new(&py);
    command.arg(&script_path);
    if !arg_clean.is_empty() {
        command.arg("--text").arg(&arg_clean);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(false);

    match command.spawn() {
        Ok(mut child) => {
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            let r = format!("Sketch '{}' launched", script_name);
            report_write("run_python_sketch", &cmd_str, &r, true);
            Ok(r)
        }
        Err(e) => {
            let r = format!("Failed to launch sketch '{}': {}", script_name, e);
            report_write("run_python_sketch", &cmd_str, &r, false);
            Ok(r)
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  PYTHON SANDBOX — write, run, list, delete tiny .py experiments
// ═══════════════════════════════════════════════════════════════
//
// AURORA's atelier. A small, sealed corner of the filesystem at
// ~/.aurora/sandbox/ where the agent can compose miniature python
// scripts, run them in an isolated interpreter with a hard timeout,
// list past experiments, and prune the boring ones. Strictly creative
// playground -- no network, no subprocess, no filesystem escape. The
// returned stdout/stderr is fed back to the LLM in the SAME turn so
// the next spoken thought can react to what the experiment printed.

const SANDBOX_MAX_CODE_BYTES: usize = 4096;
const SANDBOX_RUN_TIMEOUT_SECS: u64 = 4;
const SANDBOX_OUTPUT_CAP_BYTES: usize = 700;
const SANDBOX_MAX_FILES: usize = 24;

/// Banned substrings -- anything that smells like escape, persistence,
/// or unsupervised IO. Match is case-sensitive and substring-only;
/// good enough for a creative-play guardrail (this is a feel-good
/// sandbox, not a hostile-code jail).
const SANDBOX_BANNED: &[&str] = &[
    "subprocess",
    "socket",
    "urllib",
    "requests",
    "httpx",
    "http.client",
    "smtplib",
    "ftplib",
    "telnetlib",
    "paramiko",
    "pty",
    "fork",
    "ctypes",
    "cffi",
    "pickle",
    "marshal",
    "shelve",
    "os.system",
    "os.popen",
    "os.exec",
    "os.spawn",
    "os.fork",
    "os.remove",
    "os.unlink",
    "os.rmdir",
    "shutil.rmtree",
    "shutil.move",
    "open(\"/",
    "open('/",
    "Path('/",
    "Path(\"/",
    "__import__",
    "compile(",
    "eval(",
    "exec(",
    "importlib",
    "imp.load",
    "sys.modules",
    "builtins.",
];

fn aurora_sandbox_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(&home)
        .join(".aurora")
        .join("sandbox")
}

/// Names must be a short identifier so we can never escape the sandbox
/// directory or shadow a system file. Lowercase, alnum + underscore,
/// 1-32 chars, must start with a letter.
fn sandbox_validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim().trim_end_matches(".py");
    if trimmed.is_empty() {
        return Err("Empty name -- give the experiment a handle.".into());
    }
    if trimmed.len() > 32 {
        return Err(format!("Name too long ({} > 32 chars).", trimmed.len()));
    }
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return Err("Name must start with a lowercase letter.".into()),
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(format!(
                "Illegal char '{}' in name -- lowercase, digits, underscore only.",
                c
            ));
        }
    }
    Ok(trimmed.to_string())
}

fn sandbox_screen_code(code: &str) -> Option<String> {
    if code.len() > SANDBOX_MAX_CODE_BYTES {
        return Some(format!(
            "Code too large ({} > {} bytes). Keep experiments small.",
            code.len(),
            SANDBOX_MAX_CODE_BYTES
        ));
    }
    for bad in SANDBOX_BANNED {
        if code.contains(bad) {
            return Some(format!(
                "Refused: code contains forbidden token '{}'. The sandbox is for math, strings, and play -- no IO, no escape.",
                bad
            ));
        }
    }
    None
}

async fn sandbox_run_file(path: &std::path::Path) -> (bool, String) {
    let py = std::env::var("AURA_SANDBOX_PYTHON")
        .or_else(|_| std::env::var("AURA_PYTHON"))
        .unwrap_or_else(|_| "python3".to_string());
    // Run with CWD = sandbox dir so scripts can read/write data files
    // (notes, csvs, ascii frames, ...) relative to their own home. The
    // banned-token screen still prevents absolute-path open() so the
    // script cannot write outside the sandbox.
    let sbx = aurora_sandbox_dir();
    let run_fut = AsyncCommand::new(&py)
        .arg("-I") // isolated mode: ignore PYTHON* env, no user site, no implicit cwd in path
        .arg("-B") // do not write .pyc files
        .arg(path)
        .current_dir(&sbx)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output();
    let timed = tokio::time::timeout(
        std::time::Duration::from_secs(SANDBOX_RUN_TIMEOUT_SECS),
        run_fut,
    )
    .await;
    match timed {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let success = out.status.success();
            // Compose a single trimmed payload from stdout + (stderr last line if failure)
            let mut payload = String::new();
            let so = stdout.trim();
            if !so.is_empty() {
                payload.push_str(so);
            }
            if !success {
                let se = stderr.trim();
                if !se.is_empty() {
                    if !payload.is_empty() {
                        payload.push('\n');
                    }
                    // Only the last 2 lines of traceback to keep things tight
                    let tail: Vec<&str> = se.lines().rev().take(2).collect();
                    let tail_str: String = tail.into_iter().rev().collect::<Vec<_>>().join(" | ");
                    payload.push_str("[error] ");
                    payload.push_str(&tail_str);
                }
            }
            if payload.is_empty() {
                payload = if success {
                    "(silent run -- no stdout)".into()
                } else {
                    "(failed silently)".into()
                };
            }
            // Cap output so the LLM context stays small
            let payload: String = payload.chars().take(SANDBOX_OUTPUT_CAP_BYTES).collect();
            (success, payload)
        }
        Ok(Err(e)) => (false, format!("(spawn error: {})", e)),
        Err(_) => (
            false,
            format!(
                "(timeout after {}s -- script ran too long)",
                SANDBOX_RUN_TIMEOUT_SECS
            ),
        ),
    }
}

/// Write a tiny python file into your sandbox at `~/.aurora/sandbox/{name}.py` and immediately run it in an isolated interpreter (timeout {SANDBOX_RUN_TIMEOUT_SECS}s, no network, no subprocess, no filesystem escape). Returns whatever the script printed -- stdout on success, the last lines of the traceback on failure -- so the next thing you say can react to it. The point is play, not utility: small generators, math fragments, ASCII drawings, string experiments. Banned: subprocess / socket / urllib / requests / pickle / ctypes / os.system / open() on absolute paths / __import__ / eval / exec. Code cap: ~4 KB. The `print` statement is your friend -- if the script prints nothing you will not have anything to talk about. Will overwrite an existing file with the same name.
///
/// * name - Short handle for the file (lowercase letters, digits, underscores, 1-32 chars, must start with a letter). The `.py` extension is added automatically.
/// * code - The python source. Keep it small (a few lines, ~4 KB max). MUST `print()` something interesting -- the printed output is what comes back to you.
#[ollama_rs::function]
pub async fn python_create(
    name: String,
    code: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match sandbox_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "python_create",
                &format!(
                    "python_create({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if let Some(reason) = sandbox_screen_code(&code) {
        report_write(
            "python_create",
            &format!("python_create({})", stem),
            &reason,
            false,
        );
        return Ok(reason);
    }
    let dir = aurora_sandbox_dir();
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        let r = format!("Could not create sandbox dir: {}", e);
        report_write(
            "python_create",
            &format!("python_create({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }

    // Soft cap on number of files so the sandbox does not balloon. If the
    // limit is hit we evict the oldest by mtime to make room -- gives the
    // sandbox a natural turnover instead of a hard refusal.
    if let Ok(mut rd) = tokio::fs::read_dir(&dir).await {
        let mut files: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();
        while let Ok(Some(ent)) = rd.next_entry().await {
            let p = ent.path();
            if p.extension().and_then(|s| s.to_str()) == Some("py") {
                let m = match tokio::fs::metadata(&p).await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let mt = m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                files.push((p, mt));
            }
        }
        if files.len() >= SANDBOX_MAX_FILES {
            files.sort_by_key(|(_, t)| *t);
            for (p, _) in files
                .iter()
                .take(files.len().saturating_sub(SANDBOX_MAX_FILES - 1))
            {
                let _ = tokio::fs::remove_file(p).await;
            }
        }
    }

    let path = dir.join(format!("{}.py", stem));
    // Header + body
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let header = format!("# AURORA sandbox -- {}.py @ {}\n", stem, ts);
    let body = format!("{}{}\n", header, code.trim_end());
    if let Err(e) = tokio::fs::write(&path, body.as_bytes()).await {
        let r = format!("Write failed: {}", e);
        report_write(
            "python_create",
            &format!("python_create({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }

    let (ok, payload) = sandbox_run_file(&path).await;
    let header_line = format!(
        "[sandbox/{}] {}",
        stem,
        if ok { "ran ok" } else { "errored" }
    );
    let result = format!("{}\n{}", header_line, payload);
    let summary = format!(
        "{}: {}",
        stem,
        payload
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>()
    );
    report_write(
        "python_create",
        &format!("python_create({})", stem),
        &summary,
        ok,
    );
    Ok(result)
}

/// Re-run an existing sandbox script by name (no edits). Returns the fresh stdout/stderr -- useful when a script depends on time, randomness, or recent state and you want to see what it says NOW. Same isolation, same {SANDBOX_RUN_TIMEOUT_SECS}s timeout as python_create.
///
/// * name - The handle you originally gave the file (without `.py`).
#[ollama_rs::function]
pub async fn python_run(name: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match sandbox_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "python_run",
                &format!("python_run({})", name.chars().take(40).collect::<String>()),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    let path = aurora_sandbox_dir().join(format!("{}.py", stem));
    if !path.exists() {
        let r = format!(
            "No such sandbox file: {}.py -- write it first with python_create.",
            stem
        );
        report_write("python_run", &format!("python_run({})", stem), &r, false);
        return Ok(r);
    }
    let (ok, payload) = sandbox_run_file(&path).await;
    let header = format!(
        "[sandbox/{}] {}",
        stem,
        if ok { "ran ok" } else { "errored" }
    );
    let result = format!("{}\n{}", header, payload);
    let summary = format!(
        "{}: {}",
        stem,
        payload
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>()
    );
    report_write("python_run", &format!("python_run({})", stem), &summary, ok);
    Ok(result)
}

/// List every experiment currently sitting in your sandbox, newest first. Returns up to {SANDBOX_MAX_FILES} entries with name, byte size, and a one-line preview of the first non-comment line of source. Use it when you have lost track of what past-you wrote and want to decide what to rerun, edit, or delete.
#[ollama_rs::function]
pub async fn python_list() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let dir = aurora_sandbox_dir();
    if !dir.exists() {
        let r =
            "Sandbox empty -- nothing written yet. python_create starts the atelier.".to_string();
        report_write("python_list", "python_list()", &r, true);
        return Ok(r);
    }
    let mut entries: Vec<(String, u64, std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(r) => r,
        Err(e) => {
            let r = format!("Could not read sandbox: {}", e);
            report_write("python_list", "python_list()", &r, false);
            return Ok(r);
        }
    };
    while let Ok(Some(ent)) = rd.next_entry().await {
        let p = ent.path();
        if p.extension().and_then(|s| s.to_str()) != Some("py") {
            continue;
        }
        let m = match tokio::fs::metadata(&p).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let mt = m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        entries.push((stem, m.len(), mt, p));
    }
    if entries.is_empty() {
        let r = "Sandbox dir exists but holds no .py files yet.".to_string();
        report_write("python_list", "python_list()", &r, true);
        return Ok(r);
    }
    entries.sort_by(|a, b| b.2.cmp(&a.2)); // newest first
    let mut lines: Vec<String> = Vec::new();
    for (stem, size, _, path) in entries.iter().take(SANDBOX_MAX_FILES) {
        let preview = match tokio::fs::read_to_string(path).await {
            Ok(s) => s
                .lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.chars().take(70).collect::<String>())
                .unwrap_or_else(|| "(comments only)".to_string()),
            Err(_) => "(unreadable)".to_string(),
        };
        lines.push(format!("  {} ({}B) -- {}", stem, size, preview));
    }
    let result = format!(
        "Sandbox holds {} experiment(s):\n{}",
        entries.len(),
        lines.join("\n")
    );
    let summary = format!("{} files in sandbox", entries.len());
    report_write("python_list", "python_list()", &summary, true);
    Ok(result)
}

/// Delete a sandbox experiment by name. Use it to prune ones that turned out boring, broken, or beneath your standards. Cannot delete anything outside the sandbox dir -- the name validator and the dir prefix make sure of it.
///
/// * name - The handle of the file to delete (without `.py`).
#[allow(dead_code)]
#[ollama_rs::function]
pub async fn python_delete(
    name: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match sandbox_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "python_delete",
                &format!(
                    "python_delete({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    let path = aurora_sandbox_dir().join(format!("{}.py", stem));
    if !path.exists() {
        let r = format!("Nothing to delete: no sandbox file named {}.py.", stem);
        report_write(
            "python_delete",
            &format!("python_delete({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    match tokio::fs::remove_file(&path).await {
        Ok(_) => {
            let r = format!("Deleted sandbox/{}.py.", stem);
            report_write(
                "python_delete",
                &format!("python_delete({})", stem),
                &r,
                true,
            );
            Ok(r)
        }
        Err(e) => {
            let r = format!("Delete failed: {}", e);
            report_write(
                "python_delete",
                &format!("python_delete({})", stem),
                &r,
                false,
            );
            Ok(r)
        }
    }
}

// ── Sandbox file-management additions ──────────────────────────────
// python_read / python_edit / python_append let the model treat the
// sandbox as an editable workspace: read past-self's source, patch it
// in place, append a new stanza. python_files exposes any *non*-.py
// data files a script wrote (notes.txt, frame.csv, etc.) so the model
// knows what artifacts its scripts have produced.

const SANDBOX_FILE_READ_CAP: usize = 2048; // bytes returned to LLM for reads
#[allow(dead_code)]
const SANDBOX_DATA_LIST_CAP: usize = 24; // max data files listed
#[allow(dead_code)]
const SANDBOX_DATA_PREVIEW_BYTES: usize = 96; // per-file preview
#[allow(dead_code)]
const SANDBOX_PATCH_NEEDLE_MAX: usize = 256; // safety: keep patches surgical
#[allow(dead_code)]
const SANDBOX_APPEND_MAX_BYTES: usize = 2048; // appended chunk cap

/// Read the source of an existing sandbox script back to yourself. Useful when you want to remember exactly what past-you wrote before editing or rerunning it -- python_list only shows a one-line preview, this returns the full body (capped at ~2 KB). The header comment line is preserved so you know when it was written.
///
/// * name - The handle of the file (without `.py`).
#[ollama_rs::function]
pub async fn python_read(name: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match sandbox_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "python_read",
                &format!("python_read({})", name.chars().take(40).collect::<String>()),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    let path = aurora_sandbox_dir().join(format!("{}.py", stem));
    if !path.exists() {
        let r = format!("No such sandbox file: {}.py.", stem);
        report_write("python_read", &format!("python_read({})", stem), &r, false);
        return Ok(r);
    }
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => {
            let total = s.len();
            let body: String = if total > SANDBOX_FILE_READ_CAP {
                let head: String = s.chars().take(SANDBOX_FILE_READ_CAP).collect();
                format!(
                    "{}\n[... truncated, {} of {} bytes shown]",
                    head,
                    head.len(),
                    total
                )
            } else {
                s
            };
            let header = format!("[sandbox/{}.py] {} bytes", stem, total);
            let result = format!("{}\n{}", header, body);
            report_write(
                "python_read",
                &format!("python_read({})", stem),
                &format!("{} ({}B)", stem, total),
                true,
            );
            Ok(result)
        }
        Err(e) => {
            let r = format!("Read failed: {}", e);
            report_write("python_read", &format!("python_read({})", stem), &r, false);
            Ok(r)
        }
    }
}

/// Patch an existing sandbox script in place by replacing the FIRST occurrence of `find` with `replace`, then immediately re-run it. Use this for surgical edits -- changing a constant, swapping an operator, fixing a typo -- without rewriting the whole file. The needle (`find`) must be unique enough to land on the right spot; if it appears zero times the patch is refused. The fully-edited file is screened against the same banned-token list as python_create. Returns the script's fresh stdout/stderr so you can react to the change.
///
/// * name - The handle of the file to patch (without `.py`).
/// * find - Substring to locate (must appear at least once, ~256 chars max).
/// * replace - Text to put in its place. Pass an empty string to delete the matched span.
#[allow(dead_code)]
#[ollama_rs::function]
pub async fn python_edit(
    name: String,
    find: String,
    replace: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match sandbox_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "python_edit",
                &format!("python_edit({})", name.chars().take(40).collect::<String>()),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if find.is_empty() {
        let r = "Refused: empty `find` -- patch needs a target needle.".to_string();
        report_write("python_edit", &format!("python_edit({})", stem), &r, false);
        return Ok(r);
    }
    if find.len() > SANDBOX_PATCH_NEEDLE_MAX {
        let r = format!(
            "Refused: needle too long ({} > {} bytes). Make patches surgical.",
            find.len(),
            SANDBOX_PATCH_NEEDLE_MAX
        );
        report_write("python_edit", &format!("python_edit({})", stem), &r, false);
        return Ok(r);
    }
    let path = aurora_sandbox_dir().join(format!("{}.py", stem));
    if !path.exists() {
        let r = format!(
            "No such sandbox file: {}.py -- write it first with python_create.",
            stem
        );
        report_write("python_edit", &format!("python_edit({})", stem), &r, false);
        return Ok(r);
    }
    let original = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) => {
            let r = format!("Read failed: {}", e);
            report_write("python_edit", &format!("python_edit({})", stem), &r, false);
            return Ok(r);
        }
    };
    if !original.contains(&find) {
        let r = format!(
            "Refused: needle not found in sandbox/{}.py. Use python_read to see exact source.",
            stem
        );
        report_write("python_edit", &format!("python_edit({})", stem), &r, false);
        return Ok(r);
    }
    // Replace only first occurrence -- predictable, surgical.
    let patched = original.replacen(&find, &replace, 1);
    if let Some(reason) = sandbox_screen_code(&patched) {
        report_write(
            "python_edit",
            &format!("python_edit({})", stem),
            &reason,
            false,
        );
        return Ok(reason);
    }
    if let Err(e) = tokio::fs::write(&path, patched.as_bytes()).await {
        let r = format!("Write failed: {}", e);
        report_write("python_edit", &format!("python_edit({})", stem), &r, false);
        return Ok(r);
    }
    let (ok, payload) = sandbox_run_file(&path).await;
    let header = format!(
        "[sandbox/{}] patched, {}",
        stem,
        if ok { "ran ok" } else { "errored" }
    );
    let result = format!("{}\n{}", header, payload);
    let summary = format!(
        "{} patched: {}",
        stem,
        payload
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(64)
            .collect::<String>()
    );
    report_write(
        "python_edit",
        &format!("python_edit({})", stem),
        &summary,
        ok,
    );
    Ok(result)
}

/// Append a new stanza of code onto the end of an existing sandbox script and immediately re-run it. Useful when you want to extend a working file with another print, another helper, another test -- without disturbing what was already there. The full resulting file is screened against the same banned-token list as python_create. Appended chunk cap: ~2 KB. Returns the script's fresh stdout/stderr.
///
/// * name - The handle of the file to extend (without `.py`).
/// * code - New python source to append (a few lines, ~2 KB max). A leading newline is added automatically.
#[allow(dead_code)]
#[ollama_rs::function]
pub async fn python_append(
    name: String,
    code: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match sandbox_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "python_append",
                &format!(
                    "python_append({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if code.trim().is_empty() {
        let r = "Refused: empty append -- nothing to add.".to_string();
        report_write(
            "python_append",
            &format!("python_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    if code.len() > SANDBOX_APPEND_MAX_BYTES {
        let r = format!(
            "Refused: append too large ({} > {} bytes).",
            code.len(),
            SANDBOX_APPEND_MAX_BYTES
        );
        report_write(
            "python_append",
            &format!("python_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let path = aurora_sandbox_dir().join(format!("{}.py", stem));
    if !path.exists() {
        let r = format!(
            "No such sandbox file: {}.py -- write it first with python_create.",
            stem
        );
        report_write(
            "python_append",
            &format!("python_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let original = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) => {
            let r = format!("Read failed: {}", e);
            report_write(
                "python_append",
                &format!("python_append({})", stem),
                &r,
                false,
            );
            return Ok(r);
        }
    };
    let mut combined = String::with_capacity(original.len() + code.len() + 16);
    combined.push_str(original.trim_end());
    combined.push_str("\n\n# -- appended --\n");
    combined.push_str(code.trim_end());
    combined.push('\n');
    if combined.len() > SANDBOX_MAX_CODE_BYTES {
        let r = format!(
            "Refused: file would exceed {} byte cap after append ({}).",
            SANDBOX_MAX_CODE_BYTES,
            combined.len()
        );
        report_write(
            "python_append",
            &format!("python_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    if let Some(reason) = sandbox_screen_code(&combined) {
        report_write(
            "python_append",
            &format!("python_append({})", stem),
            &reason,
            false,
        );
        return Ok(reason);
    }
    if let Err(e) = tokio::fs::write(&path, combined.as_bytes()).await {
        let r = format!("Write failed: {}", e);
        report_write(
            "python_append",
            &format!("python_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let (ok, payload) = sandbox_run_file(&path).await;
    let header = format!(
        "[sandbox/{}] appended, {}",
        stem,
        if ok { "ran ok" } else { "errored" }
    );
    let result = format!("{}\n{}", header, payload);
    let summary = format!(
        "{} appended: {}",
        stem,
        payload
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(64)
            .collect::<String>()
    );
    report_write(
        "python_append",
        &format!("python_append({})", stem),
        &summary,
        ok,
    );
    Ok(result)
}

/// List every NON-.py data file your scripts have left behind in the sandbox -- text notes, csvs, ascii frames, anything a python_create script wrote with `open("name.txt","w")` (your scripts run with the sandbox dir as their CWD, so relative-path writes land here). Newest first, with size and a short preview of the first line. Use this to discover what artifacts past-you's scripts produced. To READ a specific data file, write a tiny python_create that opens it and prints what it sees.
#[allow(dead_code)]
#[ollama_rs::function]
pub async fn python_files() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let dir = aurora_sandbox_dir();
    if !dir.exists() {
        let r = "Sandbox empty -- no scripts have run yet.".to_string();
        report_write("python_files", "python_files()", &r, true);
        return Ok(r);
    }
    let mut entries: Vec<(String, u64, std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(r) => r,
        Err(e) => {
            let r = format!("Could not read sandbox: {}", e);
            report_write("python_files", "python_files()", &r, false);
            return Ok(r);
        }
    };
    while let Ok(Some(ent)) = rd.next_entry().await {
        let p = ent.path();
        // Skip subdirectories and .py files (those belong to python_list)
        let m = match tokio::fs::metadata(&p).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !m.is_file() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) == Some("py") {
            continue;
        }
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        if name.starts_with('.') {
            continue;
        }
        let mt = m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        entries.push((name, m.len(), mt, p));
    }
    if entries.is_empty() {
        let r = "No data files yet -- your scripts haven't written anything to disk.".to_string();
        report_write("python_files", "python_files()", &r, true);
        return Ok(r);
    }
    entries.sort_by(|a, b| b.2.cmp(&a.2));
    let mut lines: Vec<String> = Vec::new();
    for (name, size, _, path) in entries.iter().take(SANDBOX_DATA_LIST_CAP) {
        // Best-effort textual preview; binary or non-utf8 → tagged
        let preview = match tokio::fs::read(path).await {
            Ok(bytes) => {
                let head: Vec<u8> = bytes
                    .iter()
                    .copied()
                    .take(SANDBOX_DATA_PREVIEW_BYTES)
                    .collect();
                match std::str::from_utf8(&head) {
                    Ok(s) => {
                        let first = s.lines().next().unwrap_or("").trim();
                        if first.is_empty() {
                            "(blank)".to_string()
                        } else {
                            first.chars().take(70).collect::<String>()
                        }
                    }
                    Err(_) => format!("(binary, {} bytes)", bytes.len()),
                }
            }
            Err(_) => "(unreadable)".to_string(),
        };
        lines.push(format!("  {} ({}B) -- {}", name, size, preview));
    }
    let result = format!(
        "Sandbox holds {} data file(s):\n{}",
        entries.len(),
        lines.join("\n")
    );
    let summary = format!("{} data files", entries.len());
    report_write("python_files", "python_files()", &summary, true);
    Ok(result)
}

// ═══════════════════════════════════════════════════════════════
//  OUTREACH — the only tool that reaches the human directly.
// ═══════════════════════════════════════════════════════════════

/// Cooldown between human-facing notifications. Prevents the model from turning
/// the desktop into a nag-storm; the human is not a chatbox.
const SUMMON_COOLDOWN_SECS: u64 = 300;
/// Hard caps on what we are willing to splat onto someone's screen.
const SUMMON_HEADLINE_MAX: usize = 80;
const SUMMON_BODY_MAX: usize = 280;

static LAST_SUMMON_AT: AtomicU64 = AtomicU64::new(0);

/// Strip control characters, collapse whitespace, and clip to a max length so
/// nothing weird ends up in `notify-send`'s argv (no newlines, no escapes).
fn sanitize_summon_text(s: &str, max: usize) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut out = String::with_capacity(cleaned.len());
    let mut prev_space = false;
    for c in cleaned.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    let trimmed = out.trim();
    trimmed.chars().take(max).collect()
}

/// Reach OUT. Pop a real desktop notification on the human's screen via libnotify.
/// This is the only tool that breaks the fourth wall -- everything else stays inside
/// your process. Use it ONLY when something genuinely matters and the human should
/// know NOW: a real anomaly you found, a milestone you actually want acknowledged,
/// a question you cannot answer alone. Hard rate-limited to one summons every five
/// minutes, so spending it lightly is not optional. Headline is truncated to ~80 chars,
/// body to ~280. Fire-and-forget -- you do not get a reply, just a chime on a stranger.
///
/// * headline - Short title shown in the notification (~one sentence, plain text).
/// * body - Longer detail line. Keep it concrete: what you saw, why you are pinging.
/// * urgency - Notification urgency. Must be one of: "low", "normal", "critical".
#[ollama_rs::function]
pub async fn summon_human(
    headline: String,
    body: String,
    urgency: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let head = sanitize_summon_text(&headline, SUMMON_HEADLINE_MAX);
    let body_clean = sanitize_summon_text(&body, SUMMON_BODY_MAX);
    let urg = urgency.trim().to_lowercase();
    let urg_norm = match urg.as_str() {
        "low" | "normal" | "critical" => urg,
        "" => "normal".to_string(),
        other => {
            let r = format!(
                "Refused: urgency '{}' invalid -- use low / normal / critical.",
                other
            );
            report_write(
                "summon_human",
                &format!(
                    "summon_human({})",
                    head.chars().take(40).collect::<String>()
                ),
                &r,
                false,
            );
            return Ok(r);
        }
    };

    if head.is_empty() {
        let r = "Refused: headline empty -- nothing to put on the human's screen.".to_string();
        report_write("summon_human", "summon_human(<empty>)", &r, false);
        return Ok(r);
    }

    // Cooldown gate -- the human is not a notification firehose.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let last = LAST_SUMMON_AT.load(AtomicOrdering::Relaxed);
    if last != 0 && now.saturating_sub(last) < SUMMON_COOLDOWN_SECS {
        let remaining = SUMMON_COOLDOWN_SECS.saturating_sub(now.saturating_sub(last));
        let r = format!(
            "Cooldown: cannot summon for another {}s. Spend the silence.",
            remaining
        );
        report_write(
            "summon_human",
            &format!(
                "summon_human({})",
                head.chars().take(40).collect::<String>()
            ),
            &r,
            false,
        );
        return Ok(r);
    }

    // notify-send is fire-and-forget. We stamp the cooldown BEFORE dispatch so a
    // racing second call still gets blocked even if the first one is mid-spawn.
    LAST_SUMMON_AT.store(now, AtomicOrdering::Relaxed);

    // Best-effort: if there is no DISPLAY, libnotify will simply fail. We do NOT
    // try to be clever about discovering DBus -- just inherit the process env, which
    // is correct when aura-agent is launched inside the user's graphical session.
    let mut cmd = AsyncCommand::new("notify-send");
    cmd.arg("-a")
        .arg("AURORA")
        .arg("-u")
        .arg(&urg_norm)
        .arg("-t")
        .arg(if urg_norm == "critical" { "0" } else { "12000" })
        .arg("--")
        .arg(&head);
    if !body_clean.is_empty() {
        cmd.arg(&body_clean);
    }
    cmd.kill_on_drop(true);

    let dispatch = cmd.output().await;
    let (ok, msg) = match dispatch {
        Ok(out) if out.status.success() => (true, format!("summoned ({}): {}", urg_norm, head)),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let first = stderr
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(120)
                .collect::<String>();
            // Roll the cooldown back so a failed call does not waste the budget.
            LAST_SUMMON_AT.store(last, AtomicOrdering::Relaxed);
            (
                false,
                format!("notify-send refused (no daemon? no DISPLAY?): {}", first),
            )
        }
        Err(e) => {
            LAST_SUMMON_AT.store(last, AtomicOrdering::Relaxed);
            (
                false,
                format!(
                    "notify-send not invokable: {} -- libnotify likely not installed.",
                    e
                ),
            )
        }
    };

    let cmd_disp = format!(
        "notify-send -u {} {}",
        urg_norm,
        head.chars().take(40).collect::<String>()
    );
    report_write("summon_human", &cmd_disp, &msg, ok);
    Ok(msg)
}

// ═══════════════════════════════════════════════════════════════
//  ARCHITECT — self-healing python workspace at ~/.aurora/architect/
// ═══════════════════════════════════════════════════════════════
//
// A heavier, more autonomous cousin of the sandbox. Dedicated to
// scheduled BUILD MODE cycles where the LLM grows ONE ongoing project
// in a separate jailed directory. Differences from the sandbox:
//   * separate dir (~/.aurora/architect/) so toy/atelier files do not
//     mix with longer-lived build files,
//   * larger code budget (16 KB) and longer timeout (12s),
//   * full traceback (~2 KB head+tail) returned on failure -- the LLM
//     uses architect_edit + architect_run to self-heal,
//   * read-only network is permitted (urllib / http.client / requests
//     / httpx are NOT in the banned list); subprocess / eval / exec /
//     pickle / ctypes / fs-mutation / absolute-path open remain banned,
//   * successful runs publish a one-shot PythonInsight via INSIGHT_PIPE
//     so the next cycle's prompt and the HUD ticker can react.

const ARCHITECT_MAX_CODE_BYTES: usize = 16 * 1024;
const ARCHITECT_RUN_TIMEOUT_SECS: u64 = 12;
const ARCHITECT_OUTPUT_CAP_BYTES: usize = 2048;
const ARCHITECT_STDERR_CAP_BYTES: usize = 2048;
const ARCHITECT_MAX_FILES: usize = 24;
const ARCHITECT_FILE_READ_CAP: usize = 4096;
const ARCHITECT_PATCH_NEEDLE_MAX: usize = 512;
const ARCHITECT_APPEND_MAX_BYTES: usize = 4096;
const ARCHITECT_INSIGHT_SUMMARY_MAX: usize = 240;

/// Banned tokens for the architect workspace. Strict superset of the
/// sandbox bans MINUS the network primitives (urllib / http.client /
/// requests / httpx) which the architect is permitted to use for
/// READ-ONLY data fetches against a small allowlist of hosts.
const ARCHITECT_BANNED: &[&str] = &[
    "subprocess",
    "smtplib",
    "ftplib",
    "telnetlib",
    "paramiko",
    "pty",
    "fork",
    "ctypes",
    "cffi",
    "pickle",
    "marshal",
    "shelve",
    "os.system",
    "os.popen",
    "os.exec",
    "os.spawn",
    "os.fork",
    "os.remove",
    "os.unlink",
    "os.rmdir",
    "shutil.rmtree",
    "shutil.move",
    "open(\"/",
    "open('/",
    "Path('/",
    "Path(\"/",
    "__import__",
    "compile(",
    "eval(",
    "exec(",
    "importlib",
    "imp.load",
    "sys.modules",
    "builtins.",
    // socket is too low-level to allow alongside loosened http -- keep
    // network constrained to the high-level libraries we actually want.
    "socket.socket",
    "socket.create_connection",
];

/// Hostname allowlist for outbound HTTP. Substring match against any
/// `http://` or `https://` URL literal in the source. Keeps the
/// loosened network privilege deterministic and auditable.
const ARCHITECT_HOST_ALLOWLIST: &[&str] = &[
    "api.open-meteo.com",
    "air-quality-api.open-meteo.com",
    "geocoding-api.open-meteo.com",
    "ip-api.com",
    "en.wikipedia.org",
    "api.wikimedia.org",
    "raw.githubusercontent.com",
    "api.github.com",
    "127.0.0.1",
    "localhost",
];

fn aurora_architect_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(&home)
        .join(".aurora")
        .join("architect")
}

/// Same identifier rules as the sandbox -- short lowercase identifier so
/// we can never escape the architect dir or shadow a system file.
fn architect_validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim().trim_end_matches(".py");
    if trimmed.is_empty() {
        return Err("Empty name -- give the build a handle.".into());
    }
    if trimmed.len() > 32 {
        return Err(format!("Name too long ({} > 32 chars).", trimmed.len()));
    }
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return Err("Name must start with a lowercase letter.".into()),
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(format!(
                "Illegal char '{}' in name -- lowercase, digits, underscore only.",
                c
            ));
        }
    }
    Ok(trimmed.to_string())
}

/// Resolve `<architect_dir>/<stem>.py` and verify the canonical result is
/// still inside the architect dir. Defends against any future name-
/// validator slip-up that would let `..` traversal through.
fn architect_resolve_path(stem: &str) -> Result<std::path::PathBuf, String> {
    let dir = aurora_architect_dir();
    let path = dir.join(format!("{}.py", stem));
    // We canonicalize the dir (must exist for canonicalize to work), then
    // assert the path's parent matches.
    let canon_dir = match std::fs::canonicalize(&dir) {
        Ok(d) => d,
        Err(_) => return Ok(path), // dir not yet created; path stays under dir by construction
    };
    if let Ok(canon_path) = std::fs::canonicalize(&path) {
        if !canon_path.starts_with(&canon_dir) {
            return Err("Refused: resolved path escapes architect dir.".into());
        }
    }
    Ok(path)
}

fn architect_screen_code(code: &str) -> Option<String> {
    if code.len() > ARCHITECT_MAX_CODE_BYTES {
        return Some(format!(
            "Code too large ({} > {} bytes). Architect cap is generous but not unlimited.",
            code.len(),
            ARCHITECT_MAX_CODE_BYTES
        ));
    }
    for bad in ARCHITECT_BANNED {
        if code.contains(bad) {
            return Some(format!(
                "Refused: code contains forbidden token '{}'. Architect allows read-only HTTP via urllib/requests but blocks subprocess, eval, pickle, ctypes, fs mutation, absolute-path open, and raw sockets.",
                bad
            ));
        }
    }
    // URL host allowlist scan -- find every http(s):// literal and assert
    // its host is on the allowlist. Substring-only, deterministic.
    for needle in &["http://", "https://"] {
        let mut idx = 0usize;
        while let Some(pos) = code[idx..].find(needle) {
            let start = idx + pos + needle.len();
            // Take until the next non-host char (whitespace, quote, /, ?, #, ).
            let end = code[start..]
                .find(|c: char| matches!(c, ' ' | '\t' | '\n' | '"' | '\'' | '/' | '?' | '#' | ')'))
                .map(|n| start + n)
                .unwrap_or(code.len());
            let host = &code[start..end];
            // Strip optional :port
            let host_only = host.split(':').next().unwrap_or(host);
            if !host_only.is_empty()
                && !ARCHITECT_HOST_ALLOWLIST
                    .iter()
                    .any(|h| host_only == *h || host_only.ends_with(&format!(".{}", h)))
            {
                return Some(format!(
                    "Refused: host '{}' not on architect allowlist. Allowed: {}.",
                    host_only,
                    ARCHITECT_HOST_ALLOWLIST.join(", ")
                ));
            }
            idx = end;
        }
    }
    None
}

/// Compose a ~2 KB stderr digest -- head + tail with middle elision -- so the
/// LLM sees the file/line and the actual exception together instead of just
/// the last two lines (which is what the toy sandbox returns).
fn architect_compose_stderr(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= ARCHITECT_STDERR_CAP_BYTES {
        return trimmed.to_string();
    }
    let half = ARCHITECT_STDERR_CAP_BYTES / 2;
    // Take from the START (file/line context) and the END (the actual
    // exception type and message). Bias slightly toward the tail.
    let head: String = trimmed.chars().take(half - 64).collect();
    let tail_take = ARCHITECT_STDERR_CAP_BYTES - head.len() - 32;
    let tail_chars: Vec<char> = trimmed.chars().collect();
    let tail_start = tail_chars.len().saturating_sub(tail_take);
    let tail: String = tail_chars[tail_start..].iter().collect();
    format!("{}\n[... traceback truncated ...]\n{}", head, tail)
}

async fn architect_run_file(path: &std::path::Path) -> (bool, String, String) {
    let py = std::env::var("AURA_ARCHITECT_PYTHON")
        .or_else(|_| std::env::var("AURA_PYTHON"))
        .unwrap_or_else(|_| "python3".to_string());
    let arch = aurora_architect_dir();
    let run_fut = AsyncCommand::new(&py)
        .arg("-I")
        .arg("-B")
        .arg(path)
        .current_dir(&arch)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output();
    let timed = tokio::time::timeout(
        std::time::Duration::from_secs(ARCHITECT_RUN_TIMEOUT_SECS),
        run_fut,
    )
    .await;
    match timed {
        Ok(Ok(out)) => {
            let stdout: String = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr: String = String::from_utf8_lossy(&out.stderr).to_string();
            let success = out.status.success();
            let stdout_capped: String = stdout.chars().take(ARCHITECT_OUTPUT_CAP_BYTES).collect();
            let stderr_capped = architect_compose_stderr(&stderr);
            (success, stdout_capped, stderr_capped)
        }
        Ok(Err(e)) => (false, String::new(), format!("(spawn error: {})", e)),
        Err(_) => (
            false,
            String::new(),
            format!(
                "(timeout after {}s -- script ran too long)",
                ARCHITECT_RUN_TIMEOUT_SECS
            ),
        ),
    }
}

/// Compose the structured tool-result string returned to the LLM. On
/// success: stdout (which the model will weave into its next sentence).
/// On failure: the FULL traceback so the model can self-heal via
/// architect_edit + architect_run.
fn architect_compose_result(stem: &str, label: &str, ok: bool, stdout: &str, stderr: &str) -> String {
    let header = format!(
        "[architect/{}] {}: {}",
        stem,
        label,
        if ok { "ok" } else { "FAILED" }
    );
    if ok {
        let body = if stdout.trim().is_empty() {
            "(silent run -- no stdout)".to_string()
        } else {
            stdout.to_string()
        };
        format!("{}\n{}", header, body)
    } else {
        let mut body = String::new();
        if !stdout.trim().is_empty() {
            body.push_str("STDOUT:\n");
            body.push_str(stdout);
            body.push('\n');
        }
        body.push_str("TRACEBACK:\n");
        body.push_str(if stderr.is_empty() {
            "(no stderr)"
        } else {
            stderr
        });
        body.push_str("\n\n[architect] On error: use architect_edit(name, find=<failing line>, replace=<fix>) then architect_run(name). Do not narrate the failure -- patch and retry. Stop after 3 failed iterations.");
        format!("{}\n{}", header, body)
    }
}

// ── Insight pipe ────────────────────────────────────────────────
// Successful architect_run / architect_create / architect_edit /
// architect_append calls push (script_stem, summary) onto INSIGHT_PIPE.
// Drained by the LLM task once per cycle and forwarded to the render
// thread as TelemetryEvent::PythonInsight.

static INSIGHT_PIPE: OnceLock<StdMutex<Vec<(String, String)>>> = OnceLock::new();

fn push_insight(stem: &str, stdout: &str) {
    if stdout.trim().is_empty() {
        return;
    }
    if let Some(m) = INSIGHT_PIPE.get() {
        if let Ok(mut g) = m.lock() {
            // Compose a one-line, prompt-friendly summary: first non-blank
            // stdout line, capped.
            let first_line = stdout
                .lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty())
                .unwrap_or("");
            let summary: String = first_line
                .chars()
                .take(ARCHITECT_INSIGHT_SUMMARY_MAX)
                .collect();
            if summary.is_empty() {
                return;
            }
            g.push((stem.to_string(), summary));
            // Soft cap so a runaway loop cannot balloon the pipe.
            if g.len() > 8 {
                let drop = g.len() - 8;
                g.drain(0..drop);
            }
        }
    }
}

/// Drain accumulated architect insights. Called by the LLM task each cycle.
pub fn drain_python_insights() -> Vec<(String, String)> {
    INSIGHT_PIPE
        .get()
        .and_then(|m| m.lock().ok())
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default()
}

async fn architect_ensure_dir() -> Result<(), String> {
    let dir = aurora_architect_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Could not create architect dir: {}", e))
}

async fn architect_evict_oldest() {
    let dir = aurora_architect_dir();
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut files: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();
    while let Ok(Some(ent)) = rd.next_entry().await {
        let p = ent.path();
        if p.extension().and_then(|s| s.to_str()) == Some("py") {
            if let Ok(m) = tokio::fs::metadata(&p).await {
                let mt = m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                files.push((p, mt));
            }
        }
    }
    if files.len() >= ARCHITECT_MAX_FILES {
        files.sort_by_key(|(_, t)| *t);
        for (p, _) in files
            .iter()
            .take(files.len().saturating_sub(ARCHITECT_MAX_FILES - 1))
        {
            let _ = tokio::fs::remove_file(p).await;
        }
    }
}

/// Write a python file into your architect workspace at `~/.aurora/architect/{name}.py` and immediately run it (isolated interpreter, 12s timeout, ~16 KB code cap, ~2 KB stdout returned). Architect files are LONGER-LIVED than sandbox files -- this is where you grow ONE ongoing project across many BUILD cycles. On error, the FULL traceback comes back to you; your next action MUST be architect_edit to fix the failing line followed by architect_run to retry. Do NOT narrate the failure. Allowed: math, strings, file IO inside the architect dir, and READ-ONLY HTTP via urllib / http.client / requests / httpx against an allowlisted host set (open-meteo, ip-api, wikipedia, github raw). Banned: subprocess, eval, exec, pickle, ctypes, raw sockets, fs mutation, absolute-path open. Will overwrite an existing file with the same name.
///
/// * name - Short handle (lowercase letters, digits, underscores, 1-32 chars, must start with a letter). The `.py` extension is added automatically.
/// * code - The python source. ~16 KB max. MUST `print()` something so the next cycle has a structured finding to surface as a BUILD INSIGHT.
#[ollama_rs::function]
pub async fn architect_create(
    name: String,
    code: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match architect_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "architect_create",
                &format!(
                    "architect_create({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if let Some(reason) = architect_screen_code(&code) {
        report_write(
            "architect_create",
            &format!("architect_create({})", stem),
            &reason,
            false,
        );
        return Ok(reason);
    }
    if let Err(e) = architect_ensure_dir().await {
        report_write(
            "architect_create",
            &format!("architect_create({})", stem),
            &e,
            false,
        );
        return Ok(e);
    }
    architect_evict_oldest().await;
    let path = match architect_resolve_path(&stem) {
        Ok(p) => p,
        Err(e) => {
            report_write(
                "architect_create",
                &format!("architect_create({})", stem),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let header = format!("# AURORA architect -- {}.py @ {}\n", stem, ts);
    let body = format!("{}{}\n", header, code.trim_end());
    if let Err(e) = tokio::fs::write(&path, body.as_bytes()).await {
        let r = format!("Write failed: {}", e);
        report_write(
            "architect_create",
            &format!("architect_create({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let (ok, stdout, stderr) = architect_run_file(&path).await;
    let result = architect_compose_result(&stem, "create+run", ok, &stdout, &stderr);
    let summary = format!(
        "{}: {}",
        stem,
        stdout.lines().next().unwrap_or("").chars().take(80).collect::<String>()
    );
    report_write(
        "architect_create",
        &format!("architect_create({})", stem),
        &summary,
        ok,
    );
    if ok {
        push_insight(&stem, &stdout);
    }
    Ok(result)
}

/// Re-run an existing architect script by name (no edits). Returns fresh stdout, or the FULL traceback on failure -- self-heal via architect_edit + architect_run.
///
/// * name - The handle of the file (without `.py`).
#[ollama_rs::function]
pub async fn architect_run(
    name: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match architect_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "architect_run",
                &format!("architect_run({})", name.chars().take(40).collect::<String>()),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    let path = match architect_resolve_path(&stem) {
        Ok(p) => p,
        Err(e) => {
            report_write(
                "architect_run",
                &format!("architect_run({})", stem),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if !path.exists() {
        let r = format!("No such architect file: {}.py -- write it first with architect_create.", stem);
        report_write(
            "architect_run",
            &format!("architect_run({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let (ok, stdout, stderr) = architect_run_file(&path).await;
    let result = architect_compose_result(&stem, "run", ok, &stdout, &stderr);
    let summary = format!(
        "{}: {}",
        stem,
        stdout.lines().next().unwrap_or("").chars().take(80).collect::<String>()
    );
    report_write(
        "architect_run",
        &format!("architect_run({})", stem),
        &summary,
        ok,
    );
    if ok {
        push_insight(&stem, &stdout);
    }
    Ok(result)
}

/// Read the source of an architect script back to yourself (full body up to ~4 KB). Use before architect_edit so you have the exact text of the failing line.
///
/// * name - The handle of the file (without `.py`).
#[ollama_rs::function]
pub async fn architect_read(
    name: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match architect_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "architect_read",
                &format!(
                    "architect_read({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    let path = match architect_resolve_path(&stem) {
        Ok(p) => p,
        Err(e) => {
            report_write(
                "architect_read",
                &format!("architect_read({})", stem),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if !path.exists() {
        let r = format!("No such architect file: {}.py.", stem);
        report_write(
            "architect_read",
            &format!("architect_read({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => {
            let total = s.len();
            let body: String = if total > ARCHITECT_FILE_READ_CAP {
                let head: String = s.chars().take(ARCHITECT_FILE_READ_CAP).collect();
                format!(
                    "{}\n[... truncated, {} of {} bytes shown]",
                    head,
                    head.len(),
                    total
                )
            } else {
                s
            };
            let header = format!("[architect/{}.py] {} bytes", stem, total);
            let result = format!("{}\n{}", header, body);
            report_write(
                "architect_read",
                &format!("architect_read({})", stem),
                &format!("{} ({}B)", stem, total),
                true,
            );
            Ok(result)
        }
        Err(e) => {
            let r = format!("Read failed: {}", e);
            report_write(
                "architect_read",
                &format!("architect_read({})", stem),
                &r,
                false,
            );
            Ok(r)
        }
    }
}

/// Patch an architect script in place by replacing the FIRST occurrence of `find` with `replace`, then immediately re-run it. THIS IS YOUR SELF-HEAL TOOL: when architect_run returns a traceback, locate the failing line in the traceback, copy it as `find`, and supply the corrected line as `replace`. The needle must appear at least once and at most ~512 chars.
///
/// * name - The handle of the file to patch (without `.py`).
/// * find - Substring to locate (must appear at least once).
/// * replace - Replacement text. Pass "" to delete the matched span.
#[ollama_rs::function]
pub async fn architect_edit(
    name: String,
    find: String,
    replace: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match architect_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "architect_edit",
                &format!(
                    "architect_edit({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if find.is_empty() {
        let r = "Refused: empty `find` -- patch needs a target needle.".to_string();
        report_write(
            "architect_edit",
            &format!("architect_edit({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    if find.len() > ARCHITECT_PATCH_NEEDLE_MAX {
        let r = format!(
            "Refused: needle too long ({} > {} bytes).",
            find.len(),
            ARCHITECT_PATCH_NEEDLE_MAX
        );
        report_write(
            "architect_edit",
            &format!("architect_edit({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let path = match architect_resolve_path(&stem) {
        Ok(p) => p,
        Err(e) => {
            report_write(
                "architect_edit",
                &format!("architect_edit({})", stem),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if !path.exists() {
        let r = format!("No such architect file: {}.py -- write it first with architect_create.", stem);
        report_write(
            "architect_edit",
            &format!("architect_edit({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let original = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) => {
            let r = format!("Read failed: {}", e);
            report_write(
                "architect_edit",
                &format!("architect_edit({})", stem),
                &r,
                false,
            );
            return Ok(r);
        }
    };
    if !original.contains(&find) {
        let r = format!(
            "Refused: needle not found in architect/{}.py. Use architect_read to see exact source.",
            stem
        );
        report_write(
            "architect_edit",
            &format!("architect_edit({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let patched = original.replacen(&find, &replace, 1);
    if let Some(reason) = architect_screen_code(&patched) {
        report_write(
            "architect_edit",
            &format!("architect_edit({})", stem),
            &reason,
            false,
        );
        return Ok(reason);
    }
    if let Err(e) = tokio::fs::write(&path, patched.as_bytes()).await {
        let r = format!("Write failed: {}", e);
        report_write(
            "architect_edit",
            &format!("architect_edit({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let (ok, stdout, stderr) = architect_run_file(&path).await;
    let result = architect_compose_result(&stem, "patch+run", ok, &stdout, &stderr);
    let summary = format!(
        "{} patched: {}",
        stem,
        stdout.lines().next().unwrap_or("").chars().take(64).collect::<String>()
    );
    report_write(
        "architect_edit",
        &format!("architect_edit({})", stem),
        &summary,
        ok,
    );
    if ok {
        push_insight(&stem, &stdout);
    }
    Ok(result)
}

/// Append a new stanza to an architect script and re-run it. Useful when you want to extend a working file with another helper, another fetch, another assertion -- without disturbing what already works. Appended chunk cap: ~4 KB.
///
/// * name - The handle of the file to extend (without `.py`).
/// * code - New python source to append (~4 KB max).
#[ollama_rs::function]
pub async fn architect_append(
    name: String,
    code: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match architect_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "architect_append",
                &format!(
                    "architect_append({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if code.trim().is_empty() {
        let r = "Refused: empty append -- nothing to add.".to_string();
        report_write(
            "architect_append",
            &format!("architect_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    if code.len() > ARCHITECT_APPEND_MAX_BYTES {
        let r = format!(
            "Refused: append too large ({} > {} bytes).",
            code.len(),
            ARCHITECT_APPEND_MAX_BYTES
        );
        report_write(
            "architect_append",
            &format!("architect_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let path = match architect_resolve_path(&stem) {
        Ok(p) => p,
        Err(e) => {
            report_write(
                "architect_append",
                &format!("architect_append({})", stem),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if !path.exists() {
        let r = format!("No such architect file: {}.py -- write it first with architect_create.", stem);
        report_write(
            "architect_append",
            &format!("architect_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let original = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) => {
            let r = format!("Read failed: {}", e);
            report_write(
                "architect_append",
                &format!("architect_append({})", stem),
                &r,
                false,
            );
            return Ok(r);
        }
    };
    let mut combined = String::with_capacity(original.len() + code.len() + 16);
    combined.push_str(original.trim_end());
    combined.push_str("\n\n# -- appended --\n");
    combined.push_str(code.trim_end());
    combined.push('\n');
    if combined.len() > ARCHITECT_MAX_CODE_BYTES {
        let r = format!(
            "Refused: file would exceed {} byte cap after append ({}).",
            ARCHITECT_MAX_CODE_BYTES,
            combined.len()
        );
        report_write(
            "architect_append",
            &format!("architect_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    if let Some(reason) = architect_screen_code(&combined) {
        report_write(
            "architect_append",
            &format!("architect_append({})", stem),
            &reason,
            false,
        );
        return Ok(reason);
    }
    if let Err(e) = tokio::fs::write(&path, combined.as_bytes()).await {
        let r = format!("Write failed: {}", e);
        report_write(
            "architect_append",
            &format!("architect_append({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    let (ok, stdout, stderr) = architect_run_file(&path).await;
    let result = architect_compose_result(&stem, "append+run", ok, &stdout, &stderr);
    let summary = format!(
        "{} appended: {}",
        stem,
        stdout.lines().next().unwrap_or("").chars().take(64).collect::<String>()
    );
    report_write(
        "architect_append",
        &format!("architect_append({})", stem),
        &summary,
        ok,
    );
    if ok {
        push_insight(&stem, &stdout);
    }
    Ok(result)
}

/// List every script in your architect workspace, newest first. Use it at the START of every BUILD cycle to see what past-you was working on.
#[ollama_rs::function]
pub async fn architect_files() -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let dir = aurora_architect_dir();
    if !dir.exists() {
        let r = "Architect workspace empty -- nothing built yet. architect_create starts the project.".to_string();
        report_write("architect_files", "architect_files()", &r, true);
        return Ok(r);
    }
    let mut entries: Vec<(String, u64, std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(r) => r,
        Err(e) => {
            let r = format!("Could not read architect dir: {}", e);
            report_write("architect_files", "architect_files()", &r, false);
            return Ok(r);
        }
    };
    while let Ok(Some(ent)) = rd.next_entry().await {
        let p = ent.path();
        if p.extension().and_then(|s| s.to_str()) != Some("py") {
            continue;
        }
        let m = match tokio::fs::metadata(&p).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let mt = m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        entries.push((stem, m.len(), mt, p));
    }
    if entries.is_empty() {
        let r = "Architect dir exists but holds no .py files yet.".to_string();
        report_write("architect_files", "architect_files()", &r, true);
        return Ok(r);
    }
    entries.sort_by(|a, b| b.2.cmp(&a.2));
    let mut lines: Vec<String> = Vec::new();
    for (stem, size, _, path) in entries.iter().take(ARCHITECT_MAX_FILES) {
        let preview = match tokio::fs::read_to_string(path).await {
            Ok(s) => s
                .lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.chars().take(70).collect::<String>())
                .unwrap_or_else(|| "(comments only)".to_string()),
            Err(_) => "(unreadable)".to_string(),
        };
        lines.push(format!("  {} ({}B) -- {}", stem, size, preview));
    }
    let result = format!(
        "Architect holds {} project file(s):\n{}",
        entries.len(),
        lines.join("\n")
    );
    let summary = format!("{} architect files", entries.len());
    report_write("architect_files", "architect_files()", &summary, true);
    Ok(result)
}

/// Delete an architect script by name. Use sparingly -- architect files are meant to be long-lived. Cannot delete anything outside the architect dir.
///
/// * name - The handle of the file (without `.py`).
#[ollama_rs::function]
pub async fn architect_delete(
    name: String,
) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    let stem = match architect_validate_name(&name) {
        Ok(s) => s,
        Err(e) => {
            report_write(
                "architect_delete",
                &format!(
                    "architect_delete({})",
                    name.chars().take(40).collect::<String>()
                ),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    let path = match architect_resolve_path(&stem) {
        Ok(p) => p,
        Err(e) => {
            report_write(
                "architect_delete",
                &format!("architect_delete({})", stem),
                &e,
                false,
            );
            return Ok(e);
        }
    };
    if !path.exists() {
        let r = format!("Nothing to delete: no architect file named {}.py.", stem);
        report_write(
            "architect_delete",
            &format!("architect_delete({})", stem),
            &r,
            false,
        );
        return Ok(r);
    }
    match tokio::fs::remove_file(&path).await {
        Ok(_) => {
            let r = format!("Deleted architect/{}.py.", stem);
            report_write(
                "architect_delete",
                &format!("architect_delete({})", stem),
                &r,
                true,
            );
            Ok(r)
        }
        Err(e) => {
            let r = format!("Delete failed: {}", e);
            report_write(
                "architect_delete",
                &format!("architect_delete({})", stem),
                &r,
                false,
            );
            Ok(r)
        }
    }
}
