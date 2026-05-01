# Changelog

All notable changes to this project are documented in this file.

<!-- markdownlint-disable MD024 -->

## Unreleased

### Added

### Fixed

### Security

### Changed

## 0.9.1 - 2026-05-01

### Added

- `is_p2p_bssid()` validator in `core::validate`: detects Wi-Fi Alliance OUI (`50:6F:9A`) BSSIDs used by Wi-Fi Direct / P2P group owners; gates on `is_valid_bssid()` for safety
- `wifi_direct` theme style (default: yellow) for P2P BSSID highlighting in all three preset themes (`default.toml`, `solarized-dark.toml`, `high-contrast.toml`)
- "Hide Wi-Fi Direct" filter in the filter modal (`f` key): hides BSSIDs using the Wi-Fi Alliance OUI (`50:6F:9A`), commonly seen from Wi-Fi Direct / P2P group owners
- P2P BSSID visual highlighting in the AP list table — Wi-Fi Alliance OUI BSSIDs rendered in the `wifi_direct` theme color
- `ADAPTERS.md` — wireless adapter compatibility guide with tested adapters (Verified / Community status), monitor-mode-only adapters, known incompatible chipsets, detailed chipset notes, and links to community resources (morrownr/USB-WiFi, airgeddon wiki, aircrack-ng, Kali docs)
- "Adapter Compatibility" section in README with `aireplay-ng --test` instructions and link to `ADAPTERS.md`
- "Interface Naming" section in README explaining systemd's predictable naming (`wlx...`) and how to revert to `wlan0`-style names via `net.ifnames=0`
- 5 unit tests for `is_p2p_bssid()` covering valid P2P BSSIDs, non-P2P BSSIDs, and malformed input
- 4 unit tests for `FilterState::matches()` covering `hide_p2p`, `hidden_only`, and band filters

### Security

- **Setup modal display sanitization hardened**: `WirelessInterface` fields (`name`, `phy`, `addr`) in `draw_interface_select`, `draw_mode_confirm`, and `draw_deauth_card_select` now passed through `sanitize_display_string()` at the rendering boundary — defense-in-depth independent of upstream parser validation

### Changed

- `FilterState::matches()` changed from `const fn` to `fn` (now calls `is_p2p_bssid` which is not const)
- `FILTER_ROW_COUNT` increased from 2 to 3 to accommodate the new "Hide Wi-Fi Direct" row
- Filter modal row 2 uses explicit `2 =>` match arm instead of wildcard `_` to prevent silent absorption of future rows
- Filter modal height increased from 8 to 9 lines to fit the third filter row

## 0.9.0 - 2026-05-01

### Added

- Dedicated deauth card support: optionally use a second wireless adapter exclusively for sending deauth frames while the scan card channel-hops freely
  - `DeauthCardSelect` setup wizard step: shown when multiple monitor-capable cards exist; "Same as scan card" option preserves current behavior with channel-hop risk warning
  - `core::monitor` module: `MonitorGuard` RAII guard enters monitor mode on the deauth card at session start and restores managed mode on drop (including panics and crashes)
  - `resolve_binary()`: resolves `ip`/`iw` to absolute paths from `/usr/sbin/`, `/sbin/`, `/usr/bin/`, `/bin/`; never falls back to bare-name PATH lookup
  - `MonitorError` enum with `LinkDown`, `SetMonitor`, `SetManaged`, `LinkUp`, `InvalidArgument` variants
  - `DashboardState::deauth_interface` field: deauth dispatch uses the dedicated card when set, falls back to scan card otherwise
  - Header bar displays both interfaces when a dedicated deauth card is selected (`"iface: wlan0 + wlan1"`)
  - Automatic fallback: if `MonitorGuard::enter()` fails, the deauth card is cleared and the scan card is used with a warning logged to the event pane
  - `all_interfaces` threaded through `BandSelect` and `ModeConfirm` setup variants to avoid re-running `iw` for the deauth card step
- Version number displayed in header title bar via `concat!` + `env!("CARGO_PKG_VERSION")` (zero-allocation, compile-time)
- Release workflow (`.forgejo/workflows/release.yml`): tag-triggered CI builds `.tar.gz` and `.deb` artifacts, publishes to both Forgejo and GitHub
  - Tag format validation (`^vN.N.N$`) as first step; rejects non-semver tags
  - `cargo-deb` integration with runtime dependencies (`aircrack-ng`, `tshark`, `iw`, `iproute2`, `wireless-tools`)
  - Theme files included in both tarball and `.deb` under `/usr/share/veilbreak/themes/`
