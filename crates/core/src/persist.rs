//! Persistence helpers for session data (revealed SSIDs, etc.).
//!
//! Writes NDJSON records to files in the session output directory.
//! Serialization lives here because `serde_json` is a `core` dependency
//! but not a `tui` dependency.

use std::io::Write;

use serde::Serialize;

use crate::error::PersistError;

/// A single revealed-SSID record written to `revealed.jsonl`.
#[derive(Debug, Serialize)]
pub struct RevealRecord<'a> {
    /// Seconds since session start.
    pub elapsed_secs: u64,
    /// BSSID of the access point.
    pub bssid: &'a str,
    /// The revealed SSID.
    pub ssid: &'a str,
    /// How the SSID was obtained (e.g. `"probe-response"`).
    pub source: &'a str,
}

/// Appends a single NDJSON line to the given file.
///
/// # Errors
///
/// Returns [`PersistError::Serialize`] if JSON serialization fails, or
/// [`PersistError::Io`] if the write or flush fails.
pub fn write_reveal_entry(
    file: &mut std::fs::File,
    record: &RevealRecord<'_>,
) -> Result<(), PersistError> {
    serde_json::to_writer(&mut *file, record)?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_file() -> (std::fs::File, std::path::PathBuf) {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("veilbreak-test-{}-{n}", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        (file, path)
    }

    #[test]
    fn writes_ndjson_line() {
        let (mut file, path) = temp_file();
        let record = RevealRecord {
            elapsed_secs: 42,
            bssid: "AA:BB:CC:DD:EE:FF",
            ssid: "TestNet",
            source: "probe-response",
        };
        write_reveal_entry(&mut file, &record).unwrap();
        drop(file);

        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert!(contents.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(parsed["bssid"], "AA:BB:CC:DD:EE:FF");
        assert_eq!(parsed["ssid"], "TestNet");
        assert_eq!(parsed["elapsed_secs"], 42);
        assert_eq!(parsed["source"], "probe-response");
    }

    #[test]
    fn appends_multiple_lines() {
        let (mut file, path) = temp_file();
        for i in 0..3 {
            let record = RevealRecord {
                elapsed_secs: i,
                bssid: "11:22:33:44:55:66",
                ssid: "Net",
                source: "beacon-leak",
            };
            write_reveal_entry(&mut file, &record).unwrap();
        }
        drop(file);

        let mut contents = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(contents.trim().lines().count(), 3);
    }
}
