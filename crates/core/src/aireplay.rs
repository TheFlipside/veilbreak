//! Aireplay-ng deauth job management.
//!
//! Spawns `aireplay-ng --deauth` for broadcast or targeted deauthentication.
//! Captures output and emits [`DeauthComplete`](crate::AppEvent::DeauthComplete)
//! on exit. Each invocation is fire-and-forget — aireplay exits after sending
//! the requested number of frames.

use tokio::process::Command;
use tokio::sync::mpsc;

use crate::error::AireplayError;
use crate::event::AppEvent;
use crate::validate;

/// Default number of deauth frames to send per invocation.
pub const DEFAULT_DEAUTH_COUNT: u32 = 5;

const MIN_DEAUTH_COUNT: u32 = 1;
const MAX_DEAUTH_COUNT: u32 = 128;
const MAX_STDERR_LEN: usize = 512;
/// Sentinel for processes killed by a signal (no exit code available).
const UNKNOWN_EXIT_STATUS: i32 = -1;
/// Upper bound for valid Wi-Fi channel numbers.
const MAX_CHANNEL: u32 = 200;

/// Specifies the target for a deauthentication attack.
#[derive(Debug, Clone)]
pub enum DeauthTarget {
    /// Broadcast deauth — hits all clients of the AP.
    Broadcast {
        /// BSSID of the target access point.
        bssid: String,
        /// Operating channel of the target AP.
        channel: u32,
    },
    /// Targeted deauth — hits a single client.
    Targeted {
        /// BSSID of the target access point.
        bssid: String,
        /// MAC address of the target client.
        client: String,
        /// Operating channel of the target AP.
        channel: u32,
    },
}

impl DeauthTarget {
    /// Returns the BSSID of the target access point.
    #[must_use]
    pub fn bssid(&self) -> &str {
        match self {
            Self::Broadcast { bssid, .. } | Self::Targeted { bssid, .. } => bssid,
        }
    }

    /// Returns the operating channel of the target AP.
    #[must_use]
    pub const fn channel(&self) -> u32 {
        match self {
            Self::Broadcast { channel, .. } | Self::Targeted { channel, .. } => *channel,
        }
    }
}

const fn clamp_frame_count(count: u32) -> u32 {
    if count < MIN_DEAUTH_COUNT {
        MIN_DEAUTH_COUNT
    } else if count > MAX_DEAUTH_COUNT {
        MAX_DEAUTH_COUNT
    } else {
        count
    }
}

/// Sets the wireless interface to the given channel via `iw`.
///
/// This is necessary before running `aireplay-ng` because it has no
/// channel flag of its own — it relies on the interface already being
/// tuned to the target AP's channel.
async fn set_channel(interface: &str, channel: u32) -> Result<(), AireplayError> {
    let output = Command::new("iw")
        .arg("dev")
        .arg(interface)
        .arg("set")
        .arg("channel")
        .arg(channel.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| AireplayError::ChannelSet {
            channel,
            interface: interface.to_owned(),
            stderr: validate::sanitize_display_string(&e.to_string()),
        })?;

    if !output.status.success() {
        let raw = String::from_utf8_lossy(&output.stderr);
        let stderr =
            validate::sanitize_display_string(validate::truncate_utf8(&raw, MAX_STDERR_LEN));
        return Err(AireplayError::ChannelSet {
            channel,
            interface: interface.to_owned(),
            stderr,
        });
    }

    Ok(())
}

