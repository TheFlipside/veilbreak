//! Airodump-ng subprocess management and CSV parser.
//!
//! Spawns `airodump-ng` with the appropriate flags, watches the live CSV
//! it produces, parses AP and client rows, and emits [`AppEvent`](crate::AppEvent)
//! variants through an `mpsc` channel consumed by the app event loop.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::error::AirodumpError;
use crate::event::AppEvent;
use crate::state::{AccessPoint, Client};
use crate::validate;

/// Parsed result from a single CSV read cycle.
#[derive(Debug, Default)]
pub struct CsvSnapshot {
    /// All access points parsed from the AP section.
    pub access_points: Vec<AccessPoint>,
    /// All clients parsed from the station section.
    pub clients: Vec<Client>,
}

/// Parses a complete airodump-ng CSV file into APs and clients.
///
/// The CSV has two sections separated by a blank line: APs first (header
/// starts with `BSSID`), then stations (header starts with `Station MAC`).
/// Handles mid-write truncation by ignoring incomplete trailing lines.
///
/// Note: airodump-ng does not quote ESSIDs containing commas, so such
/// SSIDs will be truncated at the first internal comma.
#[must_use]
pub fn parse_csv(input: &str) -> CsvSnapshot {
    let mut snapshot = CsvSnapshot::default();

    let mut sections = input.split("\n\n");
    if let Some(ap_section) = sections.next() {
        snapshot.access_points = parse_ap_section(ap_section);
    }
    if let Some(client_section) = sections.next() {
        snapshot.clients = parse_client_section(client_section);
    }

    snapshot
}

fn parse_ap_section(section: &str) -> Vec<AccessPoint> {
    let mut aps = Vec::new();

    for line in section.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.splitn(15, ',').collect();
        if fields.len() < 14 {
            continue;
        }

        let bssid = fields[0].trim();
        if !validate::is_valid_bssid(bssid) {
            tracing::warn!("invalid BSSID in airodump CSV, skipping: {bssid}");
            continue;
        }

        let channel = fields[3].trim().parse::<u32>().unwrap_or(0);
        let privacy = validate::sanitize_display_string(fields[5].trim());
        let power = fields[8].trim().parse::<i32>().unwrap_or(0);
        let beacons = fields[9].trim().parse::<u64>().unwrap_or(0);
        let id_length = fields[12].trim().parse::<u32>().unwrap_or(0);
        let essid = fields[13].trim();

        let essid = validate::truncate_utf8(essid, validate::MAX_ESSID_LEN);
        let hidden = id_length == 0 || essid.is_empty();
        let ssid = if hidden {
            None
        } else {
            Some(validate::sanitize_display_string(essid))
        };

        aps.push(AccessPoint {
            bssid: bssid.to_owned(),
            ssid,
            channel,
            power,
            encryption: privacy,
            clients: HashMap::new(),
            beacon_count: beacons,
            hidden,
        });
    }

    aps
}

fn parse_client_section(section: &str) -> Vec<Client> {
    let mut clients = Vec::new();

    for line in section.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.splitn(7, ',').collect();
        if fields.len() < 6 {
            continue;
        }

        let mac = fields[0].trim();
        if !validate::is_valid_bssid(mac) {
            tracing::warn!("invalid client MAC in airodump CSV, skipping: {mac}");
            continue;
        }

        let power = fields[3].trim().parse::<i32>().unwrap_or(0);
        let bssid = fields[5].trim();
        // "not associated" and similar non-BSSID values are expected; skip silently.
        if !validate::is_valid_bssid(bssid) {
            continue;
        }

        clients.push(Client {
            mac: mac.to_owned(),
            power,
            bssid: bssid.to_owned(),
        });
    }

    clients
}

/// State tracked across CSV poll cycles for diffing.
#[derive(Debug, Default)]
struct KnownState {
    aps: HashMap<String, AccessPoint>,
    clients: HashSet<(String, String)>,
}

/// Diffs a new CSV snapshot against current known state and emits events.
pub fn diff_and_emit<S: std::hash::BuildHasher, S2: std::hash::BuildHasher>(
    snapshot: &CsvSnapshot,
    known: &HashMap<String, AccessPoint, S>,
    known_clients: &HashSet<(String, String), S2>,
    tx: &mpsc::Sender<AppEvent>,
) {
    for ap in &snapshot.access_points {
        let event = if known.contains_key(&ap.bssid) {
            let existing = &known[&ap.bssid];
            if ap.power != existing.power
                || ap.beacon_count != existing.beacon_count
                || ap.channel != existing.channel
            {
                Some(AppEvent::ApUpdated(ap.clone()))
            } else {
                None
            }
        } else {
            Some(AppEvent::ApDiscovered(ap.clone()))
        };

        if let Some(ev) = event
            && tx.try_send(ev).is_err()
        {
            tracing::warn!("event channel full, dropping AP event");
        }
    }

    for client in &snapshot.clients {
        let key = (client.mac.clone(), client.bssid.clone());
        if known_clients.contains(&key) {
            continue;
        }
        if tx.try_send(AppEvent::ClientSeen(client.clone())).is_err() {
            tracing::warn!("event channel full, dropping client event");
        }
    }
}

