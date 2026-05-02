#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════
#  network_nomad.sh — AURORA local network discovery
#  Outputs JSON: { "total_count": N, "highlight": { "ip": "...", "hostname": "...", "vendor": "..." } }
#  Safe: no sudo required, degrades gracefully if nmap is absent.
# ═══════════════════════════════════════════════════════════════
set -euo pipefail

# ── Dependency check ──
if ! command -v nmap &>/dev/null; then
    echo '{"error":"nmap_not_found","total_count":0,"highlight":null}'
    exit 0
fi

# ── Discover local subnet from default route ──
SUBNET=""
if command -v ip &>/dev/null; then
    # Get the subnet of the default route interface
    IFACE=$(ip route show default 2>/dev/null | awk '{print $5; exit}')
    if [[ -n "${IFACE:-}" ]]; then
        # Get the CIDR address of that interface
        SUBNET=$(ip -4 addr show "$IFACE" 2>/dev/null \
            | grep -oP 'inet \K[\d./]+' | head -1)
    fi
fi

# Fallback: try common subnets
if [[ -z "${SUBNET:-}" ]]; then
    # Try to guess from hostname -I
    LOCAL_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
    if [[ -n "${LOCAL_IP:-}" ]]; then
        # Assume /24
        SUBNET="${LOCAL_IP%.*}.0/24"
    else
        echo '{"error":"no_subnet","total_count":0,"highlight":null}'
        exit 0
    fi
fi

# ── Run ping scan (no root needed for -sn) ──
# Timeout: 10 seconds max per host, scan up to 256 hosts
SCAN_OUTPUT=$(nmap -sn --max-retries 1 --host-timeout 3s "$SUBNET" 2>/dev/null) || true

if [[ -z "$SCAN_OUTPUT" ]]; then
    echo '{"error":"scan_failed","total_count":0,"highlight":null}'
    exit 0
fi

# ── Parse results ──
# Each host block starts with "Nmap scan report for ..."
# MAC lines (if available): "MAC Address: XX:XX:XX:XX:XX:XX (Vendor Name)"
# Without root, MAC/vendor is only available for hosts != self

declare -a IPS=()
declare -a HOSTNAMES=()
declare -a VENDORS=()

CURRENT_IP=""
CURRENT_HOST=""
CURRENT_VENDOR=""

while IFS= read -r line; do
    if [[ "$line" =~ ^Nmap\ scan\ report\ for\ (.+)$ ]]; then
        # Save previous entry if exists
        if [[ -n "$CURRENT_IP" ]]; then
            IPS+=("$CURRENT_IP")
            HOSTNAMES+=("$CURRENT_HOST")
            VENDORS+=("$CURRENT_VENDOR")
        fi
        CURRENT_VENDOR="unknown"
        # Parse: "Nmap scan report for hostname (ip)" or "Nmap scan report for ip"
        REPORT="${BASH_REMATCH[1]}"
        if [[ "$REPORT" =~ (.+)\ \(([0-9.]+)\) ]]; then
            CURRENT_HOST="${BASH_REMATCH[1]}"
            CURRENT_IP="${BASH_REMATCH[2]}"
        else
            CURRENT_IP="$REPORT"
            CURRENT_HOST=""
        fi
    elif [[ "$line" =~ ^MAC\ Address:\ [0-9A-Fa-f:]+\ \((.+)\)$ ]]; then
        CURRENT_VENDOR="${BASH_REMATCH[1]}"
    fi
done <<< "$SCAN_OUTPUT"

# Don't forget the last entry
if [[ -n "$CURRENT_IP" ]]; then
    IPS+=("$CURRENT_IP")
    HOSTNAMES+=("$CURRENT_HOST")
    VENDORS+=("$CURRENT_VENDOR")
fi

TOTAL=${#IPS[@]}

if [[ $TOTAL -eq 0 ]]; then
    echo '{"error":null,"total_count":0,"highlight":null}'
    exit 0
fi

# ── Pick a random neighbor ──
RANDOM_IDX=$((RANDOM % TOTAL))
H_IP="${IPS[$RANDOM_IDX]}"
H_HOST="${HOSTNAMES[$RANDOM_IDX]}"
H_VENDOR="${VENDORS[$RANDOM_IDX]}"

# Escape JSON strings (minimal: handle quotes and backslashes)
json_escape() {
    local s="$1"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/}"
    s="${s//$'\r'/}"
    printf '%s' "$s"
}

H_IP_E=$(json_escape "$H_IP")
H_HOST_E=$(json_escape "$H_HOST")
H_VENDOR_E=$(json_escape "$H_VENDOR")

cat <<EOF
{"error":null,"total_count":${TOTAL},"highlight":{"ip":"${H_IP_E}","hostname":"${H_HOST_E}","vendor":"${H_VENDOR_E}"}}
EOF
