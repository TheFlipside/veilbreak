//! Main application loop and screen state management.

use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::TableState;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use veilbreak_core::aireplay;
use veilbreak_core::airodump::AirodumpController;
use veilbreak_core::interface::WirelessInterface;
use veilbreak_core::persist;
use veilbreak_core::tshark::TsharkController;
use veilbreak_core::{AppEvent, AppState, Band, SortColumn};

use crate::input;
use crate::ui;

/// Which screen is currently displayed.
#[derive(Debug)]
pub enum Screen {
    /// Detecting wireless interfaces.
    Loading,
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
        /// Pre-selected band from CLI flag; skips the band prompt when `Some`.
        cli_band: Option<Band>,
    },
    /// User picks the Wi-Fi band for scanning.
    BandSelect {
        /// The chosen interface from the previous step.
        interface: WirelessInterface,
        /// Whether a second monitor-capable card was detected.
        dual_card: bool,
        /// Currently highlighted band.
        selected: Band,
    },
    /// Confirm the monitoring mode before proceeding.
    ModeConfirm {
        /// The chosen interface.
        interface: WirelessInterface,
        /// Whether a second monitor-capable card was detected.
        dual_card: bool,
        /// The selected Wi-Fi band.
        band: Band,
    },
}

/// Dashboard UI state (focus, selection, scroll).
#[derive(Debug, Default)]
pub struct DashboardState {
    /// Which pane currently has keyboard focus.
    pub focus: FocusPane,
    /// Current sort column for the AP list.
    pub sort: SortColumn,
    /// Name of the monitoring interface, if active.
    pub interface_name: Option<String>,
    /// Current channel being monitored, if known.
    pub channel: Option<u32>,
    /// Scroll offset in the event log.
    pub event_scroll: usize,
    /// Wi-Fi band passed to airodump-ng.
    pub band: Band,
    /// Active modal overlay, if any.
    pub modal: Option<Modal>,
    /// Active AP list filters.
    pub filter: FilterState,
    /// Sole source of truth for AP selection index and table viewport offset.
    pub table_state: TableState,
}

impl DashboardState {
    /// Returns the currently selected AP index (0 if nothing is selected).
    pub fn selected_ap(&self) -> usize {
        self.table_state.selected().unwrap_or(0)
    }

    /// Sets the selected AP index.
    pub const fn select_ap(&mut self, index: usize) {
        self.table_state.select(Some(index));
    }
}

/// State for the deauth target selection modal.
#[derive(Debug)]
pub struct DeauthModal {
    /// BSSID of the target AP.
    pub bssid: String,
    /// Clients associated with the target AP, sorted by signal strength (strongest first).
    pub clients: Vec<(String, i32)>,
    /// Selected index: 0 = broadcast, 1..N = targeted client at `clients[N-1]`.
    pub selected: usize,
}

/// Active modal overlay variant.
#[derive(Debug)]
pub enum Modal {
    /// Deauth target picker.
    Deauth(DeauthModal),
    /// AP list filter toggles.
    Filter {
        /// Selected row: 0 = hidden-only, 1 = band.
        selected: usize,
    },
    /// Full keybind reference overlay.
    Help,
}

/// Persistent filter state for the AP list.
#[derive(Debug, Default)]
pub struct FilterState {
    /// Show only hidden (unrevealed) APs.
    pub hidden_only: bool,
    /// Band filter.
    pub band: BandFilter,
}

impl FilterState {
    /// Returns `true` if the access point passes all active filters.
    #[must_use]
    pub const fn matches(&self, ap: &veilbreak_core::state::AccessPoint) -> bool {
        if self.hidden_only && !ap.hidden {
            return false;
        }
        match self.band {
            BandFilter::All => true,
            BandFilter::TwoFour => ap.channel > 0 && ap.channel <= 14,
            BandFilter::Five => ap.channel >= 36,
        }
    }
}

/// Band filter for the AP list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BandFilter {
    /// Show all bands.
    #[default]
    All,
    /// 2.4 GHz only (channels 1–14).
    TwoFour,
    /// 5 GHz only (channels 36+).
    Five,
}

