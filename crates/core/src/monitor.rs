//! Monitor mode management for dedicated wireless interfaces.
//!
//! [`MonitorGuard`] puts an interface into monitor mode and restores it to
//! managed mode on drop. The [`Drop`] impl uses blocking I/O
//! ([`std::process::Command`]) because it may fire during panic unwind or
//! tokio runtime shutdown, where async spawns are unreliable.

use tokio::process::Command;

use crate::error::MonitorError;
use crate::validate;

const MAX_STDERR_LEN: usize = 512;

const SEARCH_DIRS: &[&str] = &["/usr/sbin/", "/sbin/", "/usr/bin/", "/bin/"];

/// Resolves a binary name to an absolute path by checking standard directories.
///
/// Returns an error if the binary is not found — never falls back to
/// bare-name PATH resolution, which would be unsafe under root.
///
/// # Trust assumption
///
/// The search directories are root-owned system paths. We assume they are
/// not writable by unprivileged users; if they were, the entire system
/// would already be compromised (any binary in those dirs could be replaced).
fn resolve_binary(name: &str) -> Result<String, MonitorError> {
    for dir in SEARCH_DIRS {
        let mut path = String::with_capacity(dir.len() + name.len());
        path.push_str(dir);
        path.push_str(name);
        if std::path::Path::new(&path).exists() {
            return Ok(path);
        }
    }
    Err(MonitorError::InvalidArgument(format!(
        "{name} not found in {SEARCH_DIRS:?}"
    )))
}

/// RAII guard that holds an interface in monitor mode.
///
/// Created via [`MonitorGuard::enter`], which brings the interface down, sets
/// monitor mode, and brings it back up. On [`Drop`], the reverse sequence
/// restores managed mode using blocking I/O.
#[derive(Debug)]
pub struct MonitorGuard {
    interface: String,
    ip_bin: String,
    iw_bin: String,
    active: bool,
}

impl MonitorGuard {
    /// Puts `interface` into monitor mode.
    ///
    /// Runs `ip link set <iface> down`, `iw dev <iface> set type monitor`,
    /// `ip link set <iface> up` in sequence. If any step after link-down
    /// fails, a best-effort rollback to managed mode is attempted before
    /// returning the error.
    ///
    /// # Errors
    ///
    /// Returns [`MonitorError`] if the interface name is invalid or any of
    /// the three subprocess commands fail.
    #[allow(clippy::similar_names)] // ip_bin and iw_bin are the actual tool names
    pub async fn enter(interface: &str) -> Result<Self, MonitorError> {
        if !validate::is_valid_interface_name(interface) {
            return Err(MonitorError::InvalidArgument(format!(
                "invalid interface name: {interface}"
            )));
        }

        let iface = interface.to_owned();
        let ip_bin = resolve_binary("ip")?;
        let iw_bin = resolve_binary("iw")?;

        link_down(&ip_bin, &iface).await?;

        if let Err(e) = set_type_monitor(&iw_bin, &iface).await {
            if let Err(rb) = rollback_managed(&ip_bin, &iw_bin, &iface).await {
                tracing::warn!("rollback after failed monitor enter on {iface}: {rb}");
            }
            return Err(e);
        }

        if let Err(e) = link_up(&ip_bin, &iface).await {
            if let Err(rb) = rollback_managed(&ip_bin, &iw_bin, &iface).await {
                tracing::warn!("rollback after failed link-up on {iface}: {rb}");
            }
            return Err(e);
        }

        tracing::info!("entered monitor mode on {iface}");

        Ok(Self {
            interface: iface,
            ip_bin,
            iw_bin,
            active: true,
        })
    }

    /// Returns the name of the monitored interface.
    #[must_use]
    pub fn interface(&self) -> &str {
        &self.interface
    }
}

impl Drop for MonitorGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;

        if !self.ip_bin.starts_with('/') || !self.iw_bin.starts_with('/') {
            tracing::error!(
                "refusing to restore {}: binary paths are not absolute",
                self.interface,
            );
            return;
        }

        tracing::info!("restoring {} to managed mode", self.interface);

        let run = |prog: &str, args: &[&str]| match std::process::Command::new(prog)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let raw = String::from_utf8_lossy(&out.stderr);
                let msg = validate::sanitize_display_string(validate::truncate_utf8(
                    &raw,
                    MAX_STDERR_LEN,
                ));
                tracing::warn!(
                    "failed to restore {}: {} {:?} — {msg}",
                    self.interface,
                    prog,
                    args,
                );
            }
            Err(e) => {
                tracing::warn!(
                    "failed to run {} {:?} for {}: {e}",
                    prog,
                    args,
                    self.interface,
                );
            }
        };

        run(&self.ip_bin, &["link", "set", &self.interface, "down"]);
        run(
            &self.iw_bin,
            &["dev", &self.interface, "set", "type", "managed"],
        );
        run(&self.ip_bin, &["link", "set", &self.interface, "up"]);
    }
}

async fn link_down(ip_bin: &str, interface: &str) -> Result<(), MonitorError> {
    run_cmd(ip_bin, &["link", "set", interface, "down"], |stderr| {
        MonitorError::LinkDown {
            interface: interface.to_owned(),
            stderr,
        }
    })
    .await
}

async fn set_type_monitor(iw_bin: &str, interface: &str) -> Result<(), MonitorError> {
    run_cmd(
        iw_bin,
        &["dev", interface, "set", "type", "monitor"],
        |stderr| MonitorError::SetMonitor {
            interface: interface.to_owned(),
            stderr,
        },
    )
    .await
}

async fn link_up(ip_bin: &str, interface: &str) -> Result<(), MonitorError> {
    run_cmd(ip_bin, &["link", "set", interface, "up"], |stderr| {
        MonitorError::LinkUp {
            interface: interface.to_owned(),
            stderr,
        }
    })
    .await
}

#[allow(clippy::similar_names)] // ip_bin and iw_bin are the actual tool names
async fn rollback_managed(ip_bin: &str, iw_bin: &str, interface: &str) -> Result<(), MonitorError> {
    if let Err(e) = link_down(ip_bin, interface).await {
        tracing::warn!("rollback link-down failed on {interface}: {e}");
        return Err(e);
    }
    run_cmd(
        iw_bin,
        &["dev", interface, "set", "type", "managed"],
        |stderr| MonitorError::SetManaged {
            interface: interface.to_owned(),
            stderr,
        },
    )
    .await?;
    link_up(ip_bin, interface).await
}

async fn run_cmd<F>(prog: &str, args: &[&str], make_err: F) -> Result<(), MonitorError>
where
    F: FnOnce(String) -> MonitorError,
{
    let output = match Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return Err(make_err(validate::sanitize_display_string(&e.to_string())));
        }
    };

    if !output.status.success() {
        let raw = String::from_utf8_lossy(&output.stderr);
        let stderr =
            validate::sanitize_display_string(validate::truncate_utf8(&raw, MAX_STDERR_LEN));
        return Err(make_err(stderr));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_interface_name() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(MonitorGuard::enter("rm -rf /"));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            MonitorError::InvalidArgument(_)
        ));
    }
}
