//! Main application loop and screen state management.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::prelude::*;
use tokio::sync::mpsc;
use veilbreak_core::airodump::AirodumpController;
use veilbreak_core::interface::WirelessInterface;
use veilbreak_core::tshark::TsharkController;
use veilbreak_core::{AppEvent, AppState, SortColumn};

use crate::input;
use crate::ui;

/// Which screen is currently displayed.
#[derive(Debug)]
pub enum Screen {
    /// Pre-session setup flow.
    Setup(SetupScreen),
    /// Main operational dashboard.
    Dashboard(DashboardState),
}

/// Steps in the pre-session setup flow.
#[derive(Debug)]
pub enum SetupScreen {
    /// User picks a monitoring interface.
    InterfaceSelect {
        /// Available wireless interfaces.
        interfaces: Vec<WirelessInterface>,
        /// Currently highlighted index.
        selected: usize,
    },
    /// Confirm the monitoring mode before proceeding.
    ModeConfirm {
        /// The chosen interface.
        interface: WirelessInterface,
        /// Whether a second monitor-capable card was detected.
        dual_card: bool,
    },
}

/// Dashboard UI state (focus, selection, scroll).
#[derive(Debug, Default)]
pub struct DashboardState {
    /// Which pane currently has keyboard focus.
    pub focus: FocusPane,
    /// Index of the selected AP in the list.
    pub selected_ap: usize,
    /// Current sort column for the AP list.
    pub sort: SortColumn,
    /// Name of the monitoring interface, if active.
    pub interface_name: Option<String>,
    /// Current channel being monitored, if known.
    pub channel: Option<u32>,
    /// Scroll offset in the event log.
    pub event_scroll: usize,
}

/// Which pane currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusPane {
    /// AP list (left).
    #[default]
    ApList,
    /// Detail pane (right).
    Detail,
    /// Event log (bottom).
    EventLog,
}

impl FocusPane {
    /// Cycle to the next pane.
    pub const fn next(self) -> Self {
        match self {
            Self::ApList => Self::Detail,
            Self::Detail => Self::EventLog,
            Self::EventLog => Self::ApList,
        }
    }

    /// Cycle to the previous pane.
    pub const fn prev(self) -> Self {
        match self {
            Self::ApList => Self::EventLog,
            Self::Detail => Self::ApList,
            Self::EventLog => Self::Detail,
        }
    }
}