- AUR PKGBUILD template (`pkg/aur/PKGBUILD`) for Arch Linux packaging
- `[package.metadata.deb]` section in `crates/tui/Cargo.toml` for Debian packaging via `cargo-deb`
- Cross-band ghost AP explanation added to README under the filter section
- `wifi-testlab/`: virtual WiFi lab using `mac80211_hwsim` for developing and testing veilbreak without physical hardware
  - `setup.sh`: lab lifecycle management (`--up`, `--down`, `--status`, `--restart`) — creates six virtual radios, isolates APs and client in network namespaces, enables monitor mode
  - `verify.sh`: smoke test confirming kernel module, services (hidden + 3 visible APs), client association, and monitor mode
  - `configs/hostapd.conf`: hidden AP configuration (`ignore_broadcast_ssid=1`, WPA2-PSK, channel 6)
  - `configs/hostapd-tplink.conf`: visible AP — TP-LINK_8907_5G (WPA2, channel 11)
  - `configs/hostapd-ddw.conf`: visible AP — DDW36563 (WPA2, channel 1)
  - `configs/hostapd-suddenlink.conf`: visible AP — SuddenLink990 (WPA2, channel 6)
  - `configs/wpa_supplicant.conf`: client configuration with `scan_ssid=1` for active probing
  - `GUIDE.md`: detailed usage walkthrough and architecture documentation
