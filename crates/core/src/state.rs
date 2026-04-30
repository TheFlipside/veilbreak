//! Application state: AP table, client table, session metadata.
//!
//! [`AppState`] is the single source of truth for everything the dashboard
//! displays. It is owned and mutated exclusively by the app event loop.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::event::AppEvent;
use crate::validate;

const MAX_EVENT_LOG: usize = 10_000;
const MAX_ACCESS_POINTS: usize = 4_096;
const MAX_CLIENTS_PER_AP: usize = 256;

/// Central application state, mutated only by the event loop.
#[derive(Debug)]
pub struct AppState {
    /// Known access points, keyed by BSSID.
    pub access_points: HashMap<String, AccessPoint>,
    /// Session start time.
    pub started_at: Instant,
    /// Current capture file size in bytes.
    pub capture_size: u64,
    /// Current channel the monitor interface is tuned to.
    pub current_channel: Option<u32>,
    /// Event log entries (newest last).
    pub event_log: VecDeque<EventLogEntry>,
}

impl AppState {
    /// Creates a new empty state with the clock starting now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            access_points: HashMap::new(),
            started_at: Instant::now(),
            capture_size: 0,
            current_channel: None,
            event_log: VecDeque::new(),
        }
    }

    /// Elapsed time since session start, as whole seconds.
    #[must_use]
    pub fn elapsed_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Applies an event to the state, updating AP/client tables and event log.
    pub fn apply_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::ApDiscovered(ap) => {
                if !validate::is_valid_bssid(&ap.bssid) {
                    return;
                }
                if self.access_points.len() < MAX_ACCESS_POINTS {
                    let mut sanitized = ap.clone();
                    sanitized.encryption = validate::sanitize_display_string(&sanitized.encryption);
                    sanitized.ssid = sanitized.ssid.map(|s| {
                        validate::sanitize_display_string(validate::truncate_utf8(
                            &s,
                            validate::MAX_ESSID_LEN,
                        ))
                    });
                    self.access_points
                        .entry(sanitized.bssid.clone())
                        .or_insert(sanitized);
                }
            }
            AppEvent::ApUpdated(ap) => {
                if !validate::is_valid_bssid(&ap.bssid) {
                    return;
                }
                if let Some(entry) = self.access_points.get_mut(&ap.bssid) {
                    entry.power = ap.power;
                    if ap.channel.is_some() {
                        entry.channel = ap.channel;
                    }
                    entry.beacon_count = ap.beacon_count;
                    entry.encryption = validate::sanitize_display_string(&ap.encryption);
                    if let Some(ssid) = &ap.ssid {
                        let sanitized = validate::sanitize_display_string(validate::truncate_utf8(
                            ssid,
                            validate::MAX_ESSID_LEN,
                        ));
                        if !sanitized.is_empty() {
                            if entry.hidden {
                                entry.revealed = true;
                            }
                            entry.ssid = Some(sanitized);
                            entry.hidden = false;
                        }
                    }
                }
            }
            AppEvent::ClientSeen(client) => {
                if !validate::is_valid_bssid(&client.mac)
                    || !validate::is_valid_bssid(&client.bssid)
                {
                    return;
                }
                if let Some(ap) = self.access_points.get_mut(&client.bssid)
                    && (ap.clients.len() < MAX_CLIENTS_PER_AP
                        || ap.clients.contains_key(&client.mac))
                {
                    ap.clients
                        .entry(client.mac.clone())
                        .and_modify(|c| c.power = client.power)
                        .or_insert_with(|| client.clone());
                }
            }
            AppEvent::SsidRevealed { bssid, ssid, .. } => {
                if !validate::is_valid_bssid(bssid) {
                    return;
                }
                if let Some(ap) = self.access_points.get_mut(bssid) {
                    let sanitized = validate::sanitize_display_string(validate::truncate_utf8(
                        ssid,
                        validate::MAX_ESSID_LEN,
                    ));
                    if !sanitized.is_empty() {
                        if ap.hidden {
                            ap.revealed = true;
                        }
                        ap.ssid = Some(sanitized);
                        ap.hidden = false;
                    }
                }
            }
            AppEvent::ChannelChanged(ch) => {
                self.current_channel = Some(*ch);
            }
            AppEvent::CaptureSize(size) => {
                self.capture_size = *size;
            }
            AppEvent::DeauthComplete { .. } | AppEvent::Error(_) => {}
        }
    }

    /// Pushes an entry to the event log, evicting the oldest entry when full.
    pub fn log_event(&mut self, message: impl Into<String>) {
        if self.event_log.len() >= MAX_EVENT_LOG {
            self.event_log.pop_front();
        }
        self.event_log.push_back(EventLogEntry {
            elapsed_secs: self.elapsed_secs(),
            message: validate::sanitize_display_string(&message.into()),
        });
    }

    /// Returns access points sorted by the given column, as `(bssid, &AP)` pairs.
    #[must_use]
    pub fn sorted_aps(&self, sort: SortColumn) -> Vec<(&str, &AccessPoint)> {
        let mut aps: Vec<_> = self
            .access_points
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        match sort {
            SortColumn::Bssid => aps.sort_by(|a, b| a.0.cmp(b.0)),
            SortColumn::Channel => {
                aps.sort_by_key(|&(_, ap)| ap.channel.unwrap_or(u32::MAX));
            }
            SortColumn::Power => aps.sort_by_key(|&(_, ap)| std::cmp::Reverse(ap.power)),
            SortColumn::Clients => {
                aps.sort_by_key(|&(_, ap)| std::cmp::Reverse(ap.clients.len()));
            }
            SortColumn::Beacons => {
                aps.sort_by_key(|&(_, ap)| std::cmp::Reverse(ap.beacon_count));
            }
        }

        aps
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
    /// Operating channel. `None` means the channel is not yet known.
    pub channel: Option<u32>,
    /// Signal strength in dBm (negative).
    pub power: i32,
    /// Encryption type (e.g. `"WPA2"`, `"WPA3"`, `"OPN"`).
    pub encryption: String,
    /// Associated clients, keyed by MAC address.
    pub clients: HashMap<String, Client>,
    /// Total beacon frames observed.
    pub beacon_count: u64,
    /// Whether the AP advertises a hidden SSID.
    pub hidden: bool,
    /// Whether the AP was initially hidden and later had its SSID revealed.
    pub revealed: bool,
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

/// Column used to sort the AP list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortColumn {
    /// Sort by BSSID lexicographically.
    Bssid,
    /// Sort by channel number.
    Channel,
    /// Sort by signal strength (strongest first).
    #[default]
    Power,
    /// Sort by number of associated clients (most first).
    Clients,
    /// Sort by beacon count (most first).
    Beacons,
}

