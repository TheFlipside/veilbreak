//! Veilbreak core library.
//!
//! Business logic, subprocess management, and parsers for the veilbreak
//! hidden-SSID reveal tool. This crate has no UI dependencies — it is
//! testable against fixture data and could power a future GUI front-end.

pub mod aireplay;
pub mod airodump;
pub mod error;
pub mod event;
pub mod interface;
pub mod state;
pub mod tshark;
pub mod validate;

pub use error::Error;
pub use event::AppEvent;
pub use state::AppState;