- `demo.gif`: animated demo showing AP discovery, hidden-SSID reveal via deauth, and filter/sort features
- `demo.tape`: VHS tape script for reproducing `demo.gif` against the wifi-testlab
- `RECORDING.md`: step-by-step guide for recording the demo GIF (VHS, Go, testlab setup, sudoers, troubleshooting)
- `Band` enum in `core::airodump` with `Bg`, `A`, `Abg` variants and `as_arg()`, `label()`, `next()` accessors
- `--band <BAND>` CLI flag: select Wi-Fi band (`bg`, `a`, `abg`) without interactive prompt; defaults to `bg` (2.4 GHz)
- `SetupScreen::BandSelect` step in setup flow: interactive band selection between interface pick and mode confirm
- Band shown in header bar and mode confirmation screen
- Help overlay (`?` key): full-screen keybind reference modal with all dashboard shortcuts
- Filter modal (`f` key): toggle hidden-only filter and band filter (All / 2.4 GHz / 5 GHz) for the AP list
- `Modal` enum generalizing deauth, filter, and help overlays (replaces `Option<DeauthModal>`)
- `FilterState` and `BandFilter` types on `DashboardState` with `matches()` predicate for AP filtering
- `FILTER_ROW_COUNT` constant in input handler for maintainable filter modal navigation bounds
- `AccessPoint::revealed` field: tracks APs that were initially hidden and later had their SSID revealed during the session
- `theme::REVEALED` style (green): visually distinguishes revealed SSIDs in AP list and detail pane
- `core::persist` module: `RevealRecord` struct and `write_reveal_entry()` NDJSON serializer for revealed-SSID logging
- `PersistError` error type with `Io` and `Serialize` variants, registered in `core::Error`
- `revealed.jsonl` persistence: each `SsidRevealed` event appends an NDJSON record to the session output directory
- `--output-dir <DIR>` CLI flag: use an existing directory for session output instead of auto-created temp directory
- `resolve_output_dir()` helper extracting output directory resolution from `run_tui`
- `--replay <PCAP>` one-shot mode: loads companion CSV for AP/client state, spawns tshark for SSID reveals, with path hardening (`canonicalize` + `is_file` check)
- `drain_events()` helper in app loop: extracts first + up to 64 queued events, keeping `run()` under clippy's 100-line limit
- Periodic 1-second redraw timer in dashboard (always active, not gated on loading state) — fixes frozen elapsed time in replay mode
- `Screen::Loading` variant with animated dot indicator during async interface detection
- `DetectGuard` abort-on-drop wrapper ensuring the detection task never outlives the app
- `f`/filter and `?`/help keybind hints added to all three focus pane hint arrays
- 2 unit tests for NDJSON serialization in `core::persist` (single record + multi-line append)
- `core::aireplay` module: spawns `aireplay-ng --deauth` for broadcast or targeted deauthentication, emits `DeauthComplete` on success and `AppEvent::Error` on failure
- `DeauthTarget` enum with `Broadcast` and `Targeted` variants and `bssid()` accessor
- `run_deauth()` async function with defense-in-depth validation of BSSID, client MAC, and interface name at the command boundary
- `AireplayError::Failed` and `AireplayError::InvalidArgument` error variants
- Deauth modal overlay: centered dialog with broadcast option and per-client targeted deauth, sorted by signal strength
- `DeauthModal` struct on `DashboardState` for modal state management
- `Outcome::Deauth(DeauthTarget)` variant for action dispatch from input to event loop
- `d` key opens deauth modal from AP list and detail panes (requires active interface, no-op in replay mode)
- Modal key handling: `↑`/`↓`/`j`/`k` navigation, `Enter` to send, `Esc`/`q` to cancel
- `DeauthGuard` struct with `Drop` impl to abort all deauth `JoinHandle`s on any exit path
- `BORDER_DANGER` theme style (red) for action modals with side effects
- `DEFAULT_DEAUTH_COUNT` constant (5 frames), frame count clamped to [1, 128] with `tracing::warn` on clamping
- 7 unit tests for frame count clamping, target accessors, validation rejection, and spawn failure handling in `core::aireplay`
- `core::tshark` module: periodic tshark poll against airodump pcap, EK-JSON parser, `TsharkController` with 3-second poll loop and `HashSet`-based dedup
- `parse_ek_json_line()` pure parser: extracts BSSID/SSID from probe responses (`0x05`), association requests (`0x00`), reassociation requests (`0x02`), and beacon leaks (`0x08`)
- `build_display_filter()`: constructs combined tshark display filter with BSSID validation (defense-in-depth)
- `RevealPacket` type with BSSID case normalization (tshark lowercase → uppercase to match airodump CSV)
- `TsharkError::Failed` and `TsharkError::Parse` error variants
- `AirodumpController::pcap_path()` accessor for the capture file path (`<prefix>-01.cap`)
- Interface name propagated from setup flow into `DashboardState` on `ModeConfirm` → `Dashboard` transition
- `AirodumpController` + `TsharkController` spawned on dashboard entry; hidden BSSIDs fed to tshark after each state-changing event batch
- Session output directory with randomized suffix and `0o700` permissions (`/tmp/veilbreak-<pid>-<hex>`)
- Test fixture `tests/fixtures/tshark_ek.jsonl` with 9 NDJSON lines covering all frame subtypes, edge cases (empty SSID, invalid BSSID, unknown subtype, control char sanitization)
- 14 unit tests for EK-JSON parsing, filter building, and fixture coverage in `core::tshark`
- LGPL-3.0 license file
- `core::airodump` module: spawns `airodump-ng`, parses live CSV, diffs AP/client state, emits events via `mpsc` channel
- `CsvSnapshot`, `AirodumpController` types for subprocess lifecycle management
- `diff_and_emit()` diffing engine: emits `ApDiscovered`, `ApUpdated`, `ClientSeen` events against known state
- `SortColumn` enum with cycling (`Power` → `Channel` → `Clients` → `Beacons` → `Bssid`)
- AP list widget with 8-column table, sort indicator, selection highlight
- Detail pane widget showing BSSID, SSID, channel, encryption, power, beacons, and associated clients
- Event log widget with reverse-chronological display and bounded scroll
- Header bar widget with interface, channel, capture size, AP count, and elapsed time
- Dashboard layout: header, split body (AP list 55% / detail 45%), event log, keybind bar
- `DashboardState` with focus pane cycling (`Tab`/`BackTab`), AP selection, sort cycling, event log scroll
- `AppState::apply_event()` as the single state mutation point for all event types
- `AppState::sorted_aps()` returning sorted `(bssid, &AccessPoint)` pairs
- `AppState::log_event()` with bounded `VecDeque` (cap: 10,000) and O(1) eviction
- `sanitize_display_string()` in `core::validate`: strips ASCII controls, DEL, C1 codes, soft hyphens, Unicode Bidi overrides/isolates
- `truncate_utf8()` in `core::validate`: safe UTF-8-aware string truncation
- `MAX_ESSID_LEN` constant (32 bytes, IEEE 802.11 limit)
- `is_valid_phy_name()` validator in `core::validate` for phy identifiers (`phyN` format)
- Phy name validation in both `parse_iw_dev` and `parse_iw_list` parsers
- `O_NOFOLLOW` flag and `0600` permissions on `/tmp/veilbreak.log` to prevent symlink attacks
- `run_tui()` helper in `main.rs` guaranteeing terminal restore on all exit paths
- Test fixture `tests/fixtures/airodump.csv` with 5 APs (2 hidden, 1 OPN) and 5 clients
- 14 unit tests for CSV parsing, diffing, and edge cases in `core::airodump`
- 5 unit tests for sanitization and truncation in `core::validate`
- `AppEvent::ChannelChanged(u32)` event variant for propagating channel-hop updates from the monitor interface
- `AppState::current_channel` field tracking the current monitor channel, updated via `ChannelChanged` events
- `channel_watch_loop` in `core::airodump`: polls `iw dev <iface> info` every 1s, parses the channel, emits `ChannelChanged` on change
- `parse_iw_channel()` parser for extracting the channel from `iw dev` output
- `validate::is_valid_channel()`: validates 802.11 channel numbers (1–196)
- 3 unit tests for `parse_iw_channel` (fixture, missing channel, 5 GHz channel)
- 2 unit tests for `ChannelChanged` state transitions (`None` → `Some`, overwrite)
- 2 unit tests for channel number validation (valid and invalid ranges)

