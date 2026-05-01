# Veilbreak

Veilbreak is a terminal-based tool that reveals hidden WiFi SSIDs from a single
keyboard-driven dashboard. It orchestrates `airodump-ng`, `tshark`, and
`aireplay-ng` behind the scenes, replacing the manual multi-terminal workflow of
monitoring, analyzing, and injecting with one unified interface.

![Veilbreak revealing a hidden SSID](demo.gif)

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
- **Dedicated deauth card** — optionally uses a second wireless adapter
  exclusively for deauth. The scan card can channel-hop freely while the deauth
  card holds the target AP's channel, dramatically improving reliability on
  5 GHz where channel-hopping often causes missed frames. Monitor mode is
  managed automatically (entered on startup, restored on exit via RAII guard).
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

A wireless adapter that supports **monitor mode** is required. Multiple cards
are supported and recommended:

- **Single card** — works, but host connectivity is lost during the session and
  deauth reliability suffers from channel-hopping.
- **Two cards** — one for scanning, one for either host connectivity or
  dedicated deauth. The setup wizard lets you choose.
- **Three cards** — scan + dedicated deauth + host connectivity. Ideal setup
  for 5 GHz environments where channel-hop timing makes single-card deauth
  unreliable.

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

## Setup Flow

On launch, Veilbreak walks through a setup wizard before starting the
dashboard:

1. **Interface select** — pick the wireless adapter for scanning. All detected
   interfaces are listed with their PHY, MAC address, and a `[mon]` tag if
   monitor mode is supported.
2. **Band select** — choose `2.4 GHz`, `5 GHz`, or `Both`. Skipped if
   `--band` was passed on the command line.
3. **Mode confirm** — review the selected interface and band. Single-card mode
   warns that host connectivity will be lost; dual-card mode notes it is
   preserved.
4. **Deauth card select** *(only shown when multiple monitor-capable cards
   exist)* — pick a dedicated card for sending deauth frames, or choose
   "Same as scan card" to use the scan adapter for both (current default
   behavior, with a channel-hop risk warning).

If a dedicated deauth card is selected, Veilbreak automatically puts it into
monitor mode (`ip link set down` → `iw set type monitor` → `ip link set up`).
On exit — including crashes and panics — the card is restored to managed mode
via a blocking RAII guard.

If monitor mode setup fails (e.g. the driver doesn't support injection), the
deauth card selection is cleared and the scan card is used as a fallback. A
warning is logged to the event pane.

## CLI Flags

| Flag                 | Description                                                                                                                                     |
|----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------|
| `--replay <PCAP>`    | Replay a captured pcap file instead of starting a live capture session. No root or wireless card required.                                      |
| `--band <BAND>`      | Wi-Fi band: `bg` (2.4 GHz), `a` (5 GHz), or `abg` (both). Defaults to `bg`. Skips the band prompt. `abg` is unreliable on some drivers.         |
| `--output-dir <DIR>` | Use an existing directory for session output (captures, logs, `revealed.jsonl`). Must already exist. Defaults to a randomized dir in `/tmp`.    |
| `--theme <FILE>`     | Load a custom theme TOML file. See [Theming](#theming) below.                                                                                   |
| `--help`             | Print usage information.                                                                                                                        |
| `--version`          | Print version.                                                                                                                                  |

## Keybinds

### Dashboard

| Key                        | Action                                                            |
|----------------------------|-------------------------------------------------------------------|
| `Up` / `Down` or `j` / `k` | Navigate AP list or scroll event log                              |
| `PgUp` / `PgDn`            | Page scroll (AP list & events pane)                               |
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

> **Note — cross-band ghost APs:** When scanning a single band you may
> occasionally see APs from the other band appear in the list, sometimes
> briefly marked as hidden before their SSID fills in. This is normal
> behaviour, not a bug. The `--band` flag controls which channels
> `airodump-ng` hops across, but this is a software-level channel list
> with no kernel or firmware enforcement. During startup — or when the
> hop schedule has not yet stabilised — the card may briefly visit
> channels outside the requested band. Any AP heard during that window
> is written to the CSV and persists in the table. Because the card is
> not dwelling on that channel, the first captured frame may lack the
> SSID field, causing a momentary "hidden" marker that resolves once a
> complete beacon or probe response arrives. Dual-band APs often use
> sequential MAC addresses for their radios (the offset direction is
> vendor-specific), so the ghost will appear as a separate BSSID from
> the same AP you already see on the correct band.

## Theming

Veilbreak supports custom color themes via TOML files. Three presets ship in
the `themes/` directory: `default.toml`, `solarized-dark.toml`, and
`high-contrast.toml`.

### Loading a theme

Load a theme file explicitly with the `--theme` flag:

```bash
sudo ./target/release/veilbreak-tui --theme themes/solarized-dark.toml
```

Or place a file at the default config path and it will be picked up
automatically:

```bash
mkdir -p ~/.config/veilbreak
cp themes/solarized-dark.toml ~/.config/veilbreak/theme.toml
sudo ./target/release/veilbreak-tui
```

The auto-discovered config path is resolved as follows:

1. `$XDG_CONFIG_HOME/veilbreak/theme.toml` (if `XDG_CONFIG_HOME` is set)
2. Otherwise, under `sudo`: `~<SUDO_USER>/.config/veilbreak/theme.toml`
   (the original user's home is resolved via `getpwnam`)
3. Otherwise: `$HOME/.config/veilbreak/theme.toml`

Steps 2 and 3 are mutually exclusive — only the first match is used.

The `--theme` flag always takes priority over auto-discovery. If the
auto-discovered file is missing, built-in defaults are used silently. If it
exists but is malformed, a warning is printed to stderr and defaults are used.

### Writing a theme

Every field is optional — omitted values inherit from the built-in defaults.
Partial overrides merge: setting only `fg` on `title` preserves its default
`bold` modifier.

```toml
[border]
fg = "dark_gray"

[border_focused]
fg = "cyan"

[title]
fg = "cyan"
bold = true

[selected]
fg = "white"
bg = "dark_gray"

[revealed]
fg = "green"
```

**Styles:** `border`, `border_focused`, `border_danger`, `header`, `title`,
`selected`, `keybind_key`, `keybind_desc`, `revealed`, `dim`.

**Colors:** Any ratatui named color (case-insensitive, underscores optional) or
`#RRGGBB` hex. Named colors: `black`, `red`, `green`, `yellow`, `blue`,
`magenta`, `cyan`, `gray`/`grey`, `white`, `dark_gray`, `light_red`,
`light_green`, `light_yellow`, `light_blue`, `light_magenta`, `light_cyan`.

**Modifiers:** `bold`, `italic`, `underline` (all optional booleans). Unset
modifiers keep the built-in default (e.g. `title` is bold by default; all
others are not). Set `bold = false` to explicitly remove a default modifier.

Unknown keys are rejected to catch typos — loading will error on unrecognised
style names or style fields.

## License

LGPL-3.0 — see [LICENSE](LICENSE) for details.
