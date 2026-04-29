//! Top-level draw dispatch.

#[allow(clippy::wildcard_imports)] // ratatui prelude is designed for glob import
use ratatui::prelude::*;
use veilbreak_core::AppState;

use crate::app::Screen;
use crate::widgets;

/// Draws the current screen into the terminal frame.
pub fn draw(frame: &mut Frame, screen: &Screen, state: &AppState) {
    match screen {
        Screen::Setup(setup) => widgets::modal::draw_setup(frame, setup),
        Screen::Dashboard(dash) => draw_dashboard(frame, dash, state),
    }
}

fn draw_dashboard(frame: &mut Frame, dash: &crate::app::DashboardState, state: &AppState) {
    let area = frame.area();

    let vertical = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(8),
        Constraint::Length(1),
    ])
    .split(area);

    widgets::header::draw(frame, vertical[0], state, dash);

    let body = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(vertical[1]);

    let sorted = state.sorted_aps(dash.sort);
    widgets::ap_list::draw(frame, body[0], &sorted, dash);
    widgets::detail::draw(frame, body[1], &sorted, dash);
    widgets::events::draw(frame, vertical[2], state, dash);
    widgets::keybinds::draw(frame, vertical[3], dash);

    if let Some(modal) = &dash.modal {
        widgets::modal::draw_deauth_modal(frame, modal);
    }
}