/// Runs the main application loop.
///
/// # Errors
///
/// Returns an error if terminal I/O fails or an unrecoverable error occurs.
pub async fn run<B: Backend<Error: Send + Sync + 'static>>(
    terminal: &mut Terminal<B>,
    replay: Option<String>,
    output_dir: &Path,
) -> Result<()> {
    let mut state = AppState::default();
    // Channel capacity sized for burst headroom during rapid CSV re-parses.
    let (tx, mut rx) = mpsc::channel::<AppEvent>(256);

    #[allow(clippy::collection_is_never_read)] // held for Drop semantics
    let mut _airodump_ctrl: Option<AirodumpController> = None;
    let mut tshark_ctrl: Option<TsharkController> = None;
    let mut session_spawn_attempted = false;

    let mut screen = if replay.is_some() {
        Screen::Dashboard(DashboardState::default())
    } else {
        match veilbreak_core::interface::detect_interfaces().await {
            Ok(ifaces) if !ifaces.is_empty() => Screen::Setup(SetupScreen::InterfaceSelect {
                interfaces: ifaces,
                selected: 0,
            }),
            Ok(_empty) => Screen::Dashboard(DashboardState::default()),
            Err(e) => {
                tracing::warn!("interface detection failed, skipping setup: {e}");
                Screen::Dashboard(DashboardState::default())
            }
        }
    };

    loop {
        terminal.draw(|frame| ui::draw(frame, &screen, &state))?;

        tokio::select! {
            Some(event) = rx.recv() => {
                state.apply_event(&event);
                apply_event_to_log(&mut state, &event);
                let mut hidden_set_changed = changes_hidden_set(&event);
                // Drain up to a bounded number of queued events per frame
                // to prevent starving the terminal input arm.
                for _ in 0..64 {
                    match rx.try_recv() {
                        Ok(ev) => {
                            hidden_set_changed |= changes_hidden_set(&ev);
                            state.apply_event(&ev);
                            apply_event_to_log(&mut state, &ev);
                        }
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            tracing::error!("event channel closed unexpectedly");
                            return Err(anyhow::anyhow!("event channel closed"));
                        }
                    }
                }
                if hidden_set_changed
                    && let Some(tc) = &tshark_ctrl
                {
                    let hidden: Vec<String> = state
                        .access_points
                        .iter()
                        .filter(|(_, ap)| ap.hidden)
                        .map(|(bssid, _)| bssid.clone())
                        .collect();
                    tc.set_hidden_bssids(hidden).await;
                }
            }
            result = poll_terminal_event() => {
                result?;
                for _ in 0..64 {
                    if !matches!(event::poll(Duration::ZERO), Ok(true)) {
                        break;
                    }
                    if let Event::Key(key) = event::read()? {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }
                        match input::handle_key(&mut screen, &state, key.code) {
                            input::Outcome::Continue => {}
                            input::Outcome::Quit => return Ok(()),
                        }
                    }
                }
                if !session_spawn_attempted
                    && let Screen::Dashboard(dash) = &screen
                    && let Some(iface_name) = &dash.interface_name
                {
                    session_spawn_attempted = true;
                    match spawn_session(iface_name, output_dir, tx.clone()) {
                        Ok((ac, tc)) => {
                            state.log_event(format!("started capture on {iface_name}"));
                            _airodump_ctrl = Some(ac);
                            tshark_ctrl = Some(tc);
                        }
                        Err(e) => {
                            tracing::error!("failed to start capture session: {e}");
                            state.log_event(format!("error: {e}"));
                        }
                    }
                }
            }
        }
    }
}

async fn poll_terminal_event() -> Result<()> {
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if event::poll(Duration::ZERO)? {
            return Ok(());
        }
    }
}

fn spawn_session(
    interface: &str,
    output_dir: &Path,
    tx: mpsc::Sender<AppEvent>,
) -> Result<(AirodumpController, TsharkController)> {
    let airodump = AirodumpController::spawn(interface, output_dir, tx.clone())?;
    let pcap = airodump.pcap_path().to_owned();
    let tshark = TsharkController::spawn(pcap, tx);
    Ok((airodump, tshark))
}

const fn changes_hidden_set(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::ApDiscovered(_) | AppEvent::SsidRevealed { .. }
    )
}

fn apply_event_to_log(state: &mut AppState, event: &AppEvent) {
    use veilbreak_core::validate::sanitize_display_string;

    let msg = match event {
        AppEvent::ApDiscovered(ap) => {
            let tag = if ap.hidden { " hidden" } else { "" };
            format!("new AP {} ch{}{tag}", ap.bssid, ap.channel)
        }
        AppEvent::ApUpdated(_) | AppEvent::CaptureSize(_) => return,
        AppEvent::ClientSeen(client) => {
            format!("client {} associated to {}", client.mac, client.bssid)
        }
        AppEvent::SsidRevealed {
            bssid,
            ssid,
            source,
        } => {
            let safe_ssid = sanitize_display_string(ssid);
            format!("ssid revealed via {source}  {bssid} \u{2192} \"{safe_ssid}\"")
        }
        AppEvent::DeauthComplete { bssid, frames_sent } => {
            format!("deauth sent \u{2192} {bssid} ({frames_sent} frames)")
        }
        AppEvent::Error(msg) => {
            let safe_msg = sanitize_display_string(msg);
            format!("error: {safe_msg}")
        }
    };
    state.log_event(msg);
}
