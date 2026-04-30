# wifi-testlab Guide

This document explains how to use the wifi-testlab to develop and test
veilbreak without physical WiFi hardware, and how the lab works under the
hood.

---

## Quick start

```bash
cd wifi-testlab/

# 1. Start the lab
sudo ./setup.sh

# 2. Verify all components are healthy
sudo ./verify.sh

# 3. Run veilbreak (from the repo root)
cd ..
sudo cargo run -p veilbreak-tui
```

When veilbreak prompts for an interface, select the monitor interface
printed by `setup.sh` (typically `wlan2`, but the exact name depends on
what other wireless hardware is present). When prompted for band, select
**2.4 GHz** — the virtual AP operates on channel 6 in the 2.4 GHz band.

Once the dashboard loads you will see:

- A hidden AP with a zeroed ESSID and one associated client.
- tshark detects the probe response exchange and reveals the real SSID
  (`VeilbreakLab`).
- The client station appears in the client list, generating ~1 data frame
  per second.
- Deauth injection via `d` works — `aireplay-ng` sends the deauth frame
  over the virtual medium, the client disassociates, and the subsequent
  reassociation produces another probe response.

This is the real veilbreak binary running its full pipeline — airodump-ng,
tshark, and aireplay-ng all operate against the virtual interfaces exactly
as they would against real hardware. The only difference is that the
underlying medium is simulated by the kernel.

## Lifecycle

| Command                       | Effect                                      |
|-------------------------------|---------------------------------------------|
| `sudo ./setup.sh`            | Start the lab (default action)              |
| `sudo ./setup.sh --down`     | Stop all services, unload kernel module     |
| `sudo ./setup.sh --status`   | Show running state of all components        |
| `sudo ./setup.sh --restart`  | Tear down and rebuild from scratch          |
| `sudo ./verify.sh`           | Run 5 health checks against the running lab |

The lab is fully ephemeral. `--down` kills every process, deletes the
network namespaces, restores the monitor interface to managed mode,
re-enables NetworkManager on it, and unloads the kernel module. Nothing
persists between runs.

## Configuration

The AP and client configs live in `configs/` and can be edited between
runs:

**`configs/hostapd.conf`** — the hidden access point:

| Setting                  | Default                | Notes                              |
|--------------------------|------------------------|------------------------------------|
| `ssid`                   | `VeilbreakLab`         | The SSID to hide and reveal        |
| `channel`                | `6`                    | Must be 1–14 for 2.4 GHz          |
| `wpa_passphrase`         | `veilbreak-test-only`  | Test-only, not a real credential   |
| `ignore_broadcast_ssid`  | `1`                    | `1` = zeroed ESSID, `2` = empty    |

**`configs/wpa_supplicant.conf`** — the associated client. Update `ssid`
and `psk` here if you change them in hostapd.conf.

After editing, restart the lab with `sudo ./setup.sh --restart`.

---

## Architecture

### The virtual medium

The lab is built on `mac80211_hwsim`, a Linux kernel module that creates
virtual 802.11 radios. Each radio has a full mac80211 stack — the same
driver interface that real WiFi chipsets use. All radios share a simulated
wireless medium: every frame transmitted by one radio is received by all
others, as if they were in the same room.

```
 kernel space
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│   mac80211_hwsim                                             │
│  ┌────────┐    ┌────────┐    ┌────────┐                      │
│  │  phy0  │    │  phy1  │    │  phy2  │    virtual radios    │
│  └───┬────┘    └───┬────┘    └───┬────┘                      │
│      │             │             │                            │
│      └─────────────┴─────────────┘                            │
│              shared medium                                    │
│         (all frames visible to all)                           │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

This is the same mechanism used upstream by hostapd, iwd, and
wpa_supplicant for their own CI suites.

### Network namespaces

Each radio is moved into a Linux network namespace to isolate it from the
host and from the other roles:

```
┌─────────────────────────────┐
│   namespace: vb-ap          │
│                             │
│   phy0 / wlan0              │
│   hostapd                   │
│   10.0.0.1/24               │
│                             │
│   Broadcasts beacons with   │
│   zeroed ESSID (hidden).    │
│   Accepts WPA2-PSK clients. │
└──────────────┬──────────────┘
               │
               │  802.11 frames traverse the
               │  virtual medium, not IP routing
               │
