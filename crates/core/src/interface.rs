//! Wireless interface detection and monitor-mode capability checking.
//!
//! Parses output from `iw dev` and `iw list` to build a list of wireless
//! interfaces annotated with their hardware capabilities. Also provides
//! preflight checks for required external tools.

use std::collections::HashMap;
use std::path::PathBuf;

use tokio::process::Command;

use crate::error::InterfaceError;
use crate::validate;

/// Operating mode of a wireless interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceMode {
    /// Standard managed/station mode.
    Managed,
    /// Monitor mode for raw frame capture.
    Monitor,
    /// Access point mode.
    Ap,
    /// Any other mode reported by the kernel.
    Other(String),
}

impl InterfaceMode {
    fn from_iw_type(s: &str) -> Self {
        match s.trim() {
            "managed" => Self::Managed,
            "monitor" => Self::Monitor,
            "AP" => Self::Ap,
            other => Self::Other(validate::sanitize_display_string(other)),
        }
    }
}

impl std::fmt::Display for InterfaceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Managed => f.write_str("managed"),
            Self::Monitor => f.write_str("monitor"),
            Self::Ap => f.write_str("AP"),
            Self::Other(s) => f.write_str(s),
        }
    }
}

/// A wireless interface detected on the system.
#[derive(Debug, Clone)]
pub struct WirelessInterface {
    /// Interface name (e.g. `wlan0`).
    pub name: String,
    /// Physical device identifier (e.g. `phy0`).
    pub phy: String,
    /// MAC address of the interface (validated, empty if `iw` returned garbage).
    pub addr: String,
    /// Current operating mode.
    pub mode: InterfaceMode,
    /// Whether this interface's physical device supports monitor mode.
    pub monitor_capable: bool,
    /// Current channel, if reported.
    pub channel: Option<u32>,
}

/// Result of checking whether a required tool is on PATH.
#[derive(Debug, Clone)]
pub struct ToolCheck {
    /// Tool name (e.g. `airodump-ng`).
    pub name: String,
    /// Whether the tool was found.
    pub found: bool,
}

/// Detects all wireless interfaces and annotates them with capabilities.
///
/// Runs `iw dev` and `iw list` to gather interface and phy information,
/// then cross-references to determine which interfaces support monitor mode.
///
/// # Errors
///
/// Returns an error if `iw` cannot be executed or produces unexpected output.
pub async fn detect_interfaces() -> Result<Vec<WirelessInterface>, InterfaceError> {
    let dev_output = run_iw(&["dev"]).await?;
    let list_output = run_iw(&["list"]).await?;

    let mut interfaces = parse_iw_dev(&dev_output);
    let phy_caps = parse_iw_list(&list_output);

    for iface in &mut interfaces {
        iface.monitor_capable = phy_caps
            .get(&iface.phy)
            .is_some_and(|modes| modes.iter().any(|m| m == "monitor"));
    }

    Ok(interfaces)
}

/// Checks whether the required external tools are available on PATH.
///
/// Searches `$PATH` directories directly rather than spawning a subprocess.
#[must_use]
pub fn check_required_tools() -> Vec<ToolCheck> {
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();

    ["airodump-ng", "tshark", "aireplay-ng"]
        .iter()
        .map(|&tool| {
            let found = path_dirs.iter().any(|dir| dir.join(tool).is_file());
            ToolCheck {
                name: tool.to_owned(),
                found,
            }
        })
        .collect()
}

/// Returns `true` if the current process has effective root privileges (EUID 0).
#[must_use]
pub fn is_root() -> bool {
    // SAFETY: geteuid() is a read-only syscall with no preconditions.
    unsafe { libc::geteuid() == 0 }
}

