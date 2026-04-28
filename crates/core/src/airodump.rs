//! Airodump-ng subprocess management and CSV parser.
//!
//! Spawns `airodump-ng` with the appropriate flags, parses the live CSV
//! it produces, and emits [`AppEvent`](crate::AppEvent) variants for
//! discovered APs and clients. Implemented in Phase 2.
