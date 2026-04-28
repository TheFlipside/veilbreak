//! Application events that flow through the central event channel.
//!
//! All background producers (airodump parser, tshark parser, key handler)
//! emit [`AppEvent`] variants into a single `mpsc` channel consumed by the
//! app event loop.

use crate::state::{AccessPoint, Client};

/// How an SSID was revealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevealSource {
    /// Probe response from the AP.
    ProbeResponse,
    /// Association request from a client.
    AssociationRequest,
    /// Reassociation request from a client.
    ReassociationRequest,
    /// Post-association beacon leak.
    BeaconLeak,
}

impl std::fmt::Display for RevealSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProbeResponse => f.write_str("probe-response"),
            Self::AssociationRequest => f.write_str("association-request"),
            Self::ReassociationRequest => f.write_str("reassociation-request"),
            Self::BeaconLeak => f.write_str("beacon-leak"),
        }
    }
}

/// Events produced by background tasks and consumed by the app event loop.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// A new access point was discovered.
    ApDiscovered(AccessPoint),
    /// An existing access point's data was updated.
    ApUpdated(AccessPoint),
    /// A client was seen associated to a BSSID.
    ClientSeen(Client),
    /// A hidden SSID was revealed.
    SsidRevealed {
        /// BSSID of the access point.
        bssid: String,
        /// The revealed SSID.
        ssid: String,
        /// How the SSID was obtained.
        source: RevealSource,
    },
    /// A deauth job completed.
    DeauthComplete {
        /// Target BSSID.
        bssid: String,
        /// Number of deauth frames sent.
        frames_sent: u32,
    },
    /// Capture file size changed.
    CaptureSize(u64),
    /// An error occurred in a background task.
    Error(String),
}
