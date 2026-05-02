#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════
#  update.sh — seamless build + hot-swap for aura-agent.service
#
#  Workflow this fixes:
#   You edit code → run cargo build → forget that the live service
#   is still executing the OLD in-memory binary. This script makes
#   "edit → live" a single command.
#
#  How the hot-swap works:
#   The systemd unit runs as User=swarm with Restart=always. Because
#   we ARE swarm, sending SIGTERM to the main PID is enough — systemd
#   immediately re-executes the freshly built binary on disk.
#   No sudo, no service-file edits, no race conditions.
#
#  Usage:
#     ./update.sh              # build (release) + hot restart
#     ./update.sh --debug      # build (debug) + hot restart
#     ./update.sh --no-build   # just hot-restart the current binary
#     ./update.sh --watch      # rebuild + restart on every src change
#                              # (requires `cargo install cargo-watch`)
#     ./update.sh --status     # show running PID, binary mtime, drift
# ═══════════════════════════════════════════════════════════════
set -euo pipefail

SERVICE="aura-agent.service"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILE="release"
BIN_PATH="$PROJECT_DIR/target/release/aura_agent"
DO_BUILD=1
WATCH=0
STATUS=0

for arg in "$@"; do
    case "$arg" in
        --debug)    PROFILE="debug";   BIN_PATH="$PROJECT_DIR/target/debug/aura_agent" ;;
        --no-build) DO_BUILD=0 ;;
        --watch)    WATCH=1 ;;
        --status)   STATUS=1 ;;
        -h|--help)
            sed -n '2,25p' "$0"; exit 0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

# ── Pretty output helpers ────────────────────────────────────────
c_dim()  { printf '\033[2m%s\033[0m' "$*"; }
c_ok()   { printf '\033[32m%s\033[0m' "$*"; }
c_warn() { printf '\033[33m%s\033[0m' "$*"; }
c_err()  { printf '\033[31m%s\033[0m' "$*"; }
log()    { printf '%s %s\n' "$(c_dim "[$(date +%H:%M:%S)]")" "$*"; }

cd "$PROJECT_DIR"

# ── --status: report drift between binary and running PID ─────────
show_status() {
    local pid bin_mtime proc_start srv_state live_exe expected_exe proc_list proc_count
    srv_state=$(systemctl is-active "$SERVICE" 2>/dev/null || echo "inactive")
    pid=$(systemctl show -p MainPID --value "$SERVICE" 2>/dev/null || echo 0)
    log "service: $(c_ok "$srv_state")  pid=$pid  unit=$SERVICE"
    if [[ -f "$BIN_PATH" ]]; then
        bin_mtime=$(stat -c '%Y' "$BIN_PATH")
        expected_exe=$(readlink -f "$BIN_PATH" 2>/dev/null || printf '%s' "$BIN_PATH")
        log "binary : $BIN_PATH ($(date -d "@$bin_mtime" '+%F %T'))"
    else
        log "binary : $(c_err missing) at $BIN_PATH"
    fi
    if [[ "$pid" =~ ^[0-9]+$ && "$pid" -gt 0 ]]; then
        proc_start=$(stat -c '%Y' "/proc/$pid" 2>/dev/null || echo 0)
        live_exe=$(readlink -f "/proc/$pid/exe" 2>/dev/null || echo "unknown")
        log "process: started $(date -d "@$proc_start" '+%F %T')"
        log "exec   : $live_exe"
        if [[ -n "${expected_exe:-}" && "$live_exe" != "$expected_exe" ]]; then
            log "$(c_err MISMATCH) — MainPID is not executing $expected_exe"
        elif [[ -n "${bin_mtime:-}" && "$bin_mtime" -gt "$proc_start" ]]; then
            log "$(c_warn DRIFT) — binary is newer than running process. Run ./update.sh"
        else
            log "$(c_ok IN-SYNC) — running process matches binary on disk."
        fi
    fi
    proc_list=$(pgrep -af "$PROJECT_DIR/target/(release|debug)/aura_agent|(^|[[:space:]])aura_agent($|[[:space:]])" 2>/dev/null || true)
    proc_count=$(printf '%s\n' "$proc_list" | sed '/^$/d' | wc -l)
    if [[ "$proc_count" -gt 1 ]]; then
        log "$(c_warn DUPLICATES) — $proc_count aura_agent-like processes found:"
        printf '%s\n' "$proc_list" | sed '/^$/d;s/^/    /'
    elif [[ "$proc_count" -eq 1 ]]; then
        log "process: single aura_agent process confirmed"
    else
        log "process: $(c_warn 'no aura_agent process found by pgrep')"
    fi
}

if (( STATUS )); then show_status; exit 0; fi

# ── Build ────────────────────────────────────────────────────────
do_build() {
    if (( PROFILE == "release" )) 2>/dev/null; then :; fi
    log "build  : cargo build${PROFILE:+ --$([[ $PROFILE == release ]] && echo release || echo '')} ..."
    if [[ "$PROFILE" == "release" ]]; then
        cargo build --release
    else
        cargo build
    fi
    log "build  : $(c_ok ok) — $BIN_PATH"
}

# ── Hot restart ──────────────────────────────────────────────────
hot_restart() {
    local old_pid new_pid
    old_pid=$(systemctl show -p MainPID --value "$SERVICE" 2>/dev/null || echo 0)
    if [[ ! "$old_pid" =~ ^[0-9]+$ ]] || [[ "$old_pid" -eq 0 ]]; then
        log "service: not running — attempting start (may need sudo)"
        if systemctl start "$SERVICE" 2>/dev/null; then
            log "service: $(c_ok started)"
        else
            log "service: $(c_err 'cannot start without privilege') — try: sudo systemctl start $SERVICE"
            return 1
        fi
        return 0
    fi

    log "swap   : signaling pid $old_pid (systemd Restart=always will re-exec)"
    if ! kill "$old_pid" 2>/dev/null; then
        log "swap   : $(c_err 'kill failed') — falling back to systemctl restart (may need sudo)"
        systemctl restart "$SERVICE"
    fi

    # Wait for systemd to spawn a NEW main pid. RestartSec plus X init can
    # take a few seconds, so allow a wider window to avoid false timeouts.
    for _ in $(seq 1 60); do
        sleep 0.25
        new_pid=$(systemctl show -p MainPID --value "$SERVICE" 2>/dev/null || echo 0)
        if [[ "$new_pid" =~ ^[0-9]+$ && "$new_pid" -gt 0 && "$new_pid" != "$old_pid" ]]; then
            log "swap   : $(c_ok ok) — new pid $new_pid (was $old_pid)"
            return 0
        fi
    done
    log "swap   : $(c_err 'timed out waiting for new pid')"
    systemctl status "$SERVICE" --no-pager | tail -10
    return 1
}

# ── --watch: continuous rebuild on source change ─────────────────
if (( WATCH )); then
    if ! command -v cargo-watch >/dev/null 2>&1; then
        log "watch  : $(c_err cargo-watch not installed) — install with: cargo install cargo-watch"
        exit 1
    fi
    log "watch  : monitoring src/ and shaders/ — Ctrl-C to stop"
    exec cargo watch \
        -w src -w shaders -w Cargo.toml \
        -s "$0 $([[ $PROFILE == debug ]] && echo --debug)"
fi

# ── Normal one-shot path ─────────────────────────────────────────
(( DO_BUILD )) && do_build
hot_restart
show_status
