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

/// Wi-Fi frequency band for airodump-ng's `--band` flag.
///
/// Some drivers (notably mt76x2u) silently fall back to 2.4 GHz when
/// `--band abg` is requested. Explicit single-band selection avoids this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Band {
    /// 2.4 GHz only (`--band bg`).
    #[default]
    Bg,
    /// 5 GHz only (`--band a`).
    A,
    /// Both bands (`--band abg`). Unreliable on some drivers.
    Abg,
}

impl Band {
    /// Returns the argument string passed to `airodump-ng --band`.
    #[must_use]
    pub const fn as_arg(self) -> &'static str {
        match self {
            Self::Bg => "bg",
            Self::A => "a",
            Self::Abg => "abg",
        }
    }

    /// Human-readable label for display.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bg => "2.4 GHz",
            Self::A => "5 GHz",
            Self::Abg => "Both (abg)",
        }
    }

    /// Cycle to the next band value.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Bg => Self::A,
            Self::A => Self::Abg,
            Self::Abg => Self::Bg,
        }
    }

    /// Cycle to the previous band value.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Bg => Self::Abg,
            Self::A => Self::Bg,
            Self::Abg => Self::A,
        }
    }
}

impl std::fmt::Display for Band {
    /// Formats as the CLI argument form (`bg`, `a`, `abg`), not the human label.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_arg())
    }
}

impl std::str::FromStr for Band {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bg" => Ok(Self::Bg),
            "a" => Ok(Self::A),
            "abg" => Ok(Self::Abg),
            _ => Err(format!("invalid band '{s}': expected bg, a, or abg")),
        }
    }
}

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

    // airodump-ng uses \r\n line endings regardless of platform.
    let normalized = input.replace('\r', "");
    let mut sections = normalized.split("\n\n");
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
            tracing::warn!(
                "invalid BSSID in airodump CSV, skipping: {}",
                validate::sanitize_display_string(bssid),
            );
            continue;
        }

        let ch_raw = fields[3].trim();
        let channel = ch_raw.parse::<u32>().ok().filter(|&ch| {
            if validate::is_valid_channel(ch) {
                true
            } else {
                tracing::debug!("invalid channel {:?} for BSSID {bssid}", ch_raw);
                false
            }
        });
        let privacy = validate::sanitize_display_string(fields[5].trim());
        let pwr_raw = fields[8].trim();
        let power = pwr_raw.parse::<i32>().unwrap_or_else(|_| {
            tracing::debug!("malformed power {:?} for BSSID {bssid}", pwr_raw);
            0
        });
        let bcn_raw = fields[9].trim();
        let beacons = bcn_raw.parse::<u64>().unwrap_or_else(|_| {
            tracing::debug!("malformed beacon count {:?} for BSSID {bssid}", bcn_raw);
            0
        });
        let idl_raw = fields[12].trim();
        let id_length = idl_raw.parse::<u32>().unwrap_or_else(|_| {
            tracing::debug!("malformed ID-length {:?} for BSSID {bssid}", idl_raw);
            0
        });
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
            // `revealed` is managed by AppState; the parser always produces false.
            revealed: false,
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
            tracing::warn!(
                "invalid client MAC in airodump CSV, skipping: {}",
                validate::sanitize_display_string(mac),
            );
            continue;
        }

        let pwr_raw = fields[3].trim();
        let power = pwr_raw.parse::<i32>().unwrap_or_else(|_| {
            tracing::debug!("malformed power {:?} for client {mac}", pwr_raw);
            0
        });
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
                || ap.ssid != existing.ssid
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
/// `Drop` terminates the subprocess via the `Child` handle when
/// uncontended, falling back to `libc::kill` by stored PID when
/// the waiter task holds the lock (which is safe because contention
/// means the child is still alive inside `wait()`).
pub struct AirodumpController {
    child: Arc<Mutex<tokio::process::Child>>,
    child_pid: u32,
    join_handle: JoinHandle<()>,
    csv_handle: JoinHandle<()>,
    channel_handle: JoinHandle<()>,
    pcap_path: PathBuf,
    output_dir: PathBuf,
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
        band: Band,
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
        clean_stale_outputs(&canonical_dir)?;
        write_pid_file(&canonical_dir);

