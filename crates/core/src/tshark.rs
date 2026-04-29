//! Tshark subprocess management and EK-JSON parser.
//!
//! Runs `tshark` against the pcap produced by airodump-ng with display
//! filters for hidden-SSID reveal frames. Parses `-T ek` line-delimited
//! JSON and emits [`SsidRevealed`](crate::AppEvent::SsidRevealed) events.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::error::TsharkError;
use crate::event::{AppEvent, RevealSource};
use crate::validate;

/// A parsed reveal packet from tshark EK-JSON output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealPacket {
    /// BSSID of the access point (normalized to uppercase).
    pub bssid: String,
    /// Revealed SSID (sanitized, truncated to 32 bytes).
    pub ssid: String,
    /// How the SSID was obtained.
    pub source: RevealSource,
}

/// Parses a single EK-JSON line from tshark `-T ek` output.
///
/// Returns `None` for index records, unrecognized frame subtypes, invalid
/// BSSIDs, empty SSIDs, or malformed JSON.
#[must_use]
pub fn parse_ek_json_line(line: &str) -> Option<RevealPacket> {
    let val: serde_json::Value = serde_json::from_str(line).ok()?;
    let layers = val.get("layers")?;
    let wlan = layers.get("wlan")?;

    let subtype_hex = wlan
        .get("wlan_wlan_fc_type_subtype")?
        .as_array()?
        .first()?
        .as_str()?;

    let source = match subtype_hex {
        "0x00000005" => RevealSource::ProbeResponse,
        "0x00000000" => RevealSource::AssociationRequest,
        "0x00000002" => RevealSource::ReassociationRequest,
        "0x00000008" => RevealSource::BeaconLeak,
        _ => return None,
    };

    let bssid_raw = wlan.get("wlan_wlan_bssid")?.as_array()?.first()?.as_str()?;

    let bssid = bssid_raw.to_uppercase();
    if !validate::is_valid_bssid(&bssid) {
        return None;
    }

    let ssid_raw = wlan.get("wlan_wlan_ssid")?.as_array()?.first()?.as_str()?;

    if ssid_raw.is_empty() {
        return None;
    }

    let ssid = validate::sanitize_display_string(validate::truncate_utf8(
        ssid_raw,
        validate::MAX_ESSID_LEN,
    ));

    if ssid.is_empty() {
        return None;
    }

    Some(RevealPacket {
        bssid,
        ssid,
        source,
    })
}

/// Builds a tshark display filter targeting reveal-capable frame subtypes
/// for the given BSSIDs.
///
/// Invalid BSSIDs are silently skipped as a defense-in-depth measure.
/// BSSIDs are lowercased to match tshark's native output format.
#[must_use]
pub fn build_display_filter(bssids: &[&str]) -> String {
    let subtype_filter = "(wlan.fc.type_subtype == 0x05 || wlan.fc.type_subtype == 0x00 || wlan.fc.type_subtype == 0x02 || wlan.fc.type_subtype == 0x08)";

    let bssid_parts: Vec<String> = bssids
        .iter()
        .filter(|b| validate::is_valid_bssid(b))
        .map(|b| format!("wlan.bssid == {}", b.to_lowercase()))
        .collect();

    match bssid_parts.len() {
        0 => subtype_filter.to_owned(),
        1 => format!("{subtype_filter} && {}", bssid_parts[0]),
        _ => {
            let bssid_or = bssid_parts.join(" || ");
            format!("{subtype_filter} && ({bssid_or})")
        }
    }
}

/// Handle to a running tshark poll task that watches for hidden-SSID reveals.
///
/// Periodically re-reads the pcap produced by airodump-ng, filtering for
/// frames that may contain hidden SSIDs. Deduplicates reveals so each BSSID
/// emits at most one [`SsidRevealed`](crate::AppEvent::SsidRevealed) event.
pub struct TsharkController {
    hidden_bssids: Arc<Mutex<Vec<String>>>,
    poll_handle: JoinHandle<()>,
}

