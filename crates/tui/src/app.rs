//! Main application loop and screen state management.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::prelude::*;
use veilbreak_core::AppState;
use veilbreak_core::interface::WirelessInterface;

use crate::input;
use crate::ui;

/// Which screen is currently displayed.
pub enum Screen {
    /// Pre-session setup flow.
    Setup(SetupScreen),
    /// Main operational dashboard.
    Dashboard(DashboardState),
}

/// Steps in the pre-session setup flow.
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
) -> Result<()> {
    let state = AppState::default();

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

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match input::handle_key(&mut screen, &state, key.code) {
                input::Outcome::Continue => {}
                input::Outcome::Quit => break,
            }
        }
    }

    Ok(())
}
