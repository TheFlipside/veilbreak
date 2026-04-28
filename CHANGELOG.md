# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Added

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

- **PID reuse race eliminated**: `AirodumpController` holds `Arc<Mutex<Child>>` instead of raw `libc::pid_t`; `blocking_lock()` guarantees process termination in `Drop`
- **Terminal escape injection mitigated**: all untrusted strings (SSIDs, encryption, error messages, iw stderr, interface modes) sanitized via `sanitize_display_string()` at ingestion, state mutation, and log sink layers
- **ESSID overflow prevented**: SSIDs truncated to 32 bytes at parse time and re-validated on every state write path
- **Unbounded collection growth capped**: event log (10,000), access points (4,096), clients per AP (256)
- **BSSID/MAC validation at state boundary**: all event handlers validate BSSID and client MAC format before state mutation
- **Output directory validated**: `AirodumpController::spawn()` canonicalizes and verifies `output_dir` is a directory
- **Bounded event processing**: both `AppEvent` and terminal input drains capped at 64 per frame to prevent starvation
- **Unicode Bidi attack surface closed**: `sanitize_display_string` strips directional overrides, isolates, zero-width marks, C1 controls, and soft hyphens

### Changed

- CI actions pinned to commit SHAs instead of mutable tags
- `AccessPoint::clients` changed from `Vec<Client>` to `HashMap<String, Client>` for O(1) lookup
- `AppState::event_log` changed from `Vec` to `VecDeque` for O(1) eviction
- App event loop uses `tokio::select!` over mpsc receiver and terminal polling
- `InterfaceMode::Other` variant now sanitized before storage

### 0.1.0 - 2026-04-29

### Added

- First release version
