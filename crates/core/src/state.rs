//! Application state: AP table, client table, session metadata.
//!
//! [`AppState`] is the single source of truth for everything the dashboard
//! displays. It is owned and mutated exclusively by the app event loop.

use std::collections::HashMap;
use std::time::Instant;

/// Central application state, mutated only by the event loop.
#[derive(Debug)]
pub struct AppState {
    /// Known access points, keyed by BSSID.
    pub access_points: HashMap<String, AccessPoint>,
    /// Session start time.
    pub started_at: Instant,
    /// Current capture file size in bytes.
    pub capture_size: u64,
    /// Event log entries (newest last).
    pub event_log: Vec<EventLogEntry>,
}

impl AppState {
    /// Creates a new empty state with the clock starting now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            access_points: HashMap::new(),
            started_at: Instant::now(),
            capture_size: 0,
            event_log: Vec::new(),
        }
    }

    /// Elapsed time since session start, as whole seconds.
    #[must_use]
    pub fn elapsed_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// A discovered access point.
#[derive(Debug, Clone)]
pub struct AccessPoint {
    /// BSSID (MAC address of the AP).
    pub bssid: String,
    /// SSID, if known. `None` means hidden and not yet revealed.
    pub ssid: Option<String>,
    /// Operating channel.
    pub channel: u32,
    /// Signal strength in dBm (negative).
    pub power: i32,
    /// Encryption type (e.g. `"WPA2"`, `"WPA3"`, `"OPN"`).
    pub encryption: String,
    /// Associated clients.
    pub clients: Vec<Client>,
    /// Total beacon frames observed.
    pub beacon_count: u64,
    /// Whether the AP advertises a hidden SSID.
    pub hidden: bool,
}

/// A client station associated to an access point.
#[derive(Debug, Clone)]
pub struct Client {
    /// Client MAC address.
    pub mac: String,
    /// Signal strength in dBm (negative).
    pub power: i32,
    /// BSSID of the AP the client is associated with.
    pub bssid: String,
}

/// An entry in the event log displayed in the TUI.
#[derive(Debug, Clone)]
pub struct EventLogEntry {
    /// When the event occurred (seconds since session start).
    pub elapsed_secs: u64,
    /// Human-readable event description.
    pub message: String,
}
