# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Added

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

### Fixed

- `is_root()` now checks effective UID (`geteuid`) instead of real UID for setuid correctness
- Stderr from failed `iw` commands capped to 512 bytes to prevent log flooding
- Interfaces with invalid phy names are now rejected during parsing

### Security

- **Aireplay command injection prevented**: `run_deauth()` validates BSSID, client MAC, and interface name via strict regex before passing to `Command::arg()`; no shell interpolation
- **Aireplay stderr bounded**: stderr from failed `aireplay-ng` truncated to 512 bytes via `truncate_utf8` (UTF-8 boundary safe) and sanitized before display
- **Aireplay stdin closed**: subprocess spawned with `stdin(Stdio::null())` to prevent ambient terminal inheritance
- **Concurrent deauth bounded**: `MAX_CONCURRENT_DEAUTHS` cap (8) prevents unbounded process accumulation
- **Deauth task lifecycle guaranteed**: `DeauthGuard` with `Drop` impl aborts all `JoinHandle`s on every exit path (`?` propagation, quit, channel disconnect) — no detached tasks
- **Event channel backpressure logged**: all `try_send` failures in `run_deauth` logged via `tracing::warn` instead of silently dropped
- **Tshark filter injection prevented**: `build_display_filter()` validates every BSSID via `is_valid_bssid()` before interpolation into the display filter expression
- **Tshark output bounded**: `run_tshark()` rejects stdout exceeding 10 MiB to prevent memory exhaustion from large pcaps or broadened filters
- **TOCTOU race on pcap path eliminated**: removed `pcap_path.exists()` pre-check; tshark failure is handled gracefully by the poll loop
- **Event channel disconnection detected**: `TryRecvError::Disconnected` now returns an error instead of silently breaking the drain loop
- **Session spawn bounded**: controller spawn attempted exactly once per session to prevent infinite retry on transient failure
- **Output directory hardened**: randomized suffix prevents PID prediction attacks; `0o700` mode prevents local information disclosure of pcap data
- **PID reuse race eliminated**: `AirodumpController` holds `Arc<Mutex<Child>>` instead of raw `libc::pid_t`; `blocking_lock()` guarantees process termination in `Drop`
- **Terminal escape injection mitigated**: all untrusted strings (SSIDs, encryption, error messages, iw stderr, interface modes) sanitized via `sanitize_display_string()` at ingestion, state mutation, and log sink layers
- **ESSID overflow prevented**: SSIDs truncated to 32 bytes at parse time and re-validated on every state write path
- **Unbounded collection growth capped**: event log (10,000), access points (4,096), clients per AP (256)
- **BSSID/MAC validation at state boundary**: all event handlers validate BSSID and client MAC format before state mutation
- **Output directory validated**: `AirodumpController::spawn()` canonicalizes and verifies `output_dir` is a directory
- **Bounded event processing**: both `AppEvent` and terminal input drains capped at 64 per frame to prevent starvation
- **Unicode Bidi attack surface closed**: `sanitize_display_string` strips directional overrides, isolates, zero-width marks, C1 controls, and soft hyphens

### Changed

- `app::run()` refactored: initial screen detection, session spawn, and deauth dispatch extracted into helpers to stay within the 100-line clippy limit
- `_airodump_ctrl` renamed to `airodump_ctrl` (no longer underscore-prefixed — now actively passed to `try_spawn_session`)
- `TsharkController` hidden-BSSID updates gated behind `ApDiscovered`/`SsidRevealed` events only (avoids unnecessary lock+alloc on `ApUpdated`/`CaptureSize`)
- Tshark poll loop uses owned `Vec<String>` for unrevealed BSSIDs to release lock before subprocess call
- CI actions pinned to commit SHAs instead of mutable tags
- `AccessPoint::clients` changed from `Vec<Client>` to `HashMap<String, Client>` for O(1) lookup
- `AppState::event_log` changed from `Vec` to `VecDeque` for O(1) eviction
- App event loop uses `tokio::select!` over mpsc receiver and terminal polling
- `InterfaceMode::Other` variant now sanitized before storage

### 0.1.0 - 2026-04-29

### Added

- First release version