        tracing::debug!(
            "spawning: airodump-ng -w {} --output-format pcap,csv --band {} {}",
            prefix.display(),
            band.as_arg(),
            interface,
        );

        let mut child = Command::new("airodump-ng")
            .arg("-w")
            .arg(&prefix)
            .arg("--output-format")
            .arg("pcap,csv")
            .arg("--band")
            .arg(band.as_arg())
            .arg(interface)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(AirodumpError::Spawn)?;

        let child_pid = child.id().ok_or_else(|| {
            AirodumpError::Spawn(std::io::Error::other(
                "airodump-ng exited immediately after spawn",
            ))
        })?;

        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));

        let child_waiter = Arc::clone(&child);
        let stderr_task = stderr.map(|s| tokio::spawn(drain_stderr(s)));
        let join_handle = tokio::spawn(async move {
            let _ = child_waiter.lock().await.wait().await;
            if let Some(t) = stderr_task {
                let _ = t.await;
            }
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

        let csv_handle = tokio::spawn(csv_watch_loop(csv_path, tx.clone()));
        let channel_handle = tokio::spawn(channel_watch_loop(interface.to_owned(), tx));

        Ok(Self {
            child,
            child_pid,
            join_handle,
            csv_handle,
            channel_handle,
            pcap_path,
            output_dir: canonical_dir,
        })
    }

    /// Path to the pcap capture file produced by airodump-ng.
    #[must_use]
    pub fn pcap_path(&self) -> &Path {
        &self.pcap_path
    }
}

impl Drop for AirodumpController {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.try_lock() {
            let _ = child.start_kill();
        } else {
            // Waiter holds the lock across wait(), so the child is still alive.
            // SIGKILL by PID is safe here — no reuse risk because the child
            // has not been reaped yet.
            #[allow(clippy::cast_possible_wrap)]
            unsafe {
                libc::kill(self.child_pid as i32, libc::SIGKILL);
            }
        }
        self.channel_handle.abort();
        self.csv_handle.abort();
        self.join_handle.abort();
        remove_pid_file(&self.output_dir);
    }
}

const PID_FILE_NAME: &str = "veilbreak.pid";

/// Removes stale `veilbreak-NN.csv` / `.cap` files from a previous session so
/// that airodump-ng starts its counter at `-01` and our hardcoded path matches.
///
/// Skips cleanup if a PID file indicates another session is still active in
/// this directory.
fn clean_stale_outputs(dir: &Path) -> Result<(), AirodumpError> {
    let pid_path = dir.join(PID_FILE_NAME);
    if let Ok(contents) = std::fs::read_to_string(&pid_path)
        && let Ok(pid) = contents.trim().parse::<u32>()
        && is_pid_alive(pid)
    {
        return Err(AirodumpError::Spawn(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "another veilbreak session (PID {pid}) is active in {}",
                dir.display()
            ),
        )));
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        let ext_matches = Path::new(name_str)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("csv") || ext.eq_ignore_ascii_case("cap"));
        if name_str.starts_with("veilbreak-")
            && ext_matches
            && let Err(e) = std::fs::remove_file(entry.path())
        {
            tracing::warn!("failed to remove stale output {name_str}: {e}");
        }
    }
    Ok(())
}

fn write_pid_file(dir: &Path) {
    let pid_path = dir.join(PID_FILE_NAME);
    if let Err(e) = std::fs::write(&pid_path, format!("{}\n", std::process::id())) {
        tracing::warn!("failed to write PID file: {e}");
    }
}

fn remove_pid_file(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(PID_FILE_NAME));
}

fn is_pid_alive(pid: u32) -> bool {
    #[allow(clippy::cast_possible_wrap)]
    // SAFETY: kill(pid, 0) is a standard existence check — sends no signal.
    unsafe {
        libc::kill(pid as i32, 0) == 0
    }
}