┌──────────────┴──────────────┐
│   namespace: vb-client      │
│                             │
│   phy1 / wlan1              │
│   wpa_supplicant            │
│   10.0.0.2/24               │
│                             │
│   Connects with scan_ssid=1 │
│   (active probing). Sends   │
│   a ping/s to generate      │
│   continuous data frames.   │
└──────────────┬──────────────┘
               │
               │  all three radios share
               │  the same virtual medium
               │
┌──────────────┴──────────────┐
│   default namespace (host)  │
│                             │
│   phy2 / wlan2              │
│   monitor mode, channel 6   │
│                             │
│   This is the interface     │
│   veilbreak operates on.    │
│   airodump-ng captures      │
│   here, tshark reads the    │
│   resulting pcap, and       │
│   aireplay-ng injects       │
│   deauth frames through it. │
└─────────────────────────────┘
```

Namespaces serve two purposes. First, they prevent NetworkManager from
trying to manage the AP and client interfaces. Second, they ensure the
only path between the radios is the 802.11 virtual medium — there is no
IP-level shortcut between namespaces.

### What veilbreak sees

From veilbreak's perspective, the lab is indistinguishable from a real
hidden network. The frame types that matter all behave identically:

| Frame type        | Source                   | What veilbreak does with it       |
|-------------------|--------------------------|-----------------------------------|
| Beacon            | hostapd                  | airodump-ng parses it; ESSID is zeroed, so the AP appears as hidden |
| Probe request     | wpa_supplicant           | Visible in the pcap; confirms client is actively seeking the AP     |
| Probe response    | hostapd                  | tshark extracts the real SSID from this frame                       |
| Data frames       | ping (client → AP)       | airodump-ng uses these to list the client station                   |
| Deauth            | aireplay-ng (veilbreak)  | Injected via the monitor interface; causes client disassociation    |
| Reassociation     | wpa_supplicant           | After deauth, the client reconnects, generating a new probe response|

### Startup sequence

`setup.sh` performs these steps in order:

1. **Load `mac80211_hwsim`** with 3 radios. Discovers the new phy devices
   by checking which phys in `/sys/class/ieee80211/` are backed by the
   hwsim driver (safe when real WiFi hardware is present).

2. **Tell NetworkManager to ignore** the virtual interfaces so it does not
   attempt to manage them.

3. **Create network namespaces** `vb-ap` and `vb-client`, and move phy0
   and phy1 into them via `iw phy <name> set netns name <ns>`.

4. **Start hostapd** inside `vb-ap` with a runtime-generated config
   (the interface name is substituted from the discovered phy mapping).

5. **Start wpa_supplicant** inside `vb-client` and poll for association
   (up to 10 seconds).

6. **Assign static IPs** (10.0.0.1 on AP, 10.0.0.2 on client) and start
   a background ping to generate continuous data frames.

7. **Set phy2 to monitor mode** on channel 6 in the default namespace.

### Teardown

`setup.sh --down` reverses every step:

1. Kill the ping, hostapd, and wpa_supplicant processes by PID file.
2. Delete both network namespaces (this automatically returns the phys
   to the default namespace).
3. Restore the monitor interface to managed mode and re-enable
   NetworkManager on it.
4. Unload `mac80211_hwsim` (removes all virtual radios).
5. Remove the `.run/` state directory.

### Limitations

- **No RF simulation.** Signal strength is not modeled — all frames are
  delivered at full strength with zero loss. Channel-hop timing cannot be
  tested. For RF-level simulation, `wmediumd` can be added as a userspace
  medium controller, but it is not included in this lab.

- **Kernel module required.** The host must be bare-metal Linux (or a VM
  with the module available). WSL and unprivileged containers do not
  support loading kernel modules.

- **Instant delivery.** The virtual medium has no propagation delay. On
  real hardware, deauth → disassociation → reassociation involves small
  timing windows; here it happens near-instantly.

- **Single AP.** The lab creates one hidden AP and one client. For
  multi-AP scenarios, edit the `HWSIM_RADIOS=3` variable at the top of
  `setup.sh` to add more radios, then create additional namespaces and
  hostapd instances to match.
