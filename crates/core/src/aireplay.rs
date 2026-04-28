//! Aireplay-ng deauth job management.
//!
//! Spawns `aireplay-ng --deauth` for broadcast or targeted deauthentication.
//! Captures output and emits [`DeauthComplete`](crate::AppEvent::DeauthComplete)
//! on exit. Implemented in Phase 4.