async fn run_iw(args: &[&str]) -> Result<String, InterfaceError> {
    let output = Command::new("iw")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(InterfaceError::IwExecution)?;

    if !output.status.success() {
        let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        stderr.truncate(512);
        let stderr = validate::sanitize_display_string(&stderr);
        return Err(InterfaceError::IwFailed {
            // None when killed by signal; -1 is the conventional sentinel.
            status: output.status.code().unwrap_or(-1),
            stderr,
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_iw_dev(output: &str) -> Vec<WirelessInterface> {
    let mut interfaces = Vec::new();
    let mut current_phy = String::new();
    let mut current_iface: Option<WirelessInterface> = None;

    for line in output.lines() {
        if let Some(phy_num) = line.strip_prefix("phy#") {
            if let Some(iface) = current_iface.take() {
                push_if_valid(&mut interfaces, iface);
            }
            // `iw dev` uses "phy#N", `iw list` uses "Wiphy phyN" — we
            // normalise to "phyN" so the two can be cross-referenced.
            let candidate = format!("phy{}", phy_num.trim());
            if validate::is_valid_phy_name(&candidate) {
                current_phy = candidate;
            } else {
                tracing::warn!("invalid phy name from iw dev, ignoring: {candidate}");
                current_phy = String::new();
            }
        } else if let Some(rest) = line.strip_prefix("\tInterface ") {
            if let Some(iface) = current_iface.take() {
                push_if_valid(&mut interfaces, iface);
            }
            current_iface = Some(WirelessInterface {
                name: rest.trim().to_owned(),
                phy: current_phy.clone(),
                addr: String::new(),
                mode: InterfaceMode::Managed,
                monitor_capable: false,
                channel: None,
            });
        } else if let Some(iface) = current_iface.as_mut() {
            let trimmed = line.trim();
            if let Some(addr) = trimmed.strip_prefix("addr ") {
                let addr = addr.trim();
                if validate::is_valid_bssid(addr) {
                    addr.clone_into(&mut iface.addr);
                } else {
                    tracing::warn!("invalid MAC from iw dev, ignoring: {addr}");
                }
            } else if let Some(type_str) = trimmed.strip_prefix("type ") {
                iface.mode = InterfaceMode::from_iw_type(type_str);
            } else if let Some(ch_str) = trimmed.strip_prefix("channel ") {
                iface.channel = ch_str
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok());
            }
        }
    }

    if let Some(iface) = current_iface {
        push_if_valid(&mut interfaces, iface);
    }

    interfaces
}

fn push_if_valid(interfaces: &mut Vec<WirelessInterface>, iface: WirelessInterface) {
    if validate::is_valid_interface_name(&iface.name) && validate::is_valid_phy_name(&iface.phy) {
        interfaces.push(iface);
    }
}

fn parse_iw_list(output: &str) -> HashMap<String, Vec<String>> {
    let mut phy_modes: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_phy = String::new();
    let mut in_modes_section = false;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Wiphy ") {
            let candidate = rest.trim();
            if validate::is_valid_phy_name(candidate) {
                candidate.clone_into(&mut current_phy);
            } else {
                tracing::warn!("invalid phy name from iw list, ignoring: {candidate}");
                current_phy = String::new();
            }
            in_modes_section = false;
        } else if line.trim() == "Supported interface modes:" {
            in_modes_section = !current_phy.is_empty();
        } else if in_modes_section {
            if let Some(mode) = line.trim().strip_prefix("* ") {
                phy_modes
                    .entry(current_phy.clone())
                    .or_default()
                    .push(mode.trim().to_owned());
            } else {
                in_modes_section = false;
            }
        }
    }

    phy_modes
}

#[cfg(test)]
mod tests {
    use super::*;

    const IW_DEV_FIXTURE: &str = include_str!("../../../tests/fixtures/iw_dev.txt");
    const IW_LIST_FIXTURE: &str = include_str!("../../../tests/fixtures/iw_list.txt");

    #[test]
    fn parses_iw_dev_both_interfaces() {
        let interfaces = parse_iw_dev(IW_DEV_FIXTURE);
        assert_eq!(interfaces.len(), 2);

        assert_eq!(interfaces[0].name, "wlan0");
        assert_eq!(interfaces[0].phy, "phy0");
        assert_eq!(interfaces[0].addr, "aa:bb:cc:00:11:20");
        assert_eq!(interfaces[0].mode, InterfaceMode::Managed);
        assert_eq!(interfaces[0].channel, Some(6));

        assert_eq!(interfaces[1].name, "wlan1");
        assert_eq!(interfaces[1].phy, "phy1");
        assert_eq!(interfaces[1].addr, "aa:bb:cc:00:11:21");
        assert_eq!(interfaces[1].mode, InterfaceMode::Managed);
        assert_eq!(interfaces[1].channel, None);
    }

    #[test]
    fn parses_iw_list_monitor_support() {
        let phy_caps = parse_iw_list(IW_LIST_FIXTURE);

        let phy0_modes = &phy_caps["phy0"];
        assert!(phy0_modes.contains(&"monitor".to_owned()));
        assert!(phy0_modes.contains(&"managed".to_owned()));
        assert!(phy0_modes.contains(&"AP".to_owned()));

        let phy1_modes = &phy_caps["phy1"];
        assert!(!phy1_modes.contains(&"monitor".to_owned()));
        assert!(phy1_modes.contains(&"managed".to_owned()));
    }

    #[test]
    fn annotates_monitor_capability() {
        let mut interfaces = parse_iw_dev(IW_DEV_FIXTURE);
        let phy_caps = parse_iw_list(IW_LIST_FIXTURE);

        for iface in &mut interfaces {
            iface.monitor_capable = phy_caps
                .get(&iface.phy)
                .is_some_and(|modes| modes.iter().any(|m| m == "monitor"));
        }

        assert!(interfaces[0].monitor_capable);
        assert!(!interfaces[1].monitor_capable);
    }

    #[test]
    fn rejects_invalid_interface_names() {
        let output = "phy#0\n\tInterface rm -rf /\n\t\taddr aa:bb:cc:dd:ee:ff\n\t\ttype managed\n";
        let interfaces = parse_iw_dev(output);
        assert!(interfaces.is_empty());
    }

    #[test]
    fn rejects_invalid_mac_address() {
        let output = "phy#0\n\tInterface wlan0\n\t\taddr NOTAMAC\n\t\ttype managed\n";
        let interfaces = parse_iw_dev(output);
        assert_eq!(interfaces.len(), 1);
        assert!(interfaces[0].addr.is_empty());
    }

    #[test]
    fn handles_empty_iw_dev() {
        assert!(parse_iw_dev("").is_empty());
    }

    #[test]
    fn handles_empty_iw_list() {
        assert!(parse_iw_list("").is_empty());
    }

    #[test]
    fn parses_monitor_mode_interface() {
        let output = "phy#0\n\tInterface wlan0mon\n\t\taddr aa:bb:cc:dd:ee:ff\n\t\ttype monitor\n";
        let interfaces = parse_iw_dev(output);
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].mode, InterfaceMode::Monitor);
    }
}