/// Handle to a running airodump-ng subprocess and its CSV watcher.
///
/// The command channel for pause/resume is not yet wired — `stop()` and
/// `Drop` terminate the subprocess directly via the held `Child` handle,
/// avoiding PID reuse races.
pub struct AirodumpController {
    child: Arc<Mutex<tokio::process::Child>>,
    join_handle: JoinHandle<()>,
    csv_handle: JoinHandle<()>,
    pcap_path: PathBuf,
}

impl AirodumpController {
    /// Spawns airodump-ng in monitor mode and starts watching its CSV output.
    ///
    /// # Errors
    ///
    /// Returns an error if the subprocess cannot be spawned or the output
    /// directory fails validation.
    pub fn spawn(
        interface: &str,
        output_dir: &Path,
        tx: mpsc::Sender<AppEvent>,
    ) -> Result<Self, AirodumpError> {
        if !validate::is_valid_interface_name(interface) {
            return Err(AirodumpError::Spawn(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid interface name",
            )));
        }

        let canonical_dir = output_dir.canonicalize().map_err(AirodumpError::Spawn)?;
        if !canonical_dir.is_dir() {
            return Err(AirodumpError::Spawn(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "output_dir is not a directory",
            )));
        }

        let prefix = canonical_dir.join("veilbreak");

        let child = Command::new("airodump-ng")
            .arg("-w")
            .arg(&prefix)
            .arg("--output-format")
            .arg("pcap,csv")
            .arg(interface)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(AirodumpError::Spawn)?;

        let child = Arc::new(Mutex::new(child));

        let child_waiter = Arc::clone(&child);
        let join_handle = tokio::spawn(async move {
            let _ = child_waiter.lock().await.wait().await;
        });

        let file_stem = prefix.file_name().unwrap_or_default().to_os_string();

        let mut pcap_name = file_stem.clone();
        pcap_name.push("-01.cap");
        let mut pcap_path = prefix.clone();
        pcap_path.set_file_name(pcap_name);

        let mut csv_name = file_stem;
        csv_name.push("-01.csv");
        let mut csv_path = prefix;
        csv_path.set_file_name(csv_name);

        let csv_handle = tokio::spawn(csv_watch_loop(csv_path, tx));

        Ok(Self {
            child,
            join_handle,
            csv_handle,
            pcap_path,
        })
    }

    /// Path to the pcap capture file produced by airodump-ng.
    #[must_use]
    pub fn pcap_path(&self) -> &Path {
        &self.pcap_path
    }

    /// Stops the airodump-ng subprocess and CSV watcher.
    pub fn stop(self) {
        kill_child(&self.child);
        self.csv_handle.abort();
        self.join_handle.abort();
    }
}

impl Drop for AirodumpController {
    fn drop(&mut self) {
        kill_child(&self.child);
        self.csv_handle.abort();
        self.join_handle.abort();
    }
}

fn kill_child(child: &Arc<Mutex<tokio::process::Child>>) {
    let _ = child.blocking_lock().start_kill();
}