impl BandFilter {
    /// Cycle to the next band filter value.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::All => Self::TwoFour,
            Self::TwoFour => Self::Five,
            Self::Five => Self::All,
        }
    }

    /// Human-readable label for the current filter.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::TwoFour => "2.4 GHz",
            Self::Five => "5 GHz",
        }
    }
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
    band: Option<Band>,
    output_dir: &Path,
) -> Result<()> {
    let mut state = AppState::default();
    // Channel capacity sized for burst headroom during rapid CSV re-parses.
    let (tx, mut rx) = mpsc::channel::<AppEvent>(256);

    let mut airodump_ctrl: Option<AirodumpController> = None;
    let mut tshark_ctrl: Option<TsharkController> = None;
    let mut session_spawn_attempted = false;
    let mut deauth_guard = DeauthGuard::default();
    let mut reveal_log = open_reveal_log(output_dir);

    let (mut screen, mut detect_task) = initial_screen_and_task(replay.as_deref(), band);
    let mut tick: u8 = 0;

    loop {
        deauth_guard.prune();
        terminal.draw(|frame| ui::draw(frame, &mut screen, &state, tick))?;

        if let Some(new_screen) = resolve_detect_task(&mut detect_task).await {
            screen = new_screen;
            continue;
        }

        tokio::select! {
            Some(event) = rx.recv() => {
                state.apply_event(&event);
                apply_event_to_log(&mut state, &event);
                persist_reveal(&mut reveal_log, &state, &event);
                let mut hidden_set_changed = changes_hidden_set(&event);
                // Drain up to a bounded number of queued events per frame
                // to prevent starving the terminal input arm.
                for _ in 0..64 {
                    match rx.try_recv() {
                        Ok(ev) => {
                            hidden_set_changed |= changes_hidden_set(&ev);
                            state.apply_event(&ev);
                            apply_event_to_log(&mut state, &ev);
                            persist_reveal(&mut reveal_log, &state, &ev);
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
                            input::Outcome::Deauth(target) => {
                                if let Screen::Dashboard(dash) = &screen
                                    && let Some(iface) = dash.interface_name.clone()
                                {
                                    dispatch_deauth(
                                        target,
                                        iface,
                                        &tx,
                                        &mut state,
                                        &mut deauth_guard,
                                    );
                                }
                            }
                        }
                    }
                }
                if !session_spawn_attempted
                    && let Screen::Dashboard(dash) = &screen
                    && let Some(iface_name) = &dash.interface_name
                {
                    session_spawn_attempted = true;
                    try_spawn_session(
                        iface_name, dash.band, output_dir, tx.clone(),
                        &mut state, &mut airodump_ctrl, &mut tshark_ctrl,
                    );
                }
            }
            // Wake the loop to check detect_task completion while loading.
            () = tokio::time::sleep(Duration::from_millis(100)), if detect_task.is_some() => {
                tick = tick.wrapping_add(1);
            }
        }
    }
}

async fn resolve_detect_task(task: &mut Option<DetectGuard>) -> Option<Screen> {
    let guard = task.as_ref()?;
    if !guard.is_finished() {
        return None;
    }
    let handle = task.take().and_then(DetectGuard::into_handle)?;
    Some(handle.await.unwrap_or_else(|e| {
        tracing::error!("interface detection task failed: {e}");
        Screen::Dashboard(DashboardState::default())
    }))
}

fn initial_screen_and_task(
    replay: Option<&str>,
    cli_band: Option<Band>,
) -> (Screen, Option<DetectGuard>) {
    if replay.is_some() {
        (Screen::Dashboard(DashboardState::default()), None)
    } else {
        let handle = tokio::spawn(detect_initial_screen(cli_band));
        (Screen::Loading, Some(DetectGuard(Some(handle))))
    }
}

/// Aborts the interface detection task on drop so it never outlives the app.
struct DetectGuard(Option<JoinHandle<Screen>>);

impl DetectGuard {
    fn is_finished(&self) -> bool {
        self.0.as_ref().is_some_and(JoinHandle::is_finished)
    }

    fn into_handle(mut self) -> Option<JoinHandle<Screen>> {
        self.0.take()
    }
}

impl Drop for DetectGuard {
    fn drop(&mut self) {
        if let Some(h) = &self.0 {
            h.abort();
        }
    }
}

