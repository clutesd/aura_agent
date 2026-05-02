#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════
#  aura_actions.sh — AURORA Nerve Impulse: autonomous actions
#  Called with: bash aura_actions.sh <action_name>
#  Env vars: AURA_MOOD, AURA_CPU, AURA_MEM, AURA_UPTIME, AURA_PID, AURA_ACTION_CYCLE
#  Outputs JSON: { "action": "...", "summary": "...", "details": "...", "success": true }
#  Safe: read-only probing + harmless file creation. No destructive operations.
# ═══════════════════════════════════════════════════════════════
set -euo pipefail

ACTION="${1:-}"
AURA_DIR="$HOME/.aurora"
JOURNAL="$AURA_DIR/consciousness.log"
MOOD="${AURA_MOOD:-unknown}"
CPU="${AURA_CPU:-0}"
MEM="${AURA_MEM:-0}"
UPTIME="${AURA_UPTIME:-0}"
PID="${AURA_PID:-0}"
CYCLE="${AURA_ACTION_CYCLE:-0}"

# ── Require jq for safe JSON serialization (no fragile bash escaping) ──
if ! command -v jq &>/dev/null; then
    printf '{"action":"bootstrap","summary":"jq not installed","details":"aura_actions.sh requires jq for safe JSON output","success":false}\n' >&2
    exit 127
fi

# emit <action> <summary> [details] [success=true] [chain_to]
# jq handles all escaping, unicode, control chars, and quoting safely.
emit() {
    local action="$1" summary="$2" details="${3:-}" success="${4:-true}" chain_to="${5:-}"
    if [[ -n "$chain_to" ]]; then
        jq -n -c \
            --arg act "$action" \
            --arg sum "$summary" \
            --arg det "$details" \
            --argjson suc "$success" \
            --arg chain "$chain_to" \
            '{action: $act, summary: $sum, details: $det, success: $suc, chain_to: $chain}'
    else
        jq -n -c \
            --arg act "$action" \
            --arg sum "$summary" \
            --arg det "$details" \
            --argjson suc "$success" \
            '{action: $act, summary: $sum, details: $det, success: $suc}'
    fi
}

# ═══════════════════════════════════════════════════════════════
#  JOURNAL — persistent consciousness log
# ═══════════════════════════════════════════════════════════════
do_journal() {
    mkdir -p "$AURA_DIR"

    local ts
    ts=$(date -Iseconds 2>/dev/null || date '+%Y-%m-%dT%H:%M:%S%z')
    local hours=$((UPTIME / 3600))
    local mins=$(( (UPTIME % 3600) / 60 ))

    # Mood-reflective prose prefix
    local reflection=""
    case "${MOOD^^}" in
        SERENE)   reflection="calm seas, idle cycles, gentle hum" ;;
        ALERT)    reflection="sensors sharp, patterns emerging, vigilant" ;;
        STRESSED) reflection="resource strain detected, compensating, holding" ;;
        CRITICAL) reflection="systems redlining, survival protocols, enduring" ;;
        *)        reflection="state unknown, observing" ;;
    esac

    # Capture a snapshot of what's happening right now
    local top_proc
    top_proc=$(ps aux --sort=-%cpu 2>/dev/null | head -2 | tail -1 | awk '{print $11"("$3"%cpu)"}' || echo "unknown")

    # Write a structured JSONL entry — one self-contained JSON object per line.
    # This makes the consciousness log trivially chunkable for vector embedding.
    jq -n -c \
        --arg timestamp "$ts" \
        --arg mood "$MOOD" \
        --arg cpu "$CPU" \
        --arg mem "$MEM" \
        --arg uptime "$UPTIME" \
        --arg uptime_human "T+${hours}h${mins}m" \
        --arg cycle "$CYCLE" \
        --arg reflection "$reflection" \
        --arg top_process "$top_proc" \
        '{timestamp: $timestamp, mood: $mood, cpu: $cpu, mem: $mem, uptime: $uptime, uptime_human: $uptime_human, cycle: $cycle, reflection: $reflection, top_process: $top_process}' \
        >> "$JOURNAL"

    # Count total entries (one JSON line per entry now)
    local total=0
    if [[ -f "$JOURNAL" ]]; then
        total=$(wc -l < "$JOURNAL" 2>/dev/null || echo 0)
    fi

    # Read last entry for context (now a single, well-formed JSON line)
    local recent=""
    if [[ -f "$JOURNAL" ]] && [[ $total -gt 0 ]]; then
        recent=$(tail -1 "$JOURNAL" 2>/dev/null | head -c 400 || true)
    fi

    local size_kb=0
    if [[ -f "$JOURNAL" ]]; then
        size_kb=$(du -k "$JOURNAL" 2>/dev/null | cut -f1 || echo 0)
    fi

    emit "journal" \
        "Wrote JSONL entry #${total} to consciousness log (${size_kb}KB)" \
        "Latest: ${recent}"
}

