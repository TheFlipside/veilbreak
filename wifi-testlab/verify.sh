#!/usr/bin/env bash
# wifi-testlab/verify.sh — smoke test to confirm the lab is ready
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNDIR="$SCRIPT_DIR/.run"
NS_AP="vb-ap"
NS_CLIENT="vb-client"

info()  { printf '\033[1;34m[info]\033[0m %s\n' "$1"; }
ok()    { printf '\033[1;32m[ ok ]\033[0m %s\n' "$1"; }
err()   { printf '\033[1;31m[err ]\033[0m %s\n' "$1" >&2; }
die()   { err "$1"; exit 1; }

# Parse the interfaces state file without executing it as shell code.
load_interfaces() {
    local file="$1"
    [[ -f "$file" ]] || return 1
    while IFS='=' read -r key val; do
        [[ "$val" =~ ^[a-zA-Z0-9_-]{1,15}$ ]] || continue
        case "$key" in
            IFACE_AP)      IFACE_AP="$val" ;;
            IFACE_CLIENT)  IFACE_CLIENT="$val" ;;
            IFACE_MONITOR) IFACE_MONITOR="$val" ;;
        esac
    done < "$file"
}

# Read and validate a PID from a file.
read_pid() {
    local pid=""
    if [[ -f "$1" ]]; then
        pid="$(<"$1")"
    fi
    if [[ "$pid" =~ ^[0-9]+$ ]]; then
        echo "$pid"
        return 0
    fi
    return 1
}

[[ $EUID -eq 0 ]] || die "must run as root"
load_interfaces "$RUNDIR/interfaces" || die "lab not running — start with: sudo ./setup.sh"

passed=0
failed=0
pass()      { ok "$1";  passed=$((passed + 1)); }
fail_check() { err "$1"; failed=$((failed + 1)); }

# ── Checks ─────────────────────────────────────────────────────────────

# 1. Kernel module
if [[ -d /sys/module/mac80211_hwsim ]]; then
    pass "mac80211_hwsim loaded"
else
    fail_check "mac80211_hwsim not loaded"
fi

# 2. hostapd (hidden AP)
pid=$(read_pid "$RUNDIR/hostapd.pid" || true)
if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    pass "hostapd (hidden) running"
else
    fail_check "hostapd (hidden) not running"
fi

# 2b. hostapd (visible APs)
vis_running=0
for vis_name in tplink ddw suddenlink; do
    pid=$(read_pid "$RUNDIR/hostapd-${vis_name}.pid" || true)
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        vis_running=$((vis_running + 1))
    fi
done
if [[ $vis_running -eq 3 ]]; then
    pass "$vis_running/3 visible APs running"
else
    fail_check "$vis_running/3 visible APs running"
fi

# 3. wpa_supplicant
pid=$(read_pid "$RUNDIR/wpa_supplicant.pid" || true)
if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    pass "wpa_supplicant running"
else
    fail_check "wpa_supplicant not running"
fi

# 4. Client association
if [[ -n "${IFACE_CLIENT:-}" ]] && \
        ip netns exec "$NS_CLIENT" iw dev "$IFACE_CLIENT" link 2>/dev/null \
        | grep -q "Connected"; then
    pass "client associated with AP"
else
    fail_check "client not associated"
fi

# 5. Monitor mode
if [[ -n "${IFACE_MONITOR:-}" ]] && \
        iw dev "$IFACE_MONITOR" info 2>/dev/null | grep -q "type monitor"; then
    pass "monitor interface ($IFACE_MONITOR) in monitor mode"
else
    fail_check "monitor interface not in monitor mode"
fi

# ── Summary ────────────────────────────────────────────────────────────

# Grab AP BSSID for reference.
ap_bssid=""
if [[ -n "${IFACE_AP:-}" ]]; then
    raw_bssid=$(ip netns exec "$NS_AP" iw dev "$IFACE_AP" info 2>/dev/null \
        | awk '/addr/{print $2}' \
        | tr '[:lower:]' '[:upper:]')
    if [[ "$raw_bssid" =~ ^([0-9A-F]{2}:){5}[0-9A-F]{2}$ ]]; then
        ap_bssid="$raw_bssid"
    fi
fi

echo
if [[ $failed -eq 0 ]]; then
    ok "all $passed checks passed"
    echo
    info "hidden SSID:   VeilbreakLab (ch 6)"
    info "visible SSIDs: TP-LINK_8907_5G (ch 11), DDW36563 (ch 1), SuddenLink990 (ch 6)"
    [[ -n "$ap_bssid" ]] && info "AP BSSID:      $ap_bssid"
    info "monitor:       ${IFACE_MONITOR:-unknown}"
    echo
    info "run veilbreak: sudo cargo run -p veilbreak-tui"
else
    err "$failed check(s) failed, $passed passed"
    info "try: sudo ./setup.sh --restart"
    exit 1
fi
