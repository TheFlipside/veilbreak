//! Tshark subprocess management and EK-JSON parser.
//!
//! Runs `tshark` against the pcap produced by airodump-ng with display
//! filters for hidden-SSID reveal frames. Parses `-T ek` line-delimited
//! JSON and emits [`SsidRevealed`](crate::AppEvent::SsidRevealed) events.
//! Implemented in Phase 3.
