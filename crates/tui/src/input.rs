//! Key-to-action mapping for each screen.

use std::cmp::Reverse;

use crossterm::event::KeyCode;
use veilbreak_core::AppState;
use veilbreak_core::aireplay::DeauthTarget;

use crate::app::{DashboardState, FocusPane, Modal, Screen, SetupScreen};

/// What the app loop should do after processing a key.
#[must_use]
pub enum Outcome {
    /// Keep running.
    Continue,
    /// Exit the application.
    Quit,
    /// Launch a deauth job with the given target.
    Deauth(DeauthTarget),
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
        Screen::Loading => match key {
            KeyCode::Char('q') | KeyCode::Esc => return Outcome::Quit,
            _ => return Outcome::Continue,
        },
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
        SetupScreen::ModeConfirm { interface, .. } => match key {
            KeyCode::Enter => {
                let dash = DashboardState {
                    interface_name: Some(interface.name.clone()),
                    ..DashboardState::default()
                };
                SetupOutcome::Transition(Screen::Dashboard(dash))
            }
            KeyCode::Char('q') | KeyCode::Esc => SetupOutcome::Quit,
            _ => SetupOutcome::Continue,
        },
    }
}

fn handle_deauth_modal_key(dash: &mut DashboardState, key: KeyCode) -> Outcome {
    let Some(Modal::Deauth(modal)) = &mut dash.modal else {
        return Outcome::Continue;
    };

    match key {
        KeyCode::Esc | KeyCode::Char('q') => {
            dash.modal = None;
            Outcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            modal.selected = modal.selected.saturating_sub(1);
            Outcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            modal.selected = (modal.selected + 1).min(modal.clients.len());
            Outcome::Continue
        }
        KeyCode::Enter => {
            let target = match modal.selected {
                0 => DeauthTarget::Broadcast {
                    bssid: modal.bssid.clone(),
                },
                n => {
                    if let Some((client_mac, _)) = modal.clients.get(n - 1) {
                        DeauthTarget::Targeted {
                            bssid: modal.bssid.clone(),
                            client: client_mac.clone(),
                        }
                    } else {
                        dash.modal = None;
                        return Outcome::Continue;
                    }
                }
            };
            dash.modal = None;
            Outcome::Deauth(target)
        }
        _ => Outcome::Continue,
    }
}

const FILTER_ROW_COUNT: usize = 2;

fn handle_filter_modal_key(dash: &mut DashboardState, key: KeyCode) -> Outcome {
    let Some(Modal::Filter { selected }) = &mut dash.modal else {
        return Outcome::Continue;
    };

    match key {
        KeyCode::Esc | KeyCode::Char('q' | 'f') => {
            dash.modal = None;
            Outcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            *selected = selected.saturating_sub(1);
            Outcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            *selected = (*selected + 1).min(FILTER_ROW_COUNT - 1);
            Outcome::Continue
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let row = *selected;
            match row {
                0 => dash.filter.hidden_only = !dash.filter.hidden_only,
                _ => dash.filter.band = dash.filter.band.next(),
            }
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}

fn handle_help_key(dash: &mut DashboardState, key: KeyCode) -> Outcome {
    match key {
        KeyCode::Esc | KeyCode::Char('q' | '?') => {
            dash.modal = None;
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}

fn handle_dashboard_key(dash: &mut DashboardState, state: &AppState, key: KeyCode) -> Outcome {
    if let Some(modal) = &dash.modal {
        return match modal {
            Modal::Deauth(_) => handle_deauth_modal_key(dash, key),
            Modal::Filter { .. } => handle_filter_modal_key(dash, key),
            Modal::Help => handle_help_key(dash, key),
        };
    }

    let ap_count = state.access_points.len();

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
            match dash.focus {
                FocusPane::ApList => {
                    dash.selected_ap = dash.selected_ap.saturating_sub(1);
                }
                FocusPane::EventLog => {
                    dash.event_scroll = dash.event_scroll.saturating_sub(1);
                }
                FocusPane::Detail => {}
            }
            Outcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            match dash.focus {
                FocusPane::ApList => {
                    let max = ap_count.saturating_sub(1);
                    dash.selected_ap = dash.selected_ap.saturating_add(1).min(max);
                }
                FocusPane::EventLog => {
                    dash.event_scroll = dash.event_scroll.saturating_add(1);
                }
                FocusPane::Detail => {}
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
                dash.selected_ap = ap_count.saturating_sub(1);
            }
            Outcome::Continue
        }
        KeyCode::Char('s') => {
            if dash.focus == FocusPane::ApList {
                dash.sort = dash.sort.next();
            }
            Outcome::Continue
        }
        KeyCode::Char('d') => {
            if dash.interface_name.is_some()
                && (dash.focus == FocusPane::ApList || dash.focus == FocusPane::Detail)
            {
                let sorted = state.sorted_aps(dash.sort);
                if let Some(&(_, ap)) = sorted.get(dash.selected_ap) {
                    let mut clients: Vec<(String, i32)> = ap
                        .clients
                        .values()
                        .map(|c| (c.mac.clone(), c.power))
                        .collect();
                    clients.sort_by_key(|&(_, power)| Reverse(power));
                    dash.modal = Some(Modal::Deauth(crate::app::DeauthModal {
                        bssid: ap.bssid.clone(),
                        clients,
                        selected: 0,
                    }));
                }
            }
            Outcome::Continue
        }
        KeyCode::Char('f') => {
            dash.modal = Some(Modal::Filter { selected: 0 });
            Outcome::Continue
        }
        KeyCode::Char('?') => {
            dash.modal = Some(Modal::Help);
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}