/// Spawns an `aireplay-ng --deauth` process and waits for completion.
///
/// Validates all arguments at the command boundary as a defense-in-depth
/// measure. On success, sends [`AppEvent::DeauthComplete`] through `tx`.
/// On failure, sends [`AppEvent::Error`] and returns the underlying error.
///
/// # Errors
///
/// Returns [`AireplayError::InvalidArgument`] if a BSSID, client MAC, or
/// interface name fails validation. Returns [`AireplayError::Spawn`] if the
/// process cannot be started, or [`AireplayError::Failed`] if it exits with
/// a non-zero status.
pub async fn run_deauth(
    target: &DeauthTarget,
    interface: &str,
    frame_count: u32,
    tx: &mpsc::Sender<AppEvent>,
) -> Result<(), AireplayError> {
    if !validate::is_valid_interface_name(interface) {
        return Err(AireplayError::InvalidArgument(format!(
            "invalid interface name: {interface}"
        )));
    }

    let bssid = target.bssid();
    if !validate::is_valid_bssid(bssid) {
        return Err(AireplayError::InvalidArgument(format!(
            "invalid BSSID: {bssid}"
        )));
    }

    if let DeauthTarget::Targeted { client, .. } = target
        && !validate::is_valid_bssid(client)
    {
        return Err(AireplayError::InvalidArgument(format!(
            "invalid client MAC: {client}"
        )));
    }

    let channel = target.channel();
    if channel == 0 || channel > MAX_CHANNEL {
        return Err(AireplayError::InvalidArgument(format!(
            "invalid channel: {channel}"
        )));
    }

    let count = clamp_frame_count(frame_count);
    if count != frame_count {
        tracing::warn!("deauth frame count clamped from {frame_count} to {count}");
    }

    set_channel(interface, channel).await?;

    let mut cmd = Command::new("aireplay-ng");
    cmd.arg("--deauth")
        .arg(count.to_string())
        .arg("-a")
        .arg(bssid);

    if let DeauthTarget::Targeted { client, .. } = target {
        cmd.arg("-c").arg(client);
    }

    cmd.arg(interface)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = match cmd.output().await {
        Ok(out) => out,
        Err(e) => {
            let msg = format!("failed to spawn aireplay-ng: {e}");
            if tx.try_send(AppEvent::Error(msg)).is_err() {
                tracing::warn!("event channel full, dropping aireplay spawn error event");
            }
            return Err(AireplayError::Spawn(e));
        }
    };

    if !output.status.success() {
        let raw_stderr = String::from_utf8_lossy(&output.stderr);
        let raw_stdout = String::from_utf8_lossy(&output.stdout);
        let combined = if raw_stdout.trim().is_empty() {
            raw_stderr
        } else {
            raw_stdout
        };
        let diag =
            validate::sanitize_display_string(validate::truncate_utf8(&combined, MAX_STDERR_LEN));
        let status = output.status.code().unwrap_or(UNKNOWN_EXIT_STATUS);
        let msg = format!("aireplay-ng failed (status {status}): {diag}");
        if tx.try_send(AppEvent::Error(msg)).is_err() {
            tracing::warn!("event channel full, dropping aireplay error event");
        }
        return Err(AireplayError::Failed {
            status,
            output: diag,
        });
    }

    if tx
        .try_send(AppEvent::DeauthComplete {
            bssid: bssid.to_owned(),
            frames_sent: count,
        })
        .is_err()
    {
        tracing::warn!("event channel full, dropping deauth complete event");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_bounds() {
        assert_eq!(clamp_frame_count(0), 1);
        assert_eq!(clamp_frame_count(1), 1);
        assert_eq!(clamp_frame_count(5), 5);
        assert_eq!(clamp_frame_count(128), 128);
        assert_eq!(clamp_frame_count(200), 128);
    }

    #[test]
    fn broadcast_bssid_accessor() {
        let target = DeauthTarget::Broadcast {
            bssid: "AA:BB:CC:DD:EE:FF".into(),
            channel: 6,
        };
        assert_eq!(target.bssid(), "AA:BB:CC:DD:EE:FF");
        assert_eq!(target.channel(), 6);
    }

    #[test]
    fn targeted_bssid_accessor() {
        let target = DeauthTarget::Targeted {
            bssid: "AA:BB:CC:DD:EE:FF".into(),
            client: "11:22:33:44:55:66".into(),
            channel: 36,
        };
        assert_eq!(target.bssid(), "AA:BB:CC:DD:EE:FF");
        assert_eq!(target.channel(), 36);
    }

    #[tokio::test]
    async fn rejects_invalid_bssid() {
        let (tx, _rx) = mpsc::channel(8);
        let target = DeauthTarget::Broadcast {
            bssid: "INVALID".into(),
            channel: 6,
        };
        let result = run_deauth(&target, "wlan0mon", 5, &tx).await;
        assert!(matches!(result, Err(AireplayError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn rejects_invalid_client_mac() {
        let (tx, _rx) = mpsc::channel(8);
        let target = DeauthTarget::Targeted {
            bssid: "AA:BB:CC:DD:EE:FF".into(),
            client: "BAD-MAC".into(),
            channel: 6,
        };
        let result = run_deauth(&target, "wlan0mon", 5, &tx).await;
        assert!(matches!(result, Err(AireplayError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn rejects_invalid_interface() {
        let (tx, _rx) = mpsc::channel(8);
        let target = DeauthTarget::Broadcast {
            bssid: "AA:BB:CC:DD:EE:FF".into(),
            channel: 6,
        };
        let result = run_deauth(&target, "rm -rf /", 5, &tx).await;
        assert!(matches!(result, Err(AireplayError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn rejects_invalid_channel() {
        let (tx, _rx) = mpsc::channel(8);
        let target = DeauthTarget::Broadcast {
            bssid: "AA:BB:CC:DD:EE:FF".into(),
            channel: 0,
        };
        let result = run_deauth(&target, "wlan0mon", 5, &tx).await;
        assert!(matches!(result, Err(AireplayError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn channel_set_failure_returns_error() {
        let (tx, _rx) = mpsc::channel(8);
        let target = DeauthTarget::Broadcast {
            bssid: "AA:BB:CC:DD:EE:FF".into(),
            channel: 6,
        };
        let result = run_deauth(&target, "wlan0mon", 5, &tx).await;
        assert!(matches!(result, Err(AireplayError::ChannelSet { .. })));
    }
}