### Fixed

- **Stdin theft from TUI**: subprocesses spawned by `iw dev` channel polling and `iw` interface detection were inheriting stdin, causing the TUI to lose keyboard input; fixed by adding `stdin(Stdio::null())` to all subprocess spawn sites
- **`ApUpdated` clobbering known channel with `None`**: CSV parser emitted `None` channel when the field was empty, overwriting a previously known channel; `AccessPoint::channel` changed to `Option<u32>` and update logic now preserves `Some` values
- **Unknown channel display**: header and AP list now show em dash (`—`) for unknown channels instead of `0`; deauth modal shows feedback message on dispatch
- **Channel display in header**: `ch:` field in the header bar now shows the current channel being hopped to by the monitor interface, polled via `iw dev <iface> info` every 1 second; previously always showed `–` because the field was never populated
- `revealed.jsonl` now captures SSID reveals from CSV updates (`ApUpdated`), not only tshark's `SsidRevealed` — fixes empty reveal log when airodump-ng's CSV parser discovers the SSID before tshark's 3-second poll fires
- `diff_and_emit()` now triggers `ApUpdated` on SSID changes, not only power/beacon/channel changes — fixes silent reveal loss when RF conditions are stable
- Session output directory printed to stderr on exit so the user can find pcap, CSV, and reveal log files
- `is_root()` now checks effective UID (`geteuid`) instead of real UID for setuid correctness
- Stderr from failed `iw` commands capped to 512 bytes to prevent log flooding
- Interfaces with invalid phy names are now rejected during parsing
- `AirodumpController::Drop` no longer panics in async context (`blocking_lock()` replaced with `try_lock()` + PID fallback)
- Redundant `AirodumpController::stop()` removed — `Drop` is the sole cleanup path
- `changes_hidden_set()` now includes `ApUpdated` — fixes stale tshark BSSID filter when an AP's SSID appeared via CSV update rather than `SsidRevealed`
- `--replay` mode: elapsed time now ticks continuously (periodic redraw timer always active); event log timestamps no longer all show `00:00` (replay events applied as summary, not individually)
- Startup delay eliminated: interface detection runs in a background task while the loading screen renders immediately

- Configurable color theme via TOML files (`--theme <FILE>` or auto-discovered at `$XDG_CONFIG_HOME/veilbreak/theme.toml`)
- `Theme` struct with `OnceLock` singleton pattern replacing hardcoded `const` style values
- `ThemeFile` TOML schema with `deny_unknown_fields` — partial overrides merge with built-in defaults
- `default_path()` config resolution: `$XDG_CONFIG_HOME` → `$SUDO_USER` home via `getpwnam` → `$HOME`
- `load_from_file()` hardened file loader: `O_NOFOLLOW | O_CLOEXEC`, `fstat`-based checks, `take()` size cap (64 KiB)
- Strict color parsing: named color allowlist (case-insensitive, with `grey` aliases) + `#RRGGBB` hex
- Modifier overrides via `Option<bool>`: `None` inherits, `Some(true)` adds, `Some(false)` explicitly removes
- 3 preset themes in `themes/`: `default.toml`, `solarized-dark.toml`, `high-contrast.toml`
- 11 unit tests for theme loading (color parsing, partial overrides, modifier inheritance, unknown field rejection)
- `toml` crate v1.1 added to workspace dependencies

