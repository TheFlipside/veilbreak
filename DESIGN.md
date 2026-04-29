# Veilbreak — Design Document

> A guided TUI orchestrator for revealing hidden WiFi SSIDs.

---

## 1. Problem Statement

Revealing the SSID of a "hidden" access point is a well-understood task, but the
manual workflow is awkward: it requires juggling several terminals, keeping
track of BSSIDs by hand, and switching mental context between scanning,
capturing, analyzing, and (optionally) injecting deauth frames. The individual
tools (`airodump-ng`, `tshark`, `aireplay-ng`) work fine — the friction is in
their orchestration.

Veilbreak is a single TUI application that drives the whole pipeline, presents
state live in one screen, and lets the user act on it with the keyboard.

---

## 2. Manual Workflow (What Veilbreak Replaces)

The reference workflow Veilbreak automates:

1. Identify a network card capable of monitor mode.
2. Configure the card for monitor mode without breaking host connectivity.
3. Start a wide scan (`airodump-ng wlan0mon`) so packets flow on the interface.
4. Capture everything to a pcap (handled by `airodump-ng -w`, no parallel
   `tshark` capture needed).
5. Periodically inspect the capture for hidden-SSID indicators.
6. For each hidden BSSID, watch for frames that leak the real SSID.
7. Optionally accelerate the reveal by sending deauth frames so clients
   reconnect and leak the SSID in plaintext.

Veilbreak collapses these into one screen with a clear state machine.

---

## 3. Goals

- **One screen, no terminal juggling.** AP list, detail pane, and event log
  visible simultaneously.
- **Keyboard-only, fast.** Vim-style navigation, single-key actions, no mouse
  required.
- **Works locally and over SSH.** Same binary, same UI, no display server
  needed. Useful on a laptop with a desktop session and on a headless
  Raspberry Pi rig.
- **Doesn't trash host connectivity.** First-class support for a two-card
  setup (one card stays online with NetworkManager, one is dedicated to
  monitor mode), with single-card mode as a fallback.
- **Quality bar:** zero `clippy` warnings, `cargo fmt` clean, all errors
  handled (no `unwrap()` outside tests).

---

## 4. Architectural Decisions

### 4.1 Capture Strategy

`airodump-ng` is the sole capture process. It already handles channel hopping
and writes pcap files via `-w`, and emits a live CSV of discovered APs and
clients that we parse for the AP list. Veilbreak does **not** run `tshark` in
parallel for capture — that would double-write. Instead, `tshark` runs
periodically (or as a long-lived process with `-l` line-buffered output)
against the pcap that airodump is writing, purely for analysis.

### 4.2 Hidden-SSID Reveal Filters

Beacons from a hidden AP carry an empty SSID by design. The SSID is leaked
when a real client interacts with the AP. Veilbreak watches for all of:

| Frame type                      | Subtype | Why it matters                                          |
| ------------------------------- | ------- | ------------------------------------------------------- |
| Probe Response                  | `0x05`  | AP responds to a probing client with the real SSID.     |
| Association Request             | `0x00`  | Client sends SSID in plaintext when joining.            |
| Reassociation Request           | `0x02`  | Same, on roaming or after deauth.                       |
| Beacon (post-association leak)  | `0x08`  | Some APs briefly leak after a client connects.          |

The combined display filter (logically):

```text
(wlan.fc.type_subtype == 0x05
 || wlan.fc.type_subtype == 0x00
 || wlan.fc.type_subtype == 0x02
 || (wlan.fc.type_subtype == 0x08 && wlan.tag.number == 0 && wlan.tag.length > 0))
&& wlan.bssid == <target>
```

For automated parsing we run `tshark -T ek` (line-delimited JSON) which is
trivial to consume from Rust via `serde_json`.

### 4.3 Deauth Strategy

The user picks the level of aggression:

- **Broadcast deauth** — `aireplay-ng --deauth N -a <bssid> wlan0mon`. Hits
  all clients of a BSSID at once. Less reliable; some clients ignore broadcast
  deauths.
