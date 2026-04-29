# Veilbreak

Veilbreak is a terminal-based tool that reveals hidden WiFi SSIDs from a single
keyboard-driven dashboard. It orchestrates `airodump-ng`, `tshark`, and
`aireplay-ng` behind the scenes, replacing the manual multi-terminal workflow of
monitoring, analyzing, and injecting with one unified interface.

<!-- screenshot placeholder -->

## Disclaimer

Veilbreak is intended for **authorized security testing**, educational use, and
research on networks you own or have explicit permission to test. Unauthorized
interception of wireless traffic and deauthentication attacks are illegal in most
jurisdictions. The authors assume no liability for misuse.

## Capabilities

- **Live AP discovery** — continuously parses `airodump-ng` CSV output, showing
  access points with BSSID, SSID, channel, encryption, signal strength, beacon
  count, and associated clients in a sortable table.
- **Hidden-SSID reveal** — periodically polls the capture pcap with `tshark`,
  extracting SSIDs from probe responses, association requests, reassociation
  requests, and beacon leaks. Revealed SSIDs appear in-place and are logged to
  `revealed.jsonl`.
- **Deauthentication injection** — sends broadcast or targeted deauth frames via
  `aireplay-ng` to force clients to reassociate, triggering SSID disclosure.
  Concurrent deauths are bounded and all subprocess handles are tracked.
- **Filtering** — filter the AP list by hidden-only status and frequency band
  (All / 2.4 GHz / 5 GHz).
- **Session output** — captures, logs, and revealed-SSID records are written to a
  session directory (auto-generated in `/tmp` or user-specified via `--output-dir`).

## Requirements

Veilbreak requires Linux with root privileges and the following packages
installed:

| Package           | Provides                                    |
|-------------------|---------------------------------------------|
| `aircrack-ng`     | `airodump-ng`, `aireplay-ng`                |
| `tshark`          | `tshark` (part of the Wireshark CLI suite)  |
| `iw`              | Wireless interface and capability detection |
| `iproute2`        | `ip` — interface state management           |
| `wireless-tools`  | Legacy wireless utilities                   |

A wireless adapter that supports **monitor mode** is required. Dual-card setups
(one card for monitoring, one for connectivity) are supported and recommended.

### Debian / Ubuntu

```bash
sudo apt install aircrack-ng tshark iw iproute2 wireless-tools
```

### Arch Linux

```bash
sudo pacman -S aircrack-ng wireshark-cli iw iproute2 wireless_tools
```

### Fedora

```bash
sudo dnf install aircrack-ng wireshark-cli iw iproute wireless-tools
```

## Building

Veilbreak is written in Rust. You need a working Rust toolchain (1.85+ for
edition 2024) installed for your **regular user** — root does not need its own
toolchain.

### Install Rust (if not already present)

See [rustup.rs](https://rustup.rs) for installation options. The quickstart:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Build a release binary

```bash
cargo build --release -p veilbreak-tui
```

The binary is placed at `target/release/veilbreak-tui`.

## Running

Veilbreak needs root to manage wireless interfaces and spawn `airodump-ng`.
Since the binary is already compiled, root does not need a Rust toolchain —
just run the binary directly with `sudo`:

```bash
sudo ./target/release/veilbreak-tui
```

To write session output to a specific directory instead of an auto-generated
one in `/tmp`:

```bash
mkdir -p /tmp/vb-session
sudo ./target/release/veilbreak-tui --output-dir /tmp/vb-session
```

To replay a previously captured pcap file (no root required, no live capture):

```bash
./target/release/veilbreak-tui --replay path/to/capture.pcap
```

## CLI Flags

| Flag                 | Description                                                                                                                                     |
|----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------|
| `--replay <PCAP>`    | Replay a captured pcap file instead of starting a live capture session. No root or wireless card required.                                      |
| `--band <BAND>`      | Wi-Fi band: `bg` (2.4 GHz), `a` (5 GHz), or `abg` (both). Defaults to `bg`. Skips the band prompt. `abg` is unreliable on some drivers.         |
| `--output-dir <DIR>` | Use an existing directory for session output (captures, logs, `revealed.jsonl`). Must already exist. Defaults to a randomized dir in `/tmp`.    |
| `--help`             | Print usage information.                                                                                                                        |
| `--version`          | Print version.                                                                                                                                  |

## Keybinds

### Dashboard

| Key                        | Action                                                            |
|----------------------------|-------------------------------------------------------------------|
| `Up` / `Down` or `j` / `k` | Navigate AP list or scroll event log                              |
| `PgUp` / `PgDn`            | Page scroll (events pane)                                         |
| `Tab` / `Shift+Tab`        | Cycle focus pane (AP list, detail, event log)                     |
| `Enter`                    | Select AP / confirm action                                        |
| `s`                        | Cycle sort column (Power > Channel > Clients > Beacons > BSSID)   |
| `g` / `G`                  | Jump to first / last AP                                           |
| `d`                        | Open deauth modal for the selected AP (requires active interface) |
| `f`                        | Open filter modal                                                 |
| `?`                        | Open keybind reference                                            |
| `q` / `Esc`                | Quit application                                                  |

### Deauth Modal

| Key                        | Action                                |
|----------------------------|---------------------------------------|
| `Up` / `Down` or `j` / `k` | Select broadcast or a specific client |
| `Enter`                    | Send deauth frames                    |
| `Esc` / `q`                | Cancel and close modal                |

### Filter Modal

| Key                        | Action                     |
|----------------------------|----------------------------|
| `Up` / `Down` or `j` / `k` | Navigate filter rows       |
| `Enter` / `Space`          | Toggle the selected filter |
| `Esc` / `q` / `f`          | Close modal                |

Available filters:

- **Hidden only** — show only APs with concealed SSIDs
- **Band** — cycle through All, 2.4 GHz (channels 1-14), 5 GHz (channels 36+)

## License

LGPL-3.0 — see [LICENSE](LICENSE) for details.