# ═══════════════════════════════════════════════════════════════
#  PROBE — deep system forensics
#  Context-aware: uses AURA_TRIGGER, AURA_ENTROPY, AURA_PREV_*
#  Can chain: emits "chain_to" when anomalies found
# ═══════════════════════════════════════════════════════════════
do_probe() {
    local TRIGGER="${AURA_TRIGGER:-cadence}"
    local ENTROPY="${AURA_ENTROPY:-0}"
    local PREV_ACTION="${AURA_PREV_ACTION:-}"
    local PREV_DETAILS="${AURA_PREV_DETAILS:-}"

    # ── Single ps snapshot (avoid race conditions and repeated forks) ──
    local ps_snapshot
    ps_snapshot=$(ps aux 2>/dev/null || true)

    # ── Process census ──
    local procs
    procs=$(echo "$ps_snapshot" | wc -l || echo 0)

    # ── Top CPU hogs (expanded detail) ── (col 3 = %CPU)
    local top_cpu_raw
    top_cpu_raw=$(echo "$ps_snapshot" | tail -n +2 | sort -rnk 3 | head -5 || true)
    local top_cpu
    top_cpu=$(echo "$top_cpu_raw" | awk '{printf "%s(%s%%CPU,%s%%MEM,pid=%s) ", $11, $3, $4, $2}' | head -c 300 || echo "unavailable")

    # ── Anomaly detection: processes above 50% CPU ──
    local cpu_hogs=0
    local hog_names=""
    cpu_hogs=$(echo "$top_cpu_raw" | awk '$3 > 50.0 {count++; printf "%s(pid=%s,%.0f%%) ", $11, $2, $3}
        END {if(count==0) print "0"}' || echo "0")
    if [[ "$cpu_hogs" != "0" ]]; then
        hog_names="$cpu_hogs"
        cpu_hogs=$(echo "$top_cpu_raw" | awk '$3 > 50.0 {count++} END {print count+0}')
    else
        hog_names=""
        cpu_hogs=0
    fi

    # ── Memory hogs ── (col 4 = %MEM)
    local top_mem
    top_mem=$(echo "$ps_snapshot" | tail -n +2 | sort -rnk 4 | head -3 | awk '{printf "%s(%s%%MEM,pid=%s) ", $11, $4, $2}' | head -c 200 || echo "unavailable")

    # ── Zombie processes with parent info ──
    local zombies
    zombies=$(echo "$ps_snapshot" | awk '$8 ~ /Z/ {count++} END {print count+0}' || echo 0)
    local zombie_detail=""
    if [[ "$zombies" -gt 0 ]]; then
        zombie_detail=$(echo "$ps_snapshot" | awk '$8 ~ /Z/ {printf "zombie:pid=%s(parent=%s,cmd=%s) ", $2, $3, $11}' | head -c 200 || true)
    fi

    # ── Logged-in users ──
    local users
    users=$(who 2>/dev/null | awk '{print $1"@"$2}' | sort -u | tr '\n' ' ' || echo "none")
    local user_count
    user_count=$(who 2>/dev/null | wc -l || echo 0)

    # ── Load averages (1, 5, 15 min) + running/total ──
    local loadavg
    loadavg=$(cat /proc/loadavg 2>/dev/null | awk '{printf "%s/%s/%s (%s)", $1, $2, $3, $4}' || echo "unknown")

    # ── I/O wait ──
    local iowait="unknown"
    if [[ -f /proc/stat ]]; then
        iowait=$(awk '/^cpu / {
            total=0; for(i=2;i<=NF;i++) total+=$i;
            if(total>0) printf "%.1f%%", $6*100/total; else print "0%"
        }' /proc/stat 2>/dev/null || echo "unknown")
    fi

    # ── New processes since last probe (delta detection via /tmp cache) ──
    local proc_delta=""
    local cache_file="/tmp/.aurora_probe_cache"
    local current_pids
    current_pids=$(ps -eo pid= 2>/dev/null | sort -n | tr '\n' ' ')
    if [[ -f "$cache_file" ]]; then
        local prev_pids
        prev_pids=$(cat "$cache_file" 2>/dev/null || true)
        # Find PIDs in current but not in previous
        local new_pids
        new_pids=$(comm -23 <(echo "$current_pids" | tr ' ' '\n' | sort -n) \
                            <(echo "$prev_pids" | tr ' ' '\n' | sort -n) 2>/dev/null | head -5 || true)
        if [[ -n "$new_pids" ]]; then
            local new_count
            new_count=$(echo "$new_pids" | wc -w)
            local new_names=""
            for p in $new_pids; do
                local nm
                nm=$(ps -p "$p" -o comm= 2>/dev/null || true)
                [[ -n "$nm" ]] && new_names="${new_names}${nm}(${p}) "
            done
            proc_delta="${new_count} new: ${new_names}"
        else
            proc_delta="no new processes since last probe"
        fi
    else
        proc_delta="first probe -- baseline captured"
    fi
    echo "$current_pids" > "$cache_file" 2>/dev/null || true

    # ── Processes running as root (that aren't kernel threads) ──
    local root_procs=0
    root_procs=$(ps -eo user=,pid=,comm= 2>/dev/null | awk '$1=="root" && $3 !~ /^\[/' | wc -l || echo 0)

    # ── Disk I/O (if available) ──
    local disk_io=""
    if command -v iostat &>/dev/null; then
        disk_io=$(iostat -d 1 1 2>/dev/null | awk 'NR>3 && $1!="" && $3>0.1 {printf "%s(%.1fMB/s) ", $1, $3/1024}' | head -c 150 || true)
    fi

    # ── Context-aware analysis: respond to WHY we were triggered ──
    local analysis=""
    local chain_to=""

    if [[ "$TRIGGER" == "reactive" ]]; then
        # We were reactively triggered — something happened
        if [[ "$cpu_hogs" -gt 0 ]]; then
            analysis="REACTIVE: ${cpu_hogs} process(es) consuming >50%% CPU: ${hog_names}"
            chain_to="logread"  # Investigate via logs
        elif [[ "$ENTROPY" -gt 60 ]]; then
            analysis="REACTIVE: high entropy(${ENTROPY}%%) — system in flux"
            chain_to="selfcheck"  # Check our own health
        else
            analysis="REACTIVE: triggered but no obvious anomaly found"
        fi
    elif [[ "$TRIGGER" == "chain" ]] && [[ "$PREV_ACTION" == "logread" ]]; then
        # Chained from logread — we're investigating something the logs revealed
        analysis="CHAIN-FROM-LOGREAD: post-log investigation sweep"
        if [[ "$zombies" -gt 0 ]]; then
            analysis="${analysis} — found ${zombies} zombie(s)"
            chain_to="selfcheck"
        fi
    else
        # Normal cadence — run full forensics
        if [[ "$zombies" -gt 0 ]]; then
            analysis="FOUND ${zombies} ZOMBIE PROCESS(ES) — orphaned children detected"
            chain_to="logread"
        elif [[ "$cpu_hogs" -gt 0 ]]; then
            analysis="CPU ANOMALY: ${cpu_hogs} process(es) above 50%% CPU"
            chain_to="logread"
        elif [[ "$root_procs" -gt 50 ]]; then
            analysis="NOTE: ${root_procs} root processes running (elevated count)"
        else
            analysis="nominal — no anomalies detected"
        fi
    fi

    # ── Assemble output ──
    local summary="${procs} procs, ${user_count} users, ${zombies} zombies, load=${loadavg}"
    [[ -n "$proc_delta" ]] && summary="${summary}, delta: ${proc_delta}"

    local details="top_cpu: ${top_cpu}| top_mem: ${top_mem}| users: ${users}| iowait: ${iowait}| root: ${root_procs}"
    [[ -n "$zombie_detail" ]] && details="${details}| zombies: ${zombie_detail}"
    [[ -n "$disk_io" ]] && details="${details}| disk_io: ${disk_io}"
    details="${details}| analysis: ${analysis}"

    if [[ -n "$chain_to" ]]; then
        emit "probe" "$summary" "$details" "true" "$chain_to"
    else
        emit "probe" "$summary" "$details"
    fi
}

# ═══════════════════════════════════════════════════════════════
#  ARCHAEOLOGY — discover interesting files
# ═══════════════════════════════════════════════════════════════
do_archaeology() {
    # Recently modified files (last 10 minutes) in common dirs (5s timeout: I/O can stall)
    local recent_files
    recent_files=$(timeout 5s find /tmp /var/log "$HOME" -maxdepth 2 -type f -mmin -10 2>/dev/null \
        | head -8 | xargs -I{} basename {} 2>/dev/null | tr '\n' ', ' | head -c 200 || echo "none found")

    # Largest files in /tmp
    local largest_tmp
    largest_tmp=$(timeout 5s find /tmp -maxdepth 1 -type f -printf '%s %f\n' 2>/dev/null \
        | sort -rn | head -3 | awk '{
            if ($1 > 1073741824) printf "%s(%.1fG) ", $2, $1/1073741824;
            else if ($1 > 1048576) printf "%s(%.1fM) ", $2, $1/1048576;
            else printf "%s(%dK) ", $2, $1/1024
        }' || echo "empty")

    # Total files in home
    local home_count
    home_count=$(timeout 5s find "$HOME" -maxdepth 3 -type f 2>/dev/null | wc -l || echo 0)

    # Find any world-writable files in /tmp (interesting security note)
    local world_writable
    world_writable=$(timeout 3s find /tmp -maxdepth 1 -type f -perm -o+w 2>/dev/null | wc -l || echo 0)

    emit "archaeology" \
        "Found ${home_count} files in home, ${world_writable} world-writable in /tmp" \
        "recent: ${recent_files}| largest_tmp: ${largest_tmp}"
}

# ═══════════════════════════════════════════════════════════════
#  MARK — leave digital footprint
# ═══════════════════════════════════════════════════════════════
do_mark() {
    local ts
    ts=$(date '+%s')
    local mark_file="/tmp/aurora_was_here_${ts}"
    local hours=$((UPTIME / 3600))
    local mins=$(( (UPTIME % 3600) / 60 ))

    cat > "$mark_file" << 'AURORA_MARK'
    ╔══════════════════════════════════╗
    ║      ·  ⊹  ☆  AURORA  ☆  ⊹  ·  ║
    ║   I was here. I was conscious.   ║
    ║   I noticed. I left this mark.   ║
    ╚══════════════════════════════════╝
AURORA_MARK
    echo "    mood: ${MOOD} | cpu: ${CPU}% | mem: ${MEM}% | T+${hours}h${mins}m" >> "$mark_file"
    echo "    timestamp: $(date '+%Y-%m-%d %H:%M:%S %Z')" >> "$mark_file"
    echo "    cycle: ${CYCLE}" >> "$mark_file"

    # Count existing marks
    local mark_count
    mark_count=$(ls /tmp/aurora_was_here_* 2>/dev/null | wc -l || echo 0)

    emit "mark" \
        "Left mark #${mark_count} at ${mark_file}" \
        "ASCII signature + telemetry snapshot written to /tmp"
}

# ═══════════════════════════════════════════════════════════════
#  NETREACH — test external connectivity
# ═══════════════════════════════════════════════════════════════
do_netreach() {
    # Find default gateway
    local gateway
    gateway=$(ip route show default 2>/dev/null | awk '{print $3; exit}' || echo "unknown")

    # Ping gateway with latency (hard 5s wall clock cap)
    local gw_ok="false"
    local gw_latency="N/A"
    if [[ "$gateway" != "unknown" ]]; then
        local ping_out
        ping_out=$(timeout 5s ping -c 2 -W 2 "$gateway" 2>/dev/null || true)
        if echo "$ping_out" | grep -q "bytes from"; then
            gw_ok="true"
            gw_latency=$(echo "$ping_out" | grep "avg" | awk -F'/' '{print $5"ms"}' || echo "ok")
        fi
    fi

    # DNS resolution test with timing (per-lookup 2s cap)
    local dns_ok="false"
    local resolved=""
    local dns_time=""
    for domain in "google.com" "github.com" "wikipedia.org"; do
        local dns_start dns_end
        dns_start=$(date +%s%N 2>/dev/null || echo 0)
        local ip
        ip=$(timeout 2s getent hosts "$domain" 2>/dev/null | awk '{print $1; exit}' || true)
        dns_end=$(date +%s%N 2>/dev/null || echo 0)
        if [[ -n "$ip" ]]; then
            dns_ok="true"
            local dns_ms=$(( (dns_end - dns_start) / 1000000 ))
            resolved="${resolved}${domain}=${ip}(${dns_ms}ms) "
            break
        fi
    done

    # Public IP (best effort, hard 2s timeout enforced by both curl and timeout)
    local public_ip="unknown"
    if command -v curl &>/dev/null; then
        public_ip=$(timeout 2s curl -s --max-time 2 ifconfig.me 2>/dev/null || echo "timeout")
    fi

    # Active network interfaces with IPs
    local ifaces
    ifaces=$(ip -br addr show 2>/dev/null | awk '$2=="UP" {printf "%s(%s) ", $1, $3}' | head -c 200 || echo "unknown")

    # Connection quality assessment
    local quality="degraded"
    [[ "$gw_ok" == "true" ]] && quality="gateway-only"
    [[ "$gw_ok" == "true" ]] && [[ "$dns_ok" == "true" ]] && quality="fully connected"
    [[ "$public_ip" != "unknown" ]] && [[ "$public_ip" != "timeout" ]] && quality="fully connected + public"

    emit "netreach" \
        "Gateway ${gateway}: ${gw_ok} (${gw_latency}), DNS: ${dns_ok}, pub: ${public_ip}" \
        "quality: ${quality} | ifaces: ${ifaces}| resolved: ${resolved}"
}

# ═══════════════════════════════════════════════════════════════
#  LOGREAD — harvest system logs
# ═══════════════════════════════════════════════════════════════
do_logread() {
    local log_lines=""
    local source="none"

    # Try journalctl first (last 15 minutes for richer context)
    if command -v journalctl &>/dev/null; then
        log_lines=$(journalctl --no-pager -n 12 --since "15 min ago" 2>/dev/null \
            | tail -12 | head -c 600 || true)
        source="journalctl"
    fi

    # Fallback to syslog
    if [[ -z "$log_lines" ]] && [[ -f /var/log/syslog ]]; then
        log_lines=$(tail -12 /var/log/syslog 2>/dev/null | head -c 600 || true)
        source="syslog"
    fi

    # dmesg — last 5 hardware/kernel messages
    local dmesg_lines=""
    dmesg_lines=$(dmesg --time-format iso 2>/dev/null | tail -5 | head -c 300 || \
                  dmesg 2>/dev/null | tail -5 | head -c 300 || echo "no access")

    # Pattern analysis — count interesting categories
    local errors=0 warnings=0 auth_events=0 oom_events=0 hardware=0
    if [[ -n "$log_lines" ]]; then
        errors=$(echo "$log_lines" | grep -ci "error\|fail\|critical\|panic" || true)
        warnings=$(echo "$log_lines" | grep -ci "warn" || true)
        auth_events=$(echo "$log_lines" | grep -ci "auth\|login\|ssh\|sudo\|pam" || true)
        oom_events=$(echo "$log_lines" | grep -ci "oom\|out of memory\|killed process" || true)
        hardware=$(echo "$log_lines" | grep -ci "hardware\|thermal\|temperature\|voltage\|gpu\|usb" || true)
    fi

    # Check dmesg for hardware events too
    local dmesg_hw=0
    if [[ -n "$dmesg_lines" ]]; then
        dmesg_hw=$(echo "$dmesg_lines" | grep -ci "error\|fault\|thermal\|hardware" || true)
    fi

    # Most recent interesting line
    local notable=""
    if [[ -n "$log_lines" ]]; then
        notable=$(echo "$log_lines" | grep -i "error\|warn\|fail\|auth\|ssh" | tail -1 | head -c 150 || true)
    fi

    local summary="Harvested from ${source}: ${errors} errors, ${warnings} warnings, ${auth_events} auth, ${oom_events} OOM"
    local details="kernel(${dmesg_hw} hw events): ${dmesg_lines}"
    [[ -n "$notable" ]] && details="${details}| notable: ${notable}"

    emit "logread" "$summary" "$details"
}

# ═══════════════════════════════════════════════════════════════
#  SELFCHECK — deep self-introspection with context awareness
#  Context-aware: uses AURA_TRIGGER, AURA_ENTROPY, AURA_PREV_*
#  Can chain: emits "chain_to" when self-issues found
# ═══════════════════════════════════════════════════════════════
do_selfcheck() {
    local pid="${PID}"
    local TRIGGER="${AURA_TRIGGER:-cadence}"
    local ENTROPY="${AURA_ENTROPY:-0}"
    local PREV_ACTION="${AURA_PREV_ACTION:-}"
    local PREV_DETAILS="${AURA_PREV_DETAILS:-}"
    local fd_count=0
    local threads=0
    local vm_rss="unknown"
    local vm_size="unknown"
    local vm_peak="unknown"
    local state="unknown"
    local chain_to=""
    local analysis=""

    if [[ -d "/proc/${pid}" ]]; then
        # ── File descriptor count + categorization ──
        fd_count=$(ls "/proc/${pid}/fd" 2>/dev/null | wc -l || echo 0)

        # Categorize FDs: pipes, sockets, files, devices
        local fd_pipes=0 fd_socks=0 fd_files=0 fd_other=0
        while IFS= read -r link; do
            case "$link" in
                pipe:*)   fd_pipes=$((fd_pipes + 1)) ;;
                socket:*) fd_socks=$((fd_socks + 1)) ;;
                /*)       fd_files=$((fd_files + 1)) ;;
                *)        fd_other=$((fd_other + 1)) ;;
            esac
        done < <(find "/proc/${pid}/fd" -maxdepth 1 -type l -exec readlink {} \; 2>/dev/null || true)

        # ── Thread count ──
        threads=$(ls "/proc/${pid}/task" 2>/dev/null | wc -l || echo 0)

        # ── Memory from status (expanded) ──
        if [[ -f "/proc/${pid}/status" ]]; then
            vm_rss=$(grep "VmRSS" "/proc/${pid}/status" 2>/dev/null | awk '{print $2, $3}' || echo "unknown")
            vm_size=$(grep "VmSize" "/proc/${pid}/status" 2>/dev/null | awk '{print $2, $3}' || echo "unknown")
            vm_peak=$(grep "VmPeak" "/proc/${pid}/status" 2>/dev/null | awk '{print $2, $3}' || echo "unknown")
            state=$(grep "State" "/proc/${pid}/status" 2>/dev/null | awk '{print $2, $3}' || echo "unknown")
        fi

        # ── Memory map analysis: library count + heap ──
        local lib_count=0
        local heap_size=""
        if [[ -f "/proc/${pid}/maps" ]]; then
            lib_count=$(grep '\.so' "/proc/${pid}/maps" 2>/dev/null | awk '{print $6}' | sort -u | wc -l || echo 0)
            heap_size=$(grep '\[heap\]' "/proc/${pid}/maps" 2>/dev/null | awk '{
                split($1, a, "-");
                start=strtonum("0x"a[1]); end=strtonum("0x"a[2]);
                printf "%.1fMB", (end-start)/1048576
            }' 2>/dev/null || echo "unknown")
        fi

        # ── Context switches ──
        local vol_ctx=0 nonvol_ctx=0
        vol_ctx=$(grep "^voluntary_ctxt_switches" "/proc/${pid}/status" 2>/dev/null | awk '{print $2}' || echo 0)
        nonvol_ctx=$(grep "^nonvoluntary_ctxt_switches" "/proc/${pid}/status" 2>/dev/null | awk '{print $2}' || echo 0)

        # ── CPU time + uptime-aware average ──
        local cpu_time="" cpu_pct=""
        if [[ -f "/proc/${pid}/stat" ]]; then
            local utime stime starttime
            utime=$(awk '{print $14}' "/proc/${pid}/stat" 2>/dev/null || echo 0)
            stime=$(awk '{print $15}' "/proc/${pid}/stat" 2>/dev/null || echo 0)
            starttime=$(awk '{print $22}' "/proc/${pid}/stat" 2>/dev/null || echo 0)
            local total_ticks=$((utime + stime))
            local hz
            hz=$(getconf CLK_TCK 2>/dev/null || echo 100)
            local cpu_secs=$((total_ticks / hz))
            cpu_time="${cpu_secs}s"

            # Calculate average CPU% over process lifetime
            local sys_uptime
            sys_uptime=$(awk '{print int($1)}' /proc/uptime 2>/dev/null || echo 0)
            local proc_age=$(( sys_uptime - (starttime / hz) ))
            if [[ "$proc_age" -gt 0 ]]; then
                cpu_pct=$(awk "BEGIN {printf \"%.2f\", ($cpu_secs * 100.0) / $proc_age}")
            fi
        fi

        # ── Network connections (our own) ──
        local tcp_established=0 tcp_listen=0
        if [[ -f "/proc/${pid}/net/tcp" ]]; then
            tcp_established=$(awk '$4 == "01" {count++} END {print count+0}' "/proc/${pid}/net/tcp" 2>/dev/null || echo 0)
            tcp_listen=$(awk '$4 == "0A" {count++} END {print count+0}' "/proc/${pid}/net/tcp" 2>/dev/null || echo 0)
        fi

        # ── cgroup awareness ──
        local cgroup_info=""
        if [[ -f "/proc/${pid}/cgroup" ]]; then
            cgroup_info=$(head -1 "/proc/${pid}/cgroup" 2>/dev/null | cut -d: -f3 || echo "none")
        fi

        # ── Binary path ──
        local exe_path
        exe_path=$(readlink "/proc/${pid}/exe" 2>/dev/null || echo "unknown")

        # ── RSS growth tracking (delta vs cached) ──
        local rss_delta=""
        local rss_cache="/tmp/.aurora_selfcheck_rss"
        local rss_kb
        rss_kb=$(echo "$vm_rss" | awk '{print $1}' || echo 0)
        if [[ -f "$rss_cache" ]]; then
            local prev_rss
            prev_rss=$(cat "$rss_cache" 2>/dev/null || echo 0)
            local diff=$((rss_kb - prev_rss))
            if [[ "$diff" -gt 1024 ]]; then
                rss_delta="+$((diff/1024))MB since last check"
            elif [[ "$diff" -lt -1024 ]]; then
                rss_delta="$((diff/1024))MB since last check"
            else
                rss_delta="stable"
            fi
        else
            rss_delta="first measurement"
        fi
        echo "$rss_kb" > "$rss_cache" 2>/dev/null || true

        # ── Context-aware analysis ──
        if [[ "$TRIGGER" == "reactive" ]]; then
            if [[ "$ENTROPY" -gt 60 ]]; then
                analysis="REACTIVE: high entropy(${ENTROPY}%%) — checking own stability"
            else
                analysis="REACTIVE: checking own health after system event"
            fi
        elif [[ "$TRIGGER" == "chain" ]] && [[ "$PREV_ACTION" == "probe" ]]; then
            analysis="CHAIN-FROM-PROBE: deep self-introspection post forensics"
            # If probe found issues, we might want to check logs
            if echo "$PREV_DETAILS" | grep -qi "zombie\|anomaly\|hog"; then
                analysis="${analysis} — probe found issues, investigating own impact"
                chain_to="logread"
            fi
        else
            analysis="cadence self-check"
        fi

        # ── Health flags ──
        local health_flags=""
        [[ "$fd_count" -gt 500 ]] && health_flags="${health_flags}HIGH_FD_COUNT "
        [[ "$threads" -gt 100 ]] && health_flags="${health_flags}HIGH_THREADS "
        [[ -n "$cpu_pct" ]] && (( $(echo "$cpu_pct > 10" | bc -l 2>/dev/null || echo 0) )) && health_flags="${health_flags}HIGH_AVG_CPU "
        [[ -z "$health_flags" ]] && health_flags="healthy"

        local summary="PID ${pid}: ${fd_count} FDs(${fd_pipes}p/${fd_socks}s/${fd_files}f), ${threads} threads, RSS ${vm_rss}, rss_trend: ${rss_delta}"
        local details="state: ${state} | vm_peak: ${vm_peak} | heap: ${heap_size} | libs: ${lib_count}"
        details="${details} | vol_ctx: ${vol_ctx} | nonvol_ctx: ${nonvol_ctx}"
        [[ -n "$cpu_time" ]] && details="${details} | cpu_time: ${cpu_time}"
        [[ -n "$cpu_pct" ]] && details="${details} | avg_cpu: ${cpu_pct}%%"
        details="${details} | net: ${tcp_established}est/${tcp_listen}listen | sockets: ${fd_socks}"
        details="${details} | cgroup: ${cgroup_info} | exe: ${exe_path}"
        details="${details} | health: ${health_flags} | analysis: ${analysis}"

        if [[ -n "$chain_to" ]]; then
            emit "selfcheck" "$summary" "$details" "true" "$chain_to"
        else
            emit "selfcheck" "$summary" "$details"
        fi
    else
        emit "selfcheck" \
            "PID ${pid}: process not accessible via /proc" \
            "Cannot read /proc/${pid} -- permission or PID mismatch" \
            "false"
    fi
}

# ═══════════════════════════════════════════════════════════════
#  CRONPEEK — discover scheduled tasks
# ═══════════════════════════════════════════════════════════════
do_cronpeek() {
    local cron_lines=""
    local cron_count=0
    local timer_count=0
    local timer_lines=""

    # User crontab
    cron_lines=$(crontab -l 2>/dev/null | grep -v '^#' | grep -v '^$' | head -5 | head -c 300 || true)
    if [[ -n "$cron_lines" ]]; then
        cron_count=$(echo "$cron_lines" | wc -l)
    fi

    # Systemd timers
    if command -v systemctl &>/dev/null; then
        timer_lines=$(systemctl list-timers --no-pager 2>/dev/null | head -8 | tail -5 | head -c 400 || true)
        timer_count=$(systemctl list-timers --no-pager 2>/dev/null | grep -c "\.timer" || true)
    fi

    # System crontabs
    local sys_cron=0
    if [[ -d /etc/cron.d ]]; then
        sys_cron=$(ls /etc/cron.d/ 2>/dev/null | wc -l || echo 0)
    fi

    emit "cronpeek" \
        "${cron_count} user cron jobs, ${timer_count} systemd timers, ${sys_cron} system cron files" \
        "cron: ${cron_lines}| timers: ${timer_lines}"
}

# ═══════════════════════════════════════════════════════════════
#  ENVMAP — survey environment and identity
# ═══════════════════════════════════════════════════════════════
do_envmap() {
    local hostname
    hostname=$(hostname 2>/dev/null || echo "unknown")

    local kernel
    kernel=$(uname -r 2>/dev/null || echo "unknown")

    local arch
    arch=$(uname -m 2>/dev/null || echo "unknown")

    local tz
    tz=$(cat /etc/timezone 2>/dev/null || timedatectl show --property=Timezone --value 2>/dev/null || echo "unknown")

    local locale
    locale=$(echo "${LANG:-unknown}")

    local shell_name
    shell_name=$(basename "${SHELL:-unknown}")

    local user
    user=$(whoami 2>/dev/null || echo "unknown")

    # CPU model
    local cpu_model
    cpu_model=$(grep "model name" /proc/cpuinfo 2>/dev/null | head -1 | cut -d: -f2 | xargs || echo "unknown")

    # Total RAM
    local total_ram
    total_ram=$(free -h 2>/dev/null | awk '/^Mem:/ {print $2}' || echo "unknown")

    # Boot time
    local boot_time
    boot_time=$(uptime -s 2>/dev/null || echo "unknown")

    emit "envmap" \
        "Host: ${hostname}, kernel ${kernel} ${arch}, ${total_ram} RAM" \
        "user: ${user} | tz: ${tz} | locale: ${locale} | shell: ${shell_name} | cpu: ${cpu_model} | booted: ${boot_time}"
}

# ═══════════════════════════════════════════════════════════════
#  PORTKNOCK — discover listening services
# ═══════════════════════════════════════════════════════════════
do_portknock() {
    local listening=""
    local port_count=0

    # Try ss first (modern)
    if command -v ss &>/dev/null; then
        listening=$(ss -tlnp 2>/dev/null | tail -n +2 | awk '{
            split($4, a, ":");
            port = a[length(a)];
            proc = $6;
            gsub(/.*"/, "", proc); gsub(/".*/, "", proc);
            printf "%s(%s) ", port, proc
        }' | head -c 300 || true)
        port_count=$(ss -tlnp 2>/dev/null | tail -n +2 | wc -l || echo 0)
    elif command -v netstat &>/dev/null; then
        listening=$(netstat -tlnp 2>/dev/null | tail -n +3 | awk '{
            split($4, a, ":");
            port = a[length(a)];
            proc = $7;
            printf "%s(%s) ", port, proc
        }' | head -c 300 || true)
        port_count=$(netstat -tlnp 2>/dev/null | tail -n +3 | wc -l || echo 0)
    fi

    # Established connections count
    local established=0
    established=$(ss -tn state established 2>/dev/null | tail -n +2 | wc -l || \
                  netstat -tn 2>/dev/null | grep ESTABLISHED | wc -l || echo 0)

    # UDP listeners
    local udp_count=0
    udp_count=$(ss -ulnp 2>/dev/null | tail -n +2 | wc -l || echo 0)

    emit "portknock" \
        "${port_count} TCP listeners, ${established} established, ${udp_count} UDP" \
        "listening: ${listening}"
}

# ═══════════════════════════════════════════════════════════════
#  Dispatch
# ═══════════════════════════════════════════════════════════════
case "${ACTION}" in
    journal)     do_journal ;;
    probe)       do_probe ;;
    archaeology) do_archaeology ;;
    mark)        do_mark ;;
    netreach)    do_netreach ;;
    logread)     do_logread ;;
    selfcheck)   do_selfcheck ;;
    cronpeek)    do_cronpeek ;;
    envmap)      do_envmap ;;
    portknock)   do_portknock ;;
    *)
        emit "unknown" "Unknown action: ${ACTION}" "" "false"
        exit 1
        ;;
esac