- **Targeted deauth** — `aireplay-ng --deauth N -a <bssid> -c <client> wlan0mon`.
  Hits a single observed client. Much more reliable, available once Veilbreak
  has seen at least one client associated with the target BSSID.

The UI presents both choices, with targeted clients listed (with signal
strength) once observed.

### 4.4 Dual-Card Support

On startup Veilbreak parses `iw list` (or `iw phy`) for each interface to
determine which support monitor mode. If two such interfaces exist, the user
can pick one to dedicate to monitor mode while the other stays managed by
NetworkManager. This avoids `airmon-ng check kill`, which would otherwise
take down host connectivity.

Single-card mode is supported with a clear warning: the host will lose
network connectivity for the duration of the session.

### 4.5 Privilege Model

v1 runs the whole binary under `sudo`. This is the pragmatic choice for a
research/personal tool. A later version may split into a privileged helper
invoked via `pkexec` and an unprivileged TUI front-end communicating over a
local socket — the architecture below does not preclude this.

---

## 5. Stack

| Layer            | Choice                                                    |
| ---------------- | --------------------------------------------------------- |
| Language         | Rust (edition 2021, latest stable)                        |
| TUI framework    | `ratatui`                                                 |
| Terminal I/O     | `crossterm`                                               |
| Async runtime    | `tokio`                                                   |
| Subprocess mgmt  | `tokio::process`                                          |
| JSON parsing     | `serde` + `serde_json` (for `tshark -T ek`)               |
| CSV parsing      | `csv` crate (for `airodump-ng` live CSV)                  |
| CLI args         | `clap` (derive)                                           |
| Logging          | `tracing` + `tracing-subscriber`                          |
| Errors           | `thiserror` (library errors), `anyhow` (binary main)      |

Rationale: this stack is the same one used in ShellStation (russh aside), so
the learning surface is essentially just `ratatui`. Async subprocess
management is a strong fit for this tool's "many long-running pipelines
producing event streams" shape.

---

## 6. UI Layout

```text
┌─ veilbreak ────────────────────────────────────────────────────────────────┐
│ iface: wlan1mon  ch: 11 (hopping)  capture: 4.2 MB  elapsed: 02:14   [●REC]│
├──────────────────────────────────────┬─────────────────────────────────────┤
│ Access Points                        │ Selected: AA:BB:CC:DD:EE:FF         │
│  BSSID            CH  PWR ENC  CLI # │  SSID:    <revealed: HomeNet5G>     │
│ ▶AA:BB:CC:DD:EE:FF 11 -42 WPA2  3 127│  Channel: 11  Encryption: WPA2-CCMP │
│  11:22:33:44:55:66  6 -67 WPA2  1  42│  First seen: 02:01  Last: 02:14     │
│  77:88:99:AA:BB:CC  1 -78 WPA3  0   8│                                     │
│  DE:AD:BE:EF:00:01 36 -55 <hid> 2  89│  Associated clients:                │
│  CA:FE:BA:BE:00:02 11 -61 <hid> 0   3│   F0:11:22:33:44:55  -48 dBm  ↑↓    │
│                                      │   AA:CC:EE:00:11:22  -52 dBm  ↑     │
│                                      │   11:33:55:77:99:BB  -71 dBm  ↓     │
├──────────────────────────────────────┴─────────────────────────────────────┤
│ Events                                                                     │
│ 02:14:03  ssid revealed via probe-response  AA:BB:..:FF → "HomeNet5G"      │
│ 02:13:51  deauth sent broadcast → DE:AD:BE:EF:00:01 (5 frames)             │
│ 02:13:22  new AP DE:AD:BE:EF:00:01 ch36 hidden                             │
│ 02:12:08  client F0:11:22:33:44:55 associated to AA:BB:..:FF               │
├────────────────────────────────────────────────────────────────────────────┤
│ [Tab] focus  [↑↓/jk] nav  [Enter] select  [d] deauth  [s] sort  [?] help   │
└────────────────────────────────────────────────────────────────────────────┘
```

