# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Added

- `is_valid_phy_name()` validator in `core::validate` for phy identifiers (`phyN` format)
- Phy name validation in both `parse_iw_dev` and `parse_iw_list` parsers
- `O_NOFOLLOW` flag and `0600` permissions on `/tmp/veilbreak.log` to prevent symlink attacks
- `run_tui()` helper in `main.rs` guaranteeing terminal restore on all exit paths

### Fixed

- `is_root()` now checks effective UID (`geteuid`) instead of real UID for setuid correctness
- Stderr from failed `iw` commands capped to 512 bytes to prevent log flooding
- Interfaces with invalid phy names are now rejected during parsing

### Changed

- CI actions pinned to commit SHAs instead of mutable tags

### Removed

### 0.1.0 - 2026-04-29

### Added

- First release version