### Security

- **Monitor mode binary path injection prevented**: `resolve_binary()` searches only root-owned system directories; never falls back to bare-name PATH resolution; `Drop` impl re-validates absolute path prefix before executing restore commands
- **Monitor mode subprocess stdin closed**: all `ip`/`iw` subprocesses in both async and blocking paths spawned with `stdin(Stdio::null())`
- **Unsanitized error paths hardened**: `MonitorGuard::enter()` failure message, replay error, and CSV companion path error sanitized via `sanitize_display_string()` before display
- **Header interface name bounded**: deauth interface name truncated to 15 characters and sanitized before rendering
- **Airodump channel poll stdin closed**: `iw dev <iface> info` subprocess in the 1-second channel polling loop now spawned with `stdin(Stdio::null())`, preventing stdin theft from the TUI
- **Interface detection stdin closed**: `run_iw()` subprocess spawned with `stdin(Stdio::null())`
- **Theme file symlink attack prevented**: `load_from_file()` opens with `O_NOFOLLOW | O_CLOEXEC`, rejects symlinks before reading; `fstat` on the open fd eliminates TOCTOU between stat and open
- **Theme file size bounded**: `take(64 KiB + 1)` hard cap on read prevents memory exhaustion from large files; double-checked via `fstat` pre-read and post-read length assertion
- **Theme TOML injection prevented**: `deny_unknown_fields` rejects unrecognised keys; color values validated against strict allowlist before conversion
- **`getpwnam` thread-safety upheld**: `main()` changed from `#[tokio::main] async fn` to sync `fn` with manual `Runtime::new()` — all pre-flight work (CLI, logging, theme) runs before any tokio threads spawn
- **`pw_dir` null dereference prevented**: explicit null check on `(*pw).pw_dir` before `CStr::from_ptr` — POSIX does not guarantee non-null `pw_dir` on all NSS backends
- **Theme auto-discovery TOCTOU eliminated**: `load_from_file` called unconditionally; `exists()` only checked in error path for diagnostic warning
- **Aireplay command injection prevented**: `run_deauth()` validates BSSID, client MAC, and interface name via strict regex before passing to `Command::arg()`; no shell interpolation
- **Aireplay stderr bounded**: stderr from failed `aireplay-ng` truncated to 512 bytes via `truncate_utf8` (UTF-8 boundary safe) and sanitized before display
- **Aireplay stdin closed**: subprocess spawned with `stdin(Stdio::null())` to prevent ambient terminal inheritance
- **Concurrent deauth bounded**: `MAX_CONCURRENT_DEAUTHS` cap (8) prevents unbounded process accumulation
- **Deauth task lifecycle guaranteed**: `DeauthGuard` with `Drop` impl aborts all `JoinHandle`s on every exit path (`?` propagation, quit, channel disconnect) — no detached tasks
- **Event channel backpressure logged**: all `try_send` failures in `run_deauth` logged via `tracing::warn` instead of silently dropped
- **Tshark filter injection prevented**: `build_display_filter()` validates every BSSID via `is_valid_bssid()` before interpolation into the display filter expression
- **Tshark output bounded at read time**: `run_tshark()` streams stdout via `AsyncReadExt::take()` with a 10 MiB hard cap, preventing unbounded heap allocation from large pcaps; child is killed on overflow
- **TOCTOU race on pcap path eliminated**: removed `pcap_path.exists()` pre-check; tshark failure is handled gracefully by the poll loop
- **Event channel disconnection detected**: `TryRecvError::Disconnected` now returns an error instead of silently breaking the drain loop
- **Session spawn bounded**: controller spawn attempted exactly once per session to prevent infinite retry on transient failure
- **Output directory hardened**: randomized suffix prevents PID prediction attacks; `0o700` mode prevents local information disclosure of pcap data
- **Airodump orphan process prevented**: `AirodumpController::Drop` uses `try_lock()` for the clean `Child::start_kill()` path, falling back to `libc::kill(pid, SIGKILL)` when the waiter task holds the lock — contention proves the child is alive and unreacped, so no PID reuse risk
- **Tshark poll loop exits on channel close**: `try_send` now distinguishes `Full` (transient, ignored) from `Closed` (app shutdown, exit loop) instead of spinning indefinitely
- **Terminal escape injection mitigated**: all untrusted strings (SSIDs, encryption, error messages, iw stderr, interface modes) sanitized via `sanitize_display_string()` at ingestion, state mutation, and log sink layers
- **ESSID overflow prevented**: SSIDs truncated to 32 bytes at parse time and re-validated on every state write path
- **Unbounded collection growth capped**: event log (10,000), access points (4,096), clients per AP (256)
- **BSSID/MAC validation at state boundary**: all event handlers validate BSSID and client MAC format before state mutation
- **Output directory validated**: `AirodumpController::spawn()` canonicalizes and verifies `output_dir` is a directory
- **Bounded event processing**: both `AppEvent` and terminal input drains capped at 64 per frame to prevent starvation
- **Unicode Bidi attack surface closed**: `sanitize_display_string` strips directional overrides, isolates, zero-width marks, C1 controls, and soft hyphens
- **Replay path hardened**: `load_replay` canonicalizes the pcap path and rejects non-regular files (devices, FIFOs, `/proc/*`, `/dev/*`) before reading, preventing symlink-based information disclosure when running as root
- **wifi-testlab state file injection prevented**: `source` replaced with `load_interfaces()` — strict key-allowlist parser with `^[a-zA-Z0-9_-]{1,15}$` value regex; eliminates arbitrary code execution via tampered `.run/interfaces` file
- **wifi-testlab run directory hardened**: `.run/` created with mode `0700`, `interfaces` file set to `0600`; prevents local information disclosure of interface names and PIDs
- **wifi-testlab PID validation**: all PID file reads validated with `^[0-9]+$` before `kill`, preventing `kill -1` (all processes) via corrupted PID file
- **wifi-testlab interface name validation**: `validate_iface()` enforces `^[a-zA-Z0-9_-]{1,15}$` on all hwsim-derived interface names before use in `iw`/`ip`/`sed` commands
- **wifi-testlab generated configs hardened**: runtime hostapd configs in `.run/` set to `0600` permissions, preventing local information disclosure of AP configuration including PSK
- **wifi-testlab BSSID display validated**: `verify.sh` validates BSSID from `iw` output against `^([0-9A-F]{2}:){5}[0-9A-F]{2}$` before display
- **wifi-testlab cleanup trap**: `lab_up` installs `trap lab_down EXIT` before forking subprocesses, preventing orphaned processes on unexpected script termination
- **wifi-testlab wpa_supplicant socket restricted**: `ctrl_interface_group=0` limits control socket access to root only
- **FD leak to child processes closed**: `open_reveal_log` and `init_logging` now set `O_CLOEXEC`, preventing airodump-ng/tshark/aireplay-ng from inheriting the reveal log and session log file descriptors
- **Output directory TOCTOU eliminated**: `resolve_output_dir` now canonicalizes the path before the `is_dir()` check, closing the symlink-swap window between stat and canonicalize
- **Session path terminal injection prevented**: output directory path sanitized via `sanitize_display_string()` before printing to stderr on exit
- **Reveal log symlink attack prevented**: `open_reveal_log` opens `revealed.jsonl` with `O_NOFOLLOW` flag, preventing symlink-based file overwrite when running as root
- **Log file moved into session directory**: `veilbreak.log` now written to the session output directory (mode `0o700`) instead of fixed `/tmp/veilbreak.log`, eliminating predictable-path hard-link information disclosure
- **User-supplied output directory canonicalized**: `--output-dir` path is canonicalized via `Path::canonicalize()` immediately after validation, resolving symlink components and `..` traversal before any file operations
- **Channel number validated at parse boundary**: CSV parser and `iw dev` output parser now reject channels outside the 802.11 range (1–196) via `validate::is_valid_channel()`, preventing display of attacker-controlled values from crafted CSV data
- **Band filter guards unknown channels**: `FilterState::matches` rejects `channel == 0` (unknown) from the 2.4 GHz filter to prevent false positives
- **Empty-SSID reveal guard**: `ApUpdated` and `SsidRevealed` handlers reject empty SSIDs (after sanitization) before flipping `revealed`/`hidden` state, preventing blank-SSID APs from being permanently marked as revealed
- **BSSID validation on `ApUpdated`**: `apply_event` now validates BSSID format before processing `ApUpdated` events, matching the defense-in-depth pattern of all other event handlers