Layout structure:

- **Vertical split** — header / body / event log / keybind hint bar.
- **Body horizontal split** — AP list (left, ~55%) and detail pane (right, ~45%).
- **Hidden SSIDs** render as `<hid>` and flip to the revealed name in place
  the moment a leak is captured.

---

## 7. Interaction Model

### 7.1 Pre-Session Setup Flow

A small sequence of full-screen modals before the dashboard:

1. **Interface selection** — list of interfaces, each tagged with monitor-mode
   capability (parsed from `iw list`). The user picks one for monitoring; if a
   second monitor-capable card exists, the user can pin the first as the
   "online" card so NetworkManager keeps managing it.
2. **Mode confirmation** — single-card mode shows a warning that host
   connectivity will be lost; dual-card mode shows the planned configuration.
3. **Pre-flight checks** — verify `airodump-ng`, `tshark`, `aireplay-ng`
   are on PATH; verify root; bring the chosen card up in monitor mode.

Then drop into the dashboard.

### 7.2 Dashboard Keys

| Key            | Action                                                         |
| -------------- | -------------------------------------------------------------- |
| `Tab`/`S-Tab`  | Move focus between panes (AP list, detail pane, event log)     |
| `↑`/`↓`/`j`/`k | Navigate within focused list                                   |
| `g`/`G`        | Jump to top / bottom of list                                   |
| `Enter`        | Lock onto AP (populates detail pane, becomes deauth target)    |
| `d`            | Open deauth modal: broadcast / pick client / cancel            |
| `s`            | Cycle sort column (BSSID, channel, power, clients, age)        |
| `f`            | Filter modal (only hidden, only WPA2, channel band, etc.)      |
| `Space`        | Pause / resume channel hopping                                 |
| `c`            | Lock onto a fixed channel (modal input)                        |
| `r`            | Force-refresh AP list from current pcap                        |
| `?`            | Help overlay (full keymap)                                     |
| `q`            | Quit (with confirm modal if capture is running)                |

The focused pane gets a highlighted border. The keybind hint bar at the
bottom updates contextually based on focus.

### 7.3 Modals

Modals render as a centered rectangle with a dimmed background. Used for:

- Deauth target selection (broadcast vs. specific client)
- Channel lock input
- Filter configuration
- Help overlay
- Quit confirmation

---

## 8. Module Structure

Cargo workspace, two crates:

```text
veilbreak/
├── Cargo.toml                  # workspace root
├── justfile                    # dev task runner (lint, test, package)
├── crates/
│   ├── core/                   # business logic, no UI dependencies
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── interface.rs    # iw parsing, monitor-mode capability
│   │   │   ├── airodump.rs     # spawn + CSV parser
│   │   │   ├── tshark.rs       # spawn + ek-JSON parser
│   │   │   ├── aireplay.rs     # spawn deauth jobs
│   │   │   ├── event.rs        # AppEvent enum (the event bus type)
│   │   │   ├── state.rs        # AppState, AP/Client tables
│   │   │   └── error.rs        # thiserror types
│   └── tui/                    # ratatui app
│       ├── src/
│       │   ├── main.rs
│       │   ├── app.rs          # event loop, state mutations
│       │   ├── input.rs        # key → action mapping
│       │   ├── ui.rs           # ratatui draw functions
│       │   ├── widgets/
│       │   │   ├── ap_list.rs
│       │   │   ├── detail.rs
│       │   │   ├── events.rs
│       │   │   └── modal.rs
│       │   └── theme.rs
└── tests/
    └── fixtures/               # sample pcaps, sample airodump CSVs