async fn csv_watch_loop(path: PathBuf, tx: mpsc::Sender<AppEvent>) {
    let mut known = KnownState::default();

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            continue;
        };

        let size = content.len() as u64;
        let snapshot = parse_csv(&content);
        diff_and_emit(&snapshot, &known.aps, &known.clients, &tx);

        let _ = tx.try_send(AppEvent::CaptureSize(size));

        for client in &snapshot.clients {
            known
                .clients
                .insert((client.mac.clone(), client.bssid.clone()));
        }
        for ap in snapshot.access_points {
            known.aps.insert(ap.bssid.clone(), ap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/airodump.csv");

    #[test]
    fn parses_all_access_points() {
        let snap = parse_csv(FIXTURE);
        assert_eq!(snap.access_points.len(), 5);
    }

    #[test]
    fn parses_ap_fields_correctly() {
        let snap = parse_csv(FIXTURE);
        let ap = &snap.access_points[0];
        assert_eq!(ap.bssid, "AA:BB:CC:00:11:20");
        assert_eq!(ap.ssid.as_deref(), Some("FIXTURE-NET-01"));
        assert_eq!(ap.channel, 6);
        assert_eq!(ap.power, -42);
        assert_eq!(ap.encryption, "WPA2");
        assert_eq!(ap.beacon_count, 127);
        assert!(!ap.hidden);
    }

    #[test]
    fn detects_hidden_ssids() {
        let snap = parse_csv(FIXTURE);
        let hidden: Vec<_> = snap.access_points.iter().filter(|ap| ap.hidden).collect();
        assert_eq!(hidden.len(), 2);
        assert_eq!(hidden[0].bssid, "77:88:99:AA:BB:CC");
        assert_eq!(hidden[1].bssid, "DE:AD:BE:EF:00:01");
        assert!(hidden[0].ssid.is_none());
        assert!(hidden[1].ssid.is_none());
    }

    #[test]
    fn parses_all_clients() {
        let snap = parse_csv(FIXTURE);
        assert_eq!(snap.clients.len(), 5);
    }

    #[test]
    fn parses_client_fields_correctly() {
        let snap = parse_csv(FIXTURE);
        let client = &snap.clients[0];
        assert_eq!(client.mac, "F0:11:22:33:44:55");
        assert_eq!(client.power, -48);
        assert_eq!(client.bssid, "AA:BB:CC:00:11:20");
    }

    #[test]
    fn associates_clients_to_correct_bssids() {
        let snap = parse_csv(FIXTURE);
        assert_eq!(
            snap.clients
                .iter()
                .filter(|c| c.bssid == "AA:BB:CC:00:11:20")
                .count(),
            3
        );
        assert_eq!(
            snap.clients
                .iter()
                .filter(|c| c.bssid == "DE:AD:BE:EF:00:01")
                .count(),
            2
        );
    }

    #[test]
    fn handles_empty_csv() {
        let snap = parse_csv("");
        assert!(snap.access_points.is_empty());
        assert!(snap.clients.is_empty());
    }

    #[test]
    fn handles_ap_only_csv() {
        let input = "\nBSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key\nAA:BB:CC:DD:EE:FF, 2025-01-15 10:00:01, 2025-01-15 10:05:32, 6, 54e, WPA2, CCMP, PSK, -42, 10, 0, 0.0.0.0, 7, TestNet,\n";
        let snap = parse_csv(input);
        assert_eq!(snap.access_points.len(), 1);
        assert!(snap.clients.is_empty());
    }

    #[test]
    fn rejects_invalid_bssid_in_csv() {
        let input = "\nBSSID, col2, col3, channel, Speed, Privacy, Cipher, Auth, Power, beacons, IV, IP, IDlen, ESSID, Key\nNOTVALID, 2025-01-15, 2025-01-15, 6, 54e, WPA2, CCMP, PSK, -42, 10, 0, 0.0.0.0, 7, Test,\n";
        let snap = parse_csv(input);
        assert!(snap.access_points.is_empty());
    }

    #[test]
    fn diff_emits_new_ap_as_discovered() {
        let snap = parse_csv(FIXTURE);
        let known = HashMap::new();
        let known_clients = HashSet::new();
        let (tx, mut rx) = mpsc::channel(64);

        diff_and_emit(&snap, &known, &known_clients, &tx);

        let mut discovered = 0;
        let mut client_count = 0;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                AppEvent::ApDiscovered(_) => discovered += 1,
                AppEvent::ClientSeen(_) => client_count += 1,
                _ => {}
            }
        }
        assert_eq!(discovered, 5);
        assert_eq!(client_count, 5);
    }

    #[test]
    fn diff_emits_updated_for_known_changed_aps() {
        let snap = parse_csv(FIXTURE);
        let mut known = HashMap::new();
        for ap in &snap.access_points {
            let mut stale = ap.clone();
            stale.power -= 10;
            known.insert(stale.bssid.clone(), stale);
        }

        let known_clients = HashSet::new();
        let (tx, mut rx) = mpsc::channel(64);
        diff_and_emit(&snap, &known, &known_clients, &tx);

        let mut updated = 0;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, AppEvent::ApUpdated(_)) {
                updated += 1;
            }
        }
        assert_eq!(updated, 5);
    }

    #[test]
    fn diff_skips_unchanged_aps() {
        let snap = parse_csv(FIXTURE);
        let mut known = HashMap::new();
        for ap in &snap.access_points {
            known.insert(ap.bssid.clone(), ap.clone());
        }

        let known_clients = HashSet::new();
        let (tx, mut rx) = mpsc::channel(64);
        diff_and_emit(&snap, &known, &known_clients, &tx);

        let mut ap_count = 0;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, AppEvent::ApDiscovered(_) | AppEvent::ApUpdated(_)) {
                ap_count += 1;
            }
        }
        assert_eq!(ap_count, 0);
    }

    #[test]
    fn diff_skips_known_clients() {
        let snap = parse_csv(FIXTURE);
        let known = HashMap::new();
        let mut known_clients = HashSet::new();
        for c in &snap.clients {
            known_clients.insert((c.mac.clone(), c.bssid.clone()));
        }

        let (tx, mut rx) = mpsc::channel(64);
        diff_and_emit(&snap, &known, &known_clients, &tx);

        let mut client_count = 0;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, AppEvent::ClientSeen(_)) {
                client_count += 1;
            }
        }
        assert_eq!(client_count, 0);
    }

    #[test]
    fn open_ap_encryption() {
        let snap = parse_csv(FIXTURE);
        let open = snap.access_points.iter().find(|ap| ap.encryption == "OPN");
        assert!(open.is_some());
        let open = open.unwrap();
        assert_eq!(open.bssid, "CA:FE:BA:BE:00:02");
        assert!(!open.hidden);
    }
}