### Changed

- `AirodumpController::spawn()` accepts `Band` parameter and passes `--band` flag to airodump-ng
- `SetupScreen::InterfaceSelect` carries `cli_band: Option<Band>` to skip band prompt when CLI flag is set
- `SetupScreen::ModeConfirm` carries `band: Band` for forwarding to `DashboardState`
- `app::run()` refactored: initial screen detection, session spawn, and deauth dispatch extracted into helpers to stay within the 100-line clippy limit
- `_airodump_ctrl` renamed to `airodump_ctrl` (no longer underscore-prefixed — now actively passed to `try_spawn_session`)
- `TsharkController` hidden-BSSID updates gated behind `ApDiscovered`/`SsidRevealed` events only (avoids unnecessary lock+alloc on `ApUpdated`/`CaptureSize`)
- Channel display sourced from `AppState::current_channel` (populated by `ChannelChanged` events) instead of the removed `DashboardState::channel` field
- `AirodumpController` now manages a `channel_handle` task alongside `csv_handle`; both aborted on drop
- `run_tshark()` refactored from `Command::output()` to `Command::spawn()` with piped stdout/stderr, bounded streaming reads, and `kill_on_drop(true)`
- `AirodumpController` stores `child_pid: u32` alongside `Arc<Mutex<Child>>` for the `Drop` fallback kill path
- `detect_initial_screen()` moved from blocking `await` to a spawned task; `resolve_detect_task()` polls completion via the `tokio::select!` sleep arm
- `ui::draw()` accepts a `tick: u8` counter for deterministic loading animation (replaces `SystemTime`)
- Tshark poll loop uses owned `Vec<String>` for unrevealed BSSIDs to release lock before subprocess call
- CI actions pinned to commit SHAs instead of mutable tags
- `AccessPoint::clients` changed from `Vec<Client>` to `HashMap<String, Client>` for O(1) lookup
- `AppState::event_log` changed from `Vec` to `VecDeque` for O(1) eviction
- App event loop uses `tokio::select!` over mpsc receiver and terminal polling
- `InterfaceMode::Other` variant now sanitized before storage
- `DashboardState::modal` changed from `Option<DeauthModal>` to `Option<Modal>` enum supporting deauth, filter, and help overlays
- `draw_filter_modal()` accepts explicit `(selected, &FilterState)` parameters instead of re-matching `DashboardState::modal` internally
- `init_logging()` now accepts `&Path` and writes log file into the session output directory
- Output directory resolution extracted from `run_tui()` into `resolve_output_dir()`, called before logging init
- AP list in dashboard filtered through `FilterState::matches()` before rendering
- `FilterState::matches` hidden-only filter now includes revealed APs (previously they vanished from the filtered view on reveal)
- Filter modal label changed from "Hidden only" to "Hidden/revealed" to reflect the updated filter semantics
- Detail pane SSID display: revealed SSIDs show `"{name} (revealed)"` in green; hidden shows `"<hidden>"` in dim; removed redundant re-sanitization (fields are pre-sanitized at state write time)
- `main()` changed from `#[tokio::main] async fn` to sync `fn` with manual `tokio::runtime::Runtime::new()` + `block_on()` for `getpwnam` thread-safety
- Widget files (`ap_list`, `detail`, `events`, `header`, `keybinds`, `modal`) migrated from `theme::CONSTANT` to `theme::function()` accessors
- DESIGN.md: configurable theme marked as implemented in Out of Scope; persistence format marked as decided in Open Questions
- README.md: screenshot placeholder replaced with `demo.gif` embed
- wifi-testlab expanded from 3 to 6 virtual radios: 3 visible APs (TP-LINK_8907_5G ch 11, DDW36563 ch 1, SuddenLink990 ch 6) alongside the hidden AP, for a realistic multi-AP demo environment
