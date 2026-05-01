#!/usr/bin/env bash
# wifi-testlab/setup.sh — virtual WiFi lab using mac80211_hwsim
#
# Creates six virtual radios:
#   phy_ap      → hostapd (hidden AP)     in vb-ap namespace
#   phy_client  → wpa_supplicant (client) in vb-client namespace
#   phy_monitor → monitor mode            in default namespace (for veilbreak)
#   phy_vis1-3  → hostapd (visible APs)   in vb-ap namespace
#
# Usage:
#   sudo ./setup.sh             start the lab
#   sudo ./setup.sh --down      stop and clean up
#   sudo ./setup.sh --status    show lab state
#   sudo ./setup.sh --restart   tear down and restart
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNDIR="$SCRIPT_DIR/.run"
CONF_DIR="$SCRIPT_DIR/configs"
NS_AP="vb-ap"
NS_CLIENT="vb-client"
HWSIM_RADIOS=6

# ── Output helpers ─────────────────────────────────────────────────────

info()  { printf '\033[1;34m[info]\033[0m %s\n' "$1"; }
ok()    { printf '\033[1;32m[ ok ]\033[0m %s\n' "$1"; }
err()   { printf '\033[1;31m[err ]\033[0m %s\n' "$1" >&2; }
warn()  { printf '\033[1;33m[warn]\033[0m %s\n' "$1"; }
die()   { err "$1"; exit 1; }

# ── Helpers ────────────────────────────────────────────────────────────

check_root() {
    [[ $EUID -eq 0 ]] || die "must run as root (sudo $0)"
}

check_deps() {
    local missing=()
    for cmd in hostapd wpa_supplicant iw ip modprobe; do
        command -v "$cmd" &>/dev/null || missing+=("$cmd")
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        die "missing dependencies: ${missing[*]}"
    fi
}

# Validate a Linux network interface name.
validate_iface() {
    [[ "$1" =~ ^[a-zA-Z0-9_-]{1,15}$ ]] || die "invalid interface name: $1"
}

# Parse the interfaces state file without executing it as shell code.
load_interfaces() {
    local file="$1"
    [[ -f "$file" ]] || return 1
    while IFS='=' read -r key val; do
        [[ "$val" =~ ^[a-zA-Z0-9_-]{1,15}$ ]] || continue
        case "$key" in
            PHY_AP)        PHY_AP="$val" ;;
            PHY_CLIENT)    PHY_CLIENT="$val" ;;
            PHY_MONITOR)   PHY_MONITOR="$val" ;;
            IFACE_AP)      IFACE_AP="$val" ;;
            IFACE_CLIENT)  IFACE_CLIENT="$val" ;;
            IFACE_MONITOR) IFACE_MONITOR="$val" ;;
            PHY_VIS1)      PHY_VIS1="$val" ;;
            PHY_VIS2)      PHY_VIS2="$val" ;;
            PHY_VIS3)      PHY_VIS3="$val" ;;
            IFACE_VIS1)    IFACE_VIS1="$val" ;;
            IFACE_VIS2)    IFACE_VIS2="$val" ;;
            IFACE_VIS3)    IFACE_VIS3="$val" ;;
        esac
    done < "$file"
}

# List all phy devices backed by mac80211_hwsim.
hwsim_phys() {
    local phy_dir
    for phy_dir in /sys/class/ieee80211/phy*; do
        [[ -d "$phy_dir" ]] || continue
        if readlink "$phy_dir/device/driver" 2>/dev/null | grep -q mac80211_hwsim; then
            basename "$phy_dir"
        fi
    done
}

# Get the network interface name for a given phy.
iface_for_phy() {
    { find "/sys/class/ieee80211/$1/device/net/" -maxdepth 1 -mindepth 1 \
        -printf '%f\n' 2>/dev/null || true; } | head -1
}