impl TsharkController {
    /// Spawns a periodic tshark poll task against the given pcap file.
    ///
    /// The poll task sleeps 3 seconds between invocations and skips polls
    /// when no unrevealed hidden BSSIDs remain.
    #[must_use]
    pub fn spawn(pcap_path: PathBuf, tx: mpsc::Sender<AppEvent>) -> Self {
        let hidden_bssids = Arc::new(Mutex::new(Vec::<String>::new()));
        let revealed = Arc::new(Mutex::new(HashSet::<String>::new()));

        let poll_bssids = Arc::clone(&hidden_bssids);

        let poll_handle = tokio::spawn(poll_loop(pcap_path, poll_bssids, revealed, tx));

        Self {
            hidden_bssids,
            poll_handle,
        }
    }

    /// Updates the set of hidden BSSIDs to search for in subsequent polls.
    pub async fn set_hidden_bssids(&self, bssids: Vec<String>) {
        *self.hidden_bssids.lock().await = bssids;
    }

    /// Stops the poll task.
    pub fn stop(self) {
        self.poll_handle.abort();
    }
}

impl Drop for TsharkController {
    fn drop(&mut self) {
        self.poll_handle.abort();
    }
}

async fn poll_loop(
    pcap_path: PathBuf,
    hidden_bssids: Arc<Mutex<Vec<String>>>,
    revealed: Arc<Mutex<HashSet<String>>>,
    tx: mpsc::Sender<AppEvent>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;

        let unrevealed: Vec<String> = {
            let bssids = hidden_bssids.lock().await;
            let rev = revealed.lock().await;
            bssids
                .iter()
                .filter(|b| !rev.contains(b.as_str()))
                .cloned()
                .collect()
        };

        if unrevealed.is_empty() {
            continue;
        }

        let refs: Vec<&str> = unrevealed.iter().map(String::as_str).collect();
        let filter = build_display_filter(&refs);

        let output = match run_tshark(&pcap_path, &filter).await {
            Ok(out) => out,
            Err(e) => {
                tracing::debug!("tshark poll failed: {e}");
                continue;
            }
        };

        let mut newly_revealed = Vec::new();
        for line in output.lines() {
            if let Some(packet) = parse_ek_json_line(line) {
                let mut rev = revealed.lock().await;
                if rev.contains(&packet.bssid) {
                    continue;
                }
                rev.insert(packet.bssid.clone());
                drop(rev);
                newly_revealed.push(packet);
            }
        }

        for packet in newly_revealed {
            let event = AppEvent::SsidRevealed {
                bssid: packet.bssid,
                ssid: packet.ssid,
                source: packet.source,
            };
            if tx.try_send(event).is_err() {
                tracing::warn!("event channel full, dropping reveal event");
            }
        }
    }
}

const MAX_TSHARK_OUTPUT: usize = 10 * 1024 * 1024;