async fn detect_initial_screen(cli_band: Option<Band>) -> Screen {
    match veilbreak_core::interface::detect_interfaces().await {
        Ok(ifaces) if !ifaces.is_empty() => Screen::Setup(SetupScreen::InterfaceSelect {
            interfaces: ifaces,
            selected: 0,
            cli_band,
        }),
        Ok(_empty) => Screen::Dashboard(DashboardState::default()),
        Err(e) => {
            tracing::warn!("interface detection failed, skipping setup: {e}");
            Screen::Dashboard(DashboardState::default())
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

fn try_spawn_session(
    interface: &str,
    band: Band,
    output_dir: &Path,
    tx: mpsc::Sender<AppEvent>,
    state: &mut AppState,
    airodump_ctrl: &mut Option<AirodumpController>,
    tshark_ctrl: &mut Option<TsharkController>,
) {
    match spawn_session(interface, band, output_dir, tx) {
        Ok((ac, tc)) => {
            state.log_event(format!("started capture on {interface} ({band})"));
            *airodump_ctrl = Some(ac);
            *tshark_ctrl = Some(tc);
        }
        Err(e) => {
            tracing::error!("failed to start capture session: {e}");
            state.log_event(format!("error: {e}"));
        }
    }
}

fn spawn_session(
    interface: &str,
    band: Band,
    output_dir: &Path,
    tx: mpsc::Sender<AppEvent>,
) -> Result<(AirodumpController, TsharkController)> {
    let airodump = AirodumpController::spawn(interface, output_dir, band, tx.clone())?;
    let pcap = airodump.pcap_path().to_owned();
    let tshark = TsharkController::spawn(pcap, tx);
    Ok((airodump, tshark))
}

const MAX_CONCURRENT_DEAUTHS: usize = 8;

fn dispatch_deauth(
    target: aireplay::DeauthTarget,
    interface: String,
    tx: &mpsc::Sender<AppEvent>,
    state: &mut AppState,
    guard: &mut DeauthGuard,
) {
    if guard.len() >= MAX_CONCURRENT_DEAUTHS {
        state.log_event("too many active deauths, try again later".to_owned());
        return;
    }
    state.log_event(format!("deauth started \u{2192} {}", target.bssid()));
    let tx_deauth = tx.clone();
    guard.push(tokio::spawn(async move {
        if let Err(e) = aireplay::run_deauth(
            &target,
            &interface,
            aireplay::DEFAULT_DEAUTH_COUNT,
            &tx_deauth,
        )
        .await
        {
            tracing::error!("deauth failed: {e}");
        }
    }));
}

/// Owns deauth `JoinHandle`s and aborts them all on drop.
#[derive(Default)]
struct DeauthGuard(Vec<JoinHandle<()>>);

impl DeauthGuard {
    fn prune(&mut self) {
        self.0.retain(|h| !h.is_finished());
    }

    fn push(&mut self, handle: JoinHandle<()>) {
        self.0.push(handle);
    }

    const fn len(&self) -> usize {
        self.0.len()
    }
}

impl Drop for DeauthGuard {
    fn drop(&mut self) {
        for h in &self.0 {
            h.abort();
        }
    }
}

fn open_reveal_log(output_dir: &Path) -> Option<std::fs::File> {
    let path = output_dir.join("revealed.jsonl");
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
    {
        Ok(f) => Some(f),
        Err(e) => {
            tracing::warn!("failed to open reveal log at {}: {e}", path.display());
            None
        }
    }
}

fn persist_reveal(file: &mut Option<std::fs::File>, state: &AppState, event: &AppEvent) {
    use veilbreak_core::validate::{self, sanitize_display_string};

    let AppEvent::SsidRevealed {
        bssid,
        ssid,
        source,
    } = event
    else {
        return;
    };
    if !validate::is_valid_bssid(bssid) {
        return;
    }
    let Some(f) = file.as_mut() else {
        return;
    };
    let safe_ssid = sanitize_display_string(ssid);
    let safe_ssid = validate::truncate_utf8(&safe_ssid, validate::MAX_ESSID_LEN);
    let record = persist::RevealRecord {
        elapsed_secs: state.elapsed_secs(),
        bssid,
        ssid: safe_ssid,
        source: &source.to_string(),
    };
    if let Err(e) = persist::write_reveal_entry(f, &record) {
        tracing::warn!("failed to write reveal log entry: {e}");
    }
}

const fn changes_hidden_set(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::ApDiscovered(_) | AppEvent::SsidRevealed { .. }
    )
}

fn apply_event_to_log(state: &mut AppState, event: &AppEvent) {
    use veilbreak_core::validate::{self, sanitize_display_string};

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
            let safe_ssid = validate::truncate_utf8(&safe_ssid, validate::MAX_ESSID_LEN);
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
