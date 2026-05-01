//! Key-to-action mapping for each screen.

use std::cmp::Reverse;

use crossterm::event::KeyCode;
use veilbreak_core::AppState;
use veilbreak_core::aireplay::DeauthTarget;

use veilbreak_core::Band;

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
    /// Post a message to the event log.
    Log(String),
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

#[allow(clippy::too_many_lines)] // one arm per setup step; splitting would obscure the state machine
fn handle_setup_key(setup: &mut SetupScreen, key: KeyCode) -> SetupOutcome {
    match setup {
        SetupScreen::InterfaceSelect {
            interfaces,
            selected,
            cli_band,
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
                let all = interfaces.clone();
                if let Some(band) = *cli_band {
                    SetupOutcome::Transition(Screen::Setup(SetupScreen::ModeConfirm {
                        interface: iface,
                        dual_card: dual,
                        band,
                        all_interfaces: all,
                    }))
                } else {
                    SetupOutcome::Transition(Screen::Setup(SetupScreen::BandSelect {
                        interface: iface,
                        dual_card: dual,
                        selected: Band::default(),
                        all_interfaces: all,
                    }))
                }
            }
            _ => SetupOutcome::Continue,
        },
        SetupScreen::BandSelect {
            interface,
            dual_card,
            selected,
            all_interfaces,
        } => match key {
            KeyCode::Char('q') | KeyCode::Esc => SetupOutcome::Quit,
            KeyCode::Up | KeyCode::Char('k') => {
                *selected = selected.prev();
                SetupOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = selected.next();
                SetupOutcome::Continue
            }
            KeyCode::Enter => SetupOutcome::Transition(Screen::Setup(SetupScreen::ModeConfirm {
                interface: interface.clone(),
                dual_card: *dual_card,
                band: *selected,
                all_interfaces: all_interfaces.clone(),
            })),
            _ => SetupOutcome::Continue,
        },
        SetupScreen::ModeConfirm {
            interface,
            band,
            dual_card,
            all_interfaces,
        } => match key {
            KeyCode::Enter => {
                if *dual_card {
                    let mut deauth_options: Vec<Option<_>> = vec![None];
                    for iface in all_interfaces {
                        if iface.name != interface.name && iface.monitor_capable {
                            deauth_options.push(Some(iface.clone()));
                        }
                    }
                    SetupOutcome::Transition(Screen::Setup(SetupScreen::DeauthCardSelect {
                        interface: interface.clone(),
                        band: *band,
                        deauth_options,
                        selected: 0,
                    }))
                } else {
                    let dash = DashboardState {
                        interface_name: Some(interface.name.clone()),
                        band: *band,
                        ..DashboardState::default()
                    };
                    SetupOutcome::Transition(Screen::Dashboard(dash))
                }
            }
            KeyCode::Char('q') | KeyCode::Esc => SetupOutcome::Quit,
            _ => SetupOutcome::Continue,
        },
        SetupScreen::DeauthCardSelect {
            interface,
            band,
            deauth_options,
            selected,
        } => match key {
            KeyCode::Char('q') | KeyCode::Esc => SetupOutcome::Quit,
            KeyCode::Up | KeyCode::Char('k') => {
                *selected = selected.saturating_sub(1);
                SetupOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if *selected + 1 < deauth_options.len() {
                    *selected += 1;
                }
                SetupOutcome::Continue
            }
            KeyCode::Enter => {
                let deauth_iface = deauth_options
                    .get(*selected)
                    .and_then(|opt| opt.as_ref())
                    .map(|iface| iface.name.clone());
                let dash = DashboardState {
                    interface_name: Some(interface.name.clone()),
                    band: *band,
                    deauth_interface: deauth_iface,
                    ..DashboardState::default()
                };
                SetupOutcome::Transition(Screen::Dashboard(dash))
            }
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
                    channel: modal.channel,
                },
                n => {
                    if let Some((client_mac, _)) = modal.clients.get(n - 1) {
                        DeauthTarget::Targeted {
                            bssid: modal.bssid.clone(),
                            client: client_mac.clone(),
                            channel: modal.channel,
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
const PAGE_SIZE: usize = 5;

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

fn scroll_up(dash: &mut DashboardState, step: usize) {
    match dash.focus {
        FocusPane::ApList => {
            dash.select_ap(dash.selected_ap().saturating_sub(step));
        }
        FocusPane::EventLog => {
            dash.event_scroll = dash.event_scroll.saturating_sub(step);
        }
        FocusPane::Detail => {}
    }
}

fn scroll_down(dash: &mut DashboardState, ap_count: usize, event_count: usize, step: usize) {
    match dash.focus {
        FocusPane::ApList => {
            let max = ap_count.saturating_sub(1);
            dash.select_ap(dash.selected_ap().saturating_add(step).min(max));
        }
        FocusPane::EventLog => {
            let max = event_count.saturating_sub(1);
            dash.event_scroll = dash.event_scroll.saturating_add(step).min(max);
        }
        FocusPane::Detail => {}
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

    let ap_count = state
        .access_points
        .values()
        .filter(|ap| dash.filter.matches(ap))
        .count();
    let event_count = state.event_log.len();

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
            scroll_up(dash, 1);
            Outcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            scroll_down(dash, ap_count, event_count, 1);
            Outcome::Continue
        }
        KeyCode::PageUp => {
            scroll_up(dash, PAGE_SIZE);
            Outcome::Continue
        }
        KeyCode::PageDown => {
            scroll_down(dash, ap_count, event_count, PAGE_SIZE);
            Outcome::Continue
        }
        KeyCode::Char('g') => {
            if dash.focus == FocusPane::ApList {
                dash.select_ap(0);
            }
            Outcome::Continue
        }
        KeyCode::Char('G') => {
            if dash.focus == FocusPane::ApList {
                dash.select_ap(ap_count.saturating_sub(1));
            }
            Outcome::Continue
        }
        KeyCode::Char('s') => {
            if dash.focus == FocusPane::ApList {
                dash.sort = dash.sort.next();
            }
            Outcome::Continue
        }
        KeyCode::Char('d') => open_deauth_modal(dash, state),
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

fn open_deauth_modal(dash: &mut DashboardState, state: &AppState) -> Outcome {
    if dash.interface_name.is_none() || !matches!(dash.focus, FocusPane::ApList | FocusPane::Detail)
    {
        return Outcome::Continue;
    }
    let ap = state
        .sorted_aps(dash.sort)
        .into_iter()
        .filter(|(_, ap)| dash.filter.matches(ap))
        .nth(dash.selected_ap());
    let Some((_, ap)) = ap else {
        return Outcome::Continue;
    };
    let Some(channel) = ap.channel else {
        return Outcome::Log(format!("cannot deauth {}: channel unknown", ap.bssid));
    };
    let mut clients: Vec<(String, i32)> = ap
        .clients
        .values()
        .map(|c| (c.mac.clone(), c.power))
        .collect();
    clients.sort_by_key(|&(_, power)| Reverse(power));
    dash.modal = Some(Modal::Deauth(crate::app::DeauthModal {
        bssid: ap.bssid.clone(),
        channel,
        clients,
        selected: 0,
    }));
    Outcome::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use veilbreak_core::state::AccessPoint;

    fn make_state_with_ap(channel: Option<u32>) -> AppState {
        let mut state = AppState::new();
        state.apply_event(&veilbreak_core::AppEvent::ApDiscovered(AccessPoint {
            bssid: "AA:BB:CC:DD:EE:FF".to_owned(),
            ssid: Some("TestNet".to_owned()),
            channel,
            power: -42,
            encryption: "WPA2".to_owned(),
            clients: HashMap::new(),
            beacon_count: 10,
            hidden: false,
            revealed: false,
        }));
        state
    }

    #[test]
    fn deauth_blocked_on_unknown_channel() {
        let state = make_state_with_ap(None);
        let mut dash = DashboardState {
            interface_name: Some("wlan0".to_owned()),
            ..DashboardState::default()
        };
        dash.table_state.select(Some(0));

        let outcome = handle_dashboard_key(&mut dash, &state, KeyCode::Char('d'));
        assert!(matches!(outcome, Outcome::Log(_)));
        assert!(dash.modal.is_none());
    }

    #[test]
    fn deauth_opens_modal_on_valid_channel() {
        let state = make_state_with_ap(Some(6));
        let mut dash = DashboardState {
            interface_name: Some("wlan0".to_owned()),
            ..DashboardState::default()
        };
        dash.table_state.select(Some(0));

        let outcome = handle_dashboard_key(&mut dash, &state, KeyCode::Char('d'));
        assert!(matches!(outcome, Outcome::Continue));
        assert!(matches!(dash.modal, Some(Modal::Deauth(_))));
    }
}