async fn run_tshark(pcap_path: &Path, filter: &str) -> Result<String, TsharkError> {
    let output = Command::new("tshark")
        .arg("-r")
        .arg(pcap_path)
        .arg("-Y")
        .arg(filter)
        .arg("-T")
        .arg("ek")
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(TsharkError::Spawn)?;

    if !output.status.success() {
        let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        stderr.truncate(512);
        let stderr = validate::sanitize_display_string(&stderr);
        return Err(TsharkError::Failed {
            status: output.status.code().unwrap_or(-1),
            stderr,
        });
    }

    if output.stdout.len() > MAX_TSHARK_OUTPUT {
        return Err(TsharkError::Parse(format!(
            "tshark output exceeds {MAX_TSHARK_OUTPUT} byte limit"
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/tshark_ek.jsonl");

    fn fixture_lines() -> Vec<&'static str> {
        FIXTURE.lines().collect()
    }

    #[test]
    fn index_record_skipped() {
        let lines = fixture_lines();
        assert!(parse_ek_json_line(lines[0]).is_none());
    }

    #[test]
    fn parses_probe_response() {
        let lines = fixture_lines();
        let packet = parse_ek_json_line(lines[1]).expect("should parse probe response");
        assert_eq!(packet.bssid, "77:88:99:AA:BB:CC");
        assert_eq!(packet.ssid, "HiddenNet-01");
        assert_eq!(packet.source, RevealSource::ProbeResponse);
    }

    #[test]
    fn parses_association_request() {
        let lines = fixture_lines();
        let packet = parse_ek_json_line(lines[2]).expect("should parse assoc request");
        assert_eq!(packet.bssid, "DE:AD:BE:EF:00:01");
        assert_eq!(packet.ssid, "SecretLab");
        assert_eq!(packet.source, RevealSource::AssociationRequest);
    }

    #[test]
    fn parses_reassociation_request() {
        let lines = fixture_lines();
        let packet = parse_ek_json_line(lines[3]).expect("should parse reassoc request");
        assert_eq!(packet.bssid, "DE:AD:BE:EF:00:01");
        assert_eq!(packet.ssid, "SecretLab");
        assert_eq!(packet.source, RevealSource::ReassociationRequest);
    }

    #[test]
    fn parses_beacon_leak() {
        let lines = fixture_lines();
        let packet = parse_ek_json_line(lines[4]).expect("should parse beacon leak");
        assert_eq!(packet.bssid, "AB:CD:EF:01:23:45");
        assert_eq!(packet.ssid, "LeakedBeacon");
        assert_eq!(packet.source, RevealSource::BeaconLeak);
    }

    #[test]
    fn rejects_empty_ssid() {
        let lines = fixture_lines();
        assert!(parse_ek_json_line(lines[5]).is_none());
    }

    #[test]
    fn rejects_invalid_bssid() {
        let lines = fixture_lines();
        assert!(parse_ek_json_line(lines[6]).is_none());
    }

    #[test]
    fn rejects_unknown_subtype() {
        let lines = fixture_lines();
        assert!(parse_ek_json_line(lines[7]).is_none());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_ek_json_line("not valid json {{{").is_none());
        assert!(parse_ek_json_line("").is_none());
    }

    #[test]
    fn sanitizes_control_chars() {
        let lines = fixture_lines();
        let packet = parse_ek_json_line(lines[8]).expect("should parse with sanitized SSID");
        assert_eq!(packet.ssid, "Evil[31mNet");
        assert_eq!(packet.bssid, "CC:DD:EE:FF:00:11");
    }

    #[test]
    fn truncates_long_ssid() {
        let long_ssid = "A".repeat(64);
        let line = format!(
            r#"{{"timestamp":"1","layers":{{"frame":{{}},"wlan":{{"wlan_wlan_fc_type_subtype":["0x00000005"],"wlan_wlan_bssid":["aa:bb:cc:dd:ee:ff"],"wlan_wlan_ssid":["{long_ssid}"]}}}}}}"#
        );
        let packet = parse_ek_json_line(&line).expect("should parse with truncated SSID");
        assert!(packet.ssid.len() <= validate::MAX_ESSID_LEN);
        assert_eq!(packet.ssid.len(), 32);
    }

    #[test]
    fn builds_filter_single_bssid() {
        let filter = build_display_filter(&["AA:BB:CC:DD:EE:FF"]);
        assert!(filter.contains("wlan.bssid == aa:bb:cc:dd:ee:ff"));
        assert!(!filter.contains("(wlan.bssid"));
    }

    #[test]
    fn builds_filter_multiple_bssids() {
        let filter = build_display_filter(&["AA:BB:CC:DD:EE:FF", "11:22:33:44:55:66"]);
        assert!(filter.contains("wlan.bssid == aa:bb:cc:dd:ee:ff"));
        assert!(filter.contains("wlan.bssid == 11:22:33:44:55:66"));
        assert!(filter.contains("(wlan.bssid"));
    }

    #[test]
    fn fixture_parse_count() {
        let count = fixture_lines()
            .iter()
            .filter_map(|line| parse_ek_json_line(line))
            .count();
        assert_eq!(count, 5);
    }
}