# Kill a service by its PID file if it is still running.
kill_by_pidfile() {
    local pidfile="$1"
    local name="$2"
    if [[ -f "$pidfile" ]]; then
        local pid
        pid="$(<"$pidfile")"
        if [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            info "stopped $name (PID $pid)"
        fi
        rm -f "$pidfile"
    fi
}

# ── Lab lifecycle ──────────────────────────────────────────────────────

lab_up() {
    check_root
    check_deps

    if [[ -d /sys/module/mac80211_hwsim ]]; then
        warn "mac80211_hwsim already loaded — tearing down first"
        lab_down
    fi

    mkdir -p "$RUNDIR"
    chmod 700 "$RUNDIR"

    # Clean up on unexpected exit (e.g. SIGINT during setup).
    trap lab_down EXIT

    # ── Load virtual radios ────────────────────────────────────────────

    info "loading mac80211_hwsim (radios=$HWSIM_RADIOS)"
    modprobe mac80211_hwsim "radios=$HWSIM_RADIOS"
    sleep 1

    mapfile -t phys < <(hwsim_phys | sort -V)
    if [[ ${#phys[@]} -lt $HWSIM_RADIOS ]]; then
        err "expected $HWSIM_RADIOS hwsim phys, found ${#phys[@]}"
        modprobe -r mac80211_hwsim
        exit 1
    fi

    local phy_ap="${phys[0]}"
    local phy_client="${phys[1]}"
    local phy_monitor="${phys[2]}"
    local phy_vis1="${phys[3]}"
    local phy_vis2="${phys[4]}"
    local phy_vis3="${phys[5]}"

    local iface_ap iface_client iface_monitor iface_vis1 iface_vis2 iface_vis3
    iface_ap="$(iface_for_phy "$phy_ap")"
    iface_client="$(iface_for_phy "$phy_client")"
    iface_monitor="$(iface_for_phy "$phy_monitor")"
    iface_vis1="$(iface_for_phy "$phy_vis1")"
    iface_vis2="$(iface_for_phy "$phy_vis2")"
    iface_vis3="$(iface_for_phy "$phy_vis3")"

    validate_iface "${iface_ap:?no interface for $phy_ap}"
    validate_iface "${iface_client:?no interface for $phy_client}"
    validate_iface "${iface_monitor:?no interface for $phy_monitor}"
    validate_iface "${iface_vis1:?no interface for $phy_vis1}"
    validate_iface "${iface_vis2:?no interface for $phy_vis2}"
    validate_iface "${iface_vis3:?no interface for $phy_vis3}"

    ok "radios: AP=$phy_ap/$iface_ap  client=$phy_client/$iface_client  monitor=$phy_monitor/$iface_monitor"
    ok "visible APs: $phy_vis1/$iface_vis1  $phy_vis2/$iface_vis2  $phy_vis3/$iface_vis3"

    # Persist mapping so verify.sh and --down can find the interfaces.
    cat > "$RUNDIR/interfaces" <<EOF
PHY_AP=$phy_ap
IFACE_AP=$iface_ap
PHY_CLIENT=$phy_client
IFACE_CLIENT=$iface_client
PHY_MONITOR=$phy_monitor
IFACE_MONITOR=$iface_monitor
PHY_VIS1=$phy_vis1
IFACE_VIS1=$iface_vis1
PHY_VIS2=$phy_vis2
IFACE_VIS2=$iface_vis2
PHY_VIS3=$phy_vis3
IFACE_VIS3=$iface_vis3
EOF
    chmod 600 "$RUNDIR/interfaces"

    # ── Prevent NetworkManager interference ────────────────────────────

    if command -v nmcli &>/dev/null; then
        for iface in "$iface_ap" "$iface_client" "$iface_monitor" \
                     "$iface_vis1" "$iface_vis2" "$iface_vis3"; do
            nmcli device set "$iface" managed no 2>/dev/null || true
        done
    fi

    # ── Isolate AP and client in network namespaces ────────────────────

    ip netns add "$NS_AP" 2>/dev/null || true
    ip netns add "$NS_CLIENT" 2>/dev/null || true

    iw phy "$phy_ap" set netns name "$NS_AP"
    iw phy "$phy_client" set netns name "$NS_CLIENT"
    iw phy "$phy_vis1" set netns name "$NS_AP"
    iw phy "$phy_vis2" set netns name "$NS_AP"
    iw phy "$phy_vis3" set netns name "$NS_AP"
    ok "isolated: $phy_ap -> $NS_AP, $phy_client -> $NS_CLIENT"
    ok "isolated: $phy_vis1, $phy_vis2, $phy_vis3 -> $NS_AP"

    # ── Start hostapd (hidden AP) ──────────────────────────────────────

    ip netns exec "$NS_AP" ip link set "$iface_ap" up

    # hostapd requires the interface in its config file — substitute the
    # actual name discovered from hwsim.
    (umask 077 && sed "s|^interface=.*|interface=$iface_ap|" "$CONF_DIR/hostapd.conf" \
        > "$RUNDIR/hostapd.conf")

    info "starting hostapd (hidden SSID on channel 6)"
    ip netns exec "$NS_AP" hostapd -B \
        -P "$RUNDIR/hostapd.pid" \
        "$RUNDIR/hostapd.conf"
    sleep 1

    local hostapd_pid=""
    if [[ -f "$RUNDIR/hostapd.pid" ]]; then
        hostapd_pid="$(<"$RUNDIR/hostapd.pid")"
    fi
    if [[ "$hostapd_pid" =~ ^[0-9]+$ ]] && kill -0 "$hostapd_pid" 2>/dev/null; then
        ok "hostapd running (PID $hostapd_pid)"
    else
        die "hostapd failed to start — check dmesg and kernel support"
    fi

    # ── Start visible APs ────────────────────────────────────────────────

    local vis_configs=("hostapd-tplink:$iface_vis1" "hostapd-ddw:$iface_vis2" "hostapd-suddenlink:$iface_vis3")
    for entry in "${vis_configs[@]}"; do
        local conf_name="${entry%%:*}"
        local vis_iface="${entry##*:}"

        ip netns exec "$NS_AP" ip link set "$vis_iface" up

        (umask 077 && sed "s|^interface=.*|interface=$vis_iface|" "$CONF_DIR/${conf_name}.conf" \
            > "$RUNDIR/${conf_name}.conf")

        ip netns exec "$NS_AP" hostapd -B \
            -P "$RUNDIR/${conf_name}.pid" \
            "$RUNDIR/${conf_name}.conf"
    done
    sleep 1

    local vis_running=0
    for entry in "${vis_configs[@]}"; do
        local conf_name="${entry%%:*}"
        local vis_pid=""
        if [[ -f "$RUNDIR/${conf_name}.pid" ]]; then
            vis_pid="$(<"$RUNDIR/${conf_name}.pid")"
        fi
        if [[ "$vis_pid" =~ ^[0-9]+$ ]] && kill -0 "$vis_pid" 2>/dev/null; then
            vis_running=$((vis_running + 1))
        fi
    done
    ok "$vis_running/3 visible APs running"

    # ── Start wpa_supplicant (client) ──────────────────────────────────

    info "starting wpa_supplicant"
    ip netns exec "$NS_CLIENT" wpa_supplicant -B \
        -i "$iface_client" \
        -c "$CONF_DIR/wpa_supplicant.conf" \
        -P "$RUNDIR/wpa_supplicant.pid"

    # Wait for association (up to 10 seconds).
    local attempts=0
    while [[ $attempts -lt 10 ]]; do
        if ip netns exec "$NS_CLIENT" iw dev "$iface_client" link 2>/dev/null \
                | grep -q "Connected"; then
            break
        fi
        sleep 1
        attempts=$((attempts + 1))
    done

    if [[ $attempts -lt 10 ]]; then
        ok "client associated with hidden AP"
    else
        warn "client not associated after 10s — lab may still work"
    fi

    # ── Assign IPs and generate traffic ────────────────────────────────
    # Without data frames, airodump-ng won't list the client station.
    # Static IPs + a background ping keeps traffic flowing.

    ip netns exec "$NS_AP" ip addr add 10.0.0.1/24 dev "$iface_ap" 2>/dev/null || true
    ip netns exec "$NS_CLIENT" ip addr add 10.0.0.2/24 dev "$iface_client" 2>/dev/null || true

    ip netns exec "$NS_CLIENT" ping -q -i 1 10.0.0.1 &>/dev/null &
    local ping_pid=$!
    if kill -0 "$ping_pid" 2>/dev/null; then
        echo "$ping_pid" > "$RUNDIR/ping.pid"
        ok "client traffic generator running (PID $ping_pid)"
    else
        warn "ping failed to start — client traffic will be absent"
    fi

    # ── Enable monitor mode ────────────────────────────────────────────

    info "enabling monitor mode on $iface_monitor"
    ip link set "$iface_monitor" down
    iw dev "$iface_monitor" set type monitor
    ip link set "$iface_monitor" up
    iw dev "$iface_monitor" set channel 6
    ok "monitor ready: $iface_monitor (channel 6)"

    # ── Summary ────────────────────────────────────────────────────────

    echo
    ok "wifi-testlab is up"
    echo
    info "monitor interface:  $iface_monitor"
    info "hidden SSID:        VeilbreakLab (ch 6)"
    info "visible SSIDs:      TP-LINK_8907_5G (ch 11), DDW36563 (ch 1), SuddenLink990 (ch 6)"
    info "run veilbreak:      sudo cargo run -p veilbreak-tui"
    info "  -> select '$iface_monitor' when prompted"
    info "stop lab:           sudo $0 --down"

    # Setup succeeded — disarm the cleanup trap.
    trap - EXIT
}

lab_down() {
    check_root

    kill_by_pidfile "$RUNDIR/ping.pid" "ping"
    kill_by_pidfile "$RUNDIR/hostapd.pid" "hostapd (hidden)"
    kill_by_pidfile "$RUNDIR/hostapd-tplink.pid" "hostapd (TP-LINK_8907_5G)"
    kill_by_pidfile "$RUNDIR/hostapd-ddw.pid" "hostapd (DDW36563)"
    kill_by_pidfile "$RUNDIR/hostapd-suddenlink.pid" "hostapd (SuddenLink990)"
    kill_by_pidfile "$RUNDIR/wpa_supplicant.pid" "wpa_supplicant"

    # Deleting namespaces returns their phys to the default namespace.
    ip netns del "$NS_AP" 2>/dev/null || true
    ip netns del "$NS_CLIENT" 2>/dev/null || true

    # Restore monitor interface to managed mode.
    if load_interfaces "$RUNDIR/interfaces"; then
        if [[ -n "${IFACE_MONITOR:-}" ]] && ip link show "$IFACE_MONITOR" &>/dev/null; then
            ip link set "$IFACE_MONITOR" down 2>/dev/null || true
            iw dev "$IFACE_MONITOR" set type managed 2>/dev/null || true
            if command -v nmcli &>/dev/null; then
                nmcli device set "$IFACE_MONITOR" managed yes 2>/dev/null || true
            fi
        fi
    fi

    if [[ -d /sys/module/mac80211_hwsim ]]; then
        modprobe -r mac80211_hwsim
        ok "unloaded mac80211_hwsim"
    fi

    rm -rf "$RUNDIR"
    ok "wifi-testlab is down"
}

lab_status() {
    if [[ ! -d /sys/module/mac80211_hwsim ]]; then
        info "wifi-testlab is not running"
        return
    fi

    ok "mac80211_hwsim loaded"

    if ! load_interfaces "$RUNDIR/interfaces"; then
        warn "interface state file missing — lab may be partially up"
        return
    fi
    info "AP:       ${PHY_AP:-?} / ${IFACE_AP:-?}  (namespace $NS_AP)"
    info "client:   ${PHY_CLIENT:-?} / ${IFACE_CLIENT:-?}  (namespace $NS_CLIENT)"
    info "monitor:  ${PHY_MONITOR:-?} / ${IFACE_MONITOR:-?}  (default namespace)"
    info "visible:  ${PHY_VIS1:-?}/${IFACE_VIS1:-?}  ${PHY_VIS2:-?}/${IFACE_VIS2:-?}  ${PHY_VIS3:-?}/${IFACE_VIS3:-?}  (namespace $NS_AP)"

    local -A svc_labels=(
        ["hostapd"]="hostapd (hidden)"
        ["hostapd-tplink"]="hostapd (TP-LINK_8907_5G)"
        ["hostapd-ddw"]="hostapd (DDW36563)"
        ["hostapd-suddenlink"]="hostapd (SuddenLink990)"
        ["wpa_supplicant"]="wpa_supplicant"
    )
    for svc in hostapd hostapd-tplink hostapd-ddw hostapd-suddenlink wpa_supplicant; do
        local pidfile="$RUNDIR/$svc.pid"
        local pid=""
        if [[ -f "$pidfile" ]]; then
            pid="$(<"$pidfile")"
        fi
        if [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
            ok "${svc_labels[$svc]} running (PID $pid)"
        else
            warn "${svc_labels[$svc]} not running"
        fi
    done

    if ip netns exec "$NS_CLIENT" iw dev "${IFACE_CLIENT:-}" link 2>/dev/null \
            | grep -q "Connected"; then
        ok "client associated"
    else
        warn "client not associated"
    fi
}

# ── Main ───────────────────────────────────────────────────────────────

case "${1:---up}" in
    --up|up)
        lab_up
        ;;
    --down|down)
        lab_down
        ;;
    --status|status)
        lab_status
        ;;
    --restart|restart)
        lab_down
        lab_up
        ;;
    -h|--help)
        echo "Usage: sudo $0 [--up|--down|--status|--restart]"
        echo
        echo "  --up        Start the lab (default)"
        echo "  --down      Stop and clean up"
        echo "  --status    Show lab state"
        echo "  --restart   Tear down and restart"
        ;;
    *)
        err "unknown option: $1"
        echo "Usage: sudo $0 [--up|--down|--status|--restart]"
        exit 1
        ;;
esac