impl SortColumn {
    /// Cycle to the next sort column.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Power => Self::Channel,
            Self::Channel => Self::Clients,
            Self::Clients => Self::Beacons,
            Self::Beacons => Self::Bssid,
            Self::Bssid => Self::Power,
        }
    }
}

impl std::fmt::Display for SortColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bssid => f.write_str("BSSID"),
            Self::Channel => f.write_str("CH"),
            Self::Power => f.write_str("PWR"),
            Self::Clients => f.write_str("CLI"),
            Self::Beacons => f.write_str("BCN"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_changed_sets_current_channel() {
        let mut state = AppState::new();
        assert_eq!(state.current_channel, None);

        state.apply_event(&AppEvent::ChannelChanged(6));
        assert_eq!(state.current_channel, Some(6));
    }

    #[test]
    fn channel_changed_overwrites_previous() {
        let mut state = AppState::new();
        state.apply_event(&AppEvent::ChannelChanged(6));
        state.apply_event(&AppEvent::ChannelChanged(11));
        assert_eq!(state.current_channel, Some(11));
    }

    fn make_ap(channel: Option<u32>) -> AccessPoint {
        AccessPoint {
            bssid: "AA:BB:CC:DD:EE:FF".to_owned(),
            ssid: Some("TestNet".to_owned()),
            channel,
            power: -42,
            encryption: "WPA2".to_owned(),
            clients: std::collections::HashMap::new(),
            beacon_count: 10,
            hidden: false,
            revealed: false,
        }
    }

    #[test]
    fn ap_update_preserves_channel_when_new_is_none() {
        let mut state = AppState::new();
        state.apply_event(&AppEvent::ApDiscovered(make_ap(Some(6))));
        assert_eq!(state.access_points["AA:BB:CC:DD:EE:FF"].channel, Some(6));

        state.apply_event(&AppEvent::ApUpdated(make_ap(None)));
        assert_eq!(state.access_points["AA:BB:CC:DD:EE:FF"].channel, Some(6),);
    }

    #[test]
    fn ap_update_overwrites_channel_when_new_is_some() {
        let mut state = AppState::new();
        state.apply_event(&AppEvent::ApDiscovered(make_ap(Some(6))));
        state.apply_event(&AppEvent::ApUpdated(make_ap(Some(11))));
        assert_eq!(state.access_points["AA:BB:CC:DD:EE:FF"].channel, Some(11),);
    }
}