async fn drain_stderr(stderr: tokio::process::ChildStderr) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut lines = BufReader::new(stderr).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => tracing::debug!(
                target: "airodump_stderr",
                "{}",
                validate::sanitize_display_string(&line),
            ),
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("airodump stderr read error: {e}");
                break;
            }
        }
    }
}

/// Parses the current channel from `iw dev <iface> info` output.
///
/// Looks for a line matching `channel <N> (<freq> MHz)` and returns `N`.
#[must_use]
pub fn parse_iw_channel(output: &str) -> Option<u32> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("channel ")
            && let Some(ch_str) = rest.split_whitespace().next()
        {
            return ch_str
                .parse()
                .ok()
                .filter(|&ch| validate::is_valid_channel(ch));
        }
    }
    None
}

async fn channel_watch_loop(interface: String, tx: mpsc::Sender<AppEvent>) {
    let mut last_ch: Option<u32> = None;

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let Ok(output) = Command::new("iw")
            .arg("dev")
            .arg(&interface)
            .arg("info")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .await
        else {
            continue;
        };

        if !output.status.success() {
            continue;
        }

        let Ok(stdout) = String::from_utf8(output.stdout) else {
            continue;
        };

        if let Some(ch) = parse_iw_channel(&stdout)
            && last_ch != Some(ch)
        {
            last_ch = Some(ch);
            let _ = tx.try_send(AppEvent::ChannelChanged(ch));
        }
    }
}

async fn csv_watch_loop(path: PathBuf, tx: mpsc::Sender<AppEvent>) {
    let mut known = KnownState::default();

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            continue;
        };

        // usize fits in u64 on all supported targets; Err arm is unreachable.
        let size = u64::try_from(content.len()).unwrap_or(0);
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
        assert_eq!(ap.channel, Some(6));
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

    #[test]
    fn parses_crlf_line_endings() {
        let crlf = FIXTURE.replace('\n', "\r\n");
        let snap = parse_csv(&crlf);
        assert_eq!(snap.access_points.len(), 5);
        assert_eq!(snap.clients.len(), 5);
        assert_eq!(snap.clients[0].bssid, "AA:BB:CC:00:11:20");
    }

    #[test]
    fn invalid_channel_becomes_none() {
        let input = "\nBSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key\nAA:BB:CC:DD:EE:FF, 2025-01-15, 2025-01-15, 0, 54e, WPA2, CCMP, PSK, -42, 10, 0, 0.0.0.0, 7, TestNet,\n";
        let snap = parse_csv(input);
        assert_eq!(snap.access_points.len(), 1);
        assert_eq!(snap.access_points[0].channel, None);
    }

    #[test]
    fn out_of_range_channel_becomes_none() {
        let input = "\nBSSID, First time seen, Last time seen, channel, Speed, Privacy, Cipher, Authentication, Power, # beacons, # IV, LAN IP, ID-length, ESSID, Key\nAA:BB:CC:DD:EE:FF, 2025-01-15, 2025-01-15, 999, 54e, WPA2, CCMP, PSK, -42, 10, 0, 0.0.0.0, 7, TestNet,\n";
        let snap = parse_csv(input);
        assert_eq!(snap.access_points.len(), 1);
        assert_eq!(snap.access_points[0].channel, None);
    }

    const IW_DEV_FIXTURE: &str = include_str!("../../../tests/fixtures/iw_dev.txt");

    #[test]
    fn parses_channel_from_iw_dev_output() {
        assert_eq!(parse_iw_channel(IW_DEV_FIXTURE), Some(6));
    }

    #[test]
    fn parse_iw_channel_no_channel_line() {
        let output = "Interface wlan0\n\tifindex 3\n\twdev 0x1\n\ttype monitor\n";
        assert_eq!(parse_iw_channel(output), None);
    }

    #[test]
    fn parse_iw_channel_different_channel() {
        let output = "\t\tchannel 149 (5745 MHz), width: 80 MHz, center1: 5775 MHz\n";
        assert_eq!(parse_iw_channel(output), Some(149));
    }
}
