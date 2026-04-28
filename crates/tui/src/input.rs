//! Key-to-action mapping for each screen.

use crossterm::event::KeyCode;
use veilbreak_core::AppState;

use crate::app::{DashboardState, FocusPane, Screen, SetupScreen};

/// What the app loop should do after processing a key.
pub enum Outcome {
    /// Keep running.
    Continue,
    /// Exit the application.
    Quit,
}

/// Internal outcome for setup screens that may trigger screen transitions.
enum SetupOutcome {
    Continue,
    Quit,
    Transition(Screen),
}

/// Processes a key press, mutating screen/state as needed.
pub fn handle_key(screen: &mut Screen, state: &AppState, key: KeyCode) -> Outcome {
    let transition = match screen {
        Screen::Setup(setup) => handle_setup_key(setup, key),
        Screen::Dashboard(dash) => return handle_dashboard_key(dash, state, key),
    };

    match transition {
        SetupOutcome::Continue => Outcome::Continue,
        SetupOutcome::Quit => Outcome::Quit,
        SetupOutcome::Transition(new_screen) => {
            *screen = new_screen;
            Outcome::Continue
        }
    }
}

fn handle_setup_key(setup: &mut SetupScreen, key: KeyCode) -> SetupOutcome {
    match setup {
        SetupScreen::InterfaceSelect {
            interfaces,
            selected,
        } => match key {
            KeyCode::Char('q') | KeyCode::Esc => SetupOutcome::Quit,
            KeyCode::Up | KeyCode::Char('k') => {
                *selected = selected.saturating_sub(1);
                SetupOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if *selected + 1 < interfaces.len() {
                    *selected += 1;
                }
                SetupOutcome::Continue
            }
            KeyCode::Enter => {
                let Some(iface) = interfaces.get(*selected).cloned() else {
                    return SetupOutcome::Continue;
                };
                let dual = interfaces.iter().filter(|i| i.monitor_capable).count() > 1;
                SetupOutcome::Transition(Screen::Setup(SetupScreen::ModeConfirm {
                    interface: iface,
                    dual_card: dual,
                }))
            }
            _ => SetupOutcome::Continue,
        },
        SetupScreen::ModeConfirm { .. } => match key {
            KeyCode::Enter => {
                SetupOutcome::Transition(Screen::Dashboard(DashboardState::default()))
            }
            KeyCode::Char('q') | KeyCode::Esc => SetupOutcome::Quit,
            _ => SetupOutcome::Continue,
        },
    }
}

fn handle_dashboard_key(dash: &mut DashboardState, state: &AppState, key: KeyCode) -> Outcome {
    match key {
        KeyCode::Char('q') => Outcome::Quit,
        KeyCode::Tab => {
            dash.focus = dash.focus.next();
            Outcome::Continue
        }
        KeyCode::BackTab => {
            dash.focus = dash.focus.prev();
            Outcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if dash.focus == FocusPane::ApList {
                dash.selected_ap = dash.selected_ap.saturating_sub(1);
            }
            Outcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if dash.focus == FocusPane::ApList {
                let max = state.access_points.len().saturating_sub(1);
                dash.selected_ap = dash.selected_ap.saturating_add(1).min(max);
            }
            Outcome::Continue
        }
        KeyCode::Char('g') => {
            if dash.focus == FocusPane::ApList {
                dash.selected_ap = 0;
            }
            Outcome::Continue
        }
        KeyCode::Char('G') => {
            if dash.focus == FocusPane::ApList {
                dash.selected_ap = state.access_points.len().saturating_sub(1);
            }
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}