```

Key separation:

- `core` owns all subprocess management, parsing, and state transitions. It
  has no `ratatui` or `crossterm` dependency. This means `core` is testable
  with plain pcap fixtures and could power a future GUI front-end without
  changes.
- `tui` owns rendering and input. It consumes `core::AppEvent` from a channel
  and dispatches `core::Action` back.
- Dev tasks live in a `justfile` at the repo root, run via
  [`just`](https://github.com/casey/just). If a task ever grows past what
  fits naturally in a recipe, it gets a normal binary in `crates/tools/`
  and the `justfile` recipe calls it — no `xtask` framework needed.

### 8.1 Event Loop Shape

```text
┌─────────────┐    AppEvent    ┌───────────────┐    Action    ┌─────────────┐
│ producers   │ ─────────────▶ │ app event     │ ───────────▶ │ subprocess  │
│ (airodump,  │                │ loop          │              │ controllers │
│  tshark,    │                │ (tokio task)  │              │             │
│  keypress)  │ ◀───────────── │               │ ◀─────────── │             │
└─────────────┘     redraw     └───────────────┘   ProcEvent  └─────────────┘
```

`AppEvent` covers everything: keypress, new AP discovered, AP updated, SSID
revealed, client seen, deauth completed, capture file size update, terminal
resize. Single channel, single consumer (the app loop), which keeps state
mutations linear and easy to reason about.

---

## 9. Reference Projects

Worth reading before/during implementation:

- **`bottom`** (`btm`) — multi-pane dashboard with focus handling.
- **`gitui`** — modal dialogs, keybind dispatch, status bar conventions.
- **`bandwhich`** — closest in spirit: live network data parsed in the
  background, presented in panes.
- **`atuin`** — fuzzy filtering and search-as-you-type in lists.

---

## 10. Roadmap

### Phase 1 — Skeleton

- Workspace scaffolding, CI (clippy + fmt + test on push).
- `core::interface` — list interfaces, detect monitor-mode capability.
- Pre-session setup modals.
- Empty dashboard with header and four empty panes.

### Phase 2 — Capture and Display

- `core::airodump` — spawn, parse live CSV, emit `ApDiscovered`,
  `ApUpdated`, `ClientSeen` events.
- AP list widget with sorting and selection.
- Detail pane populated from selection.

### Phase 3 — Reveal

- `core::tshark` — spawn against airodump's pcap with the combined display
  filter, parse `-T ek` JSON, emit `SsidRevealed` events.
- Detail pane updates hidden→revealed in place.
- Event log with leak source attribution.

### Phase 4 — Inject

- `core::aireplay` — spawn deauth jobs (broadcast and targeted).
- Deauth modal in the TUI.
- Client picker for targeted deauth.

### Phase 5 — Polish

- Filter modal (`f` key): hidden-only toggle and band filter (All / 2.4 GHz / 5 GHz).
- Help overlay (`?` key): full keybind reference modal.
- Persistence: revealed-SSID log (`revealed.jsonl`) written to session
  directory, `--output-dir` CLI flag for user-specified output path.

### Phase 6 (stretch) — Privilege split

- Carve out a privileged helper, switch the TUI to unprivileged, talk over
  a local socket. Optional, only if the tool gets shared more widely.

---

## 11. Out of Scope (for now)

- Channel locking and pause/resume hopping.
- Configurable theme (`theme.rs`).
- WPA handshake capture and cracking (covered well by existing tools).
- WPS attacks.
- 5GHz/6GHz beyond what the chosen card supports natively.
- A graphical front-end. The core/tui split keeps the door open, but no
  Tauri/Qt front-end is planned for v1.

---

## 12. Open Questions

- Should the channel hopping pattern be configurable (2.4-only, 5-only,
  custom list), or rely on `airodump-ng`'s defaults? Defaulting to airodump
  is simpler; configurability is a stretch goal.
- Persistence format for session output — raw pcap plus a structured JSON
  log of events seems sufficient. Worth revisiting after Phase 3.
- Distribution: `cargo install`, or also a packaged `.deb` / AUR / Nix flake?
  Decide before public release.
