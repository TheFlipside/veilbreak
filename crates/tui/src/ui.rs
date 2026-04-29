//! Top-level draw dispatch.

#[allow(clippy::wildcard_imports)] // ratatui prelude is designed for glob import
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use veilbreak_core::AppState;

use crate::app::Screen;
use crate::widgets;

/// Draws the current screen into the terminal frame.
pub fn draw(frame: &mut Frame, screen: &Screen, state: &AppState, tick: u8) {
    match screen {
        Screen::Loading => draw_loading(frame, tick),
        Screen::Setup(setup) => widgets::modal::draw_setup(frame, setup),
        Screen::Dashboard(dash) => draw_dashboard(frame, dash, state),
    }
}

fn draw_loading(frame: &mut Frame, tick: u8) {
    let area = frame.area();
    let dots = match tick % 4 {
        0 => "   ",
        1 => ".  ",
        2 => ".. ",
        _ => "...",
    };
    let text =
        Paragraph::new(format!("Detecting wireless interfaces{dots}")).alignment(Alignment::Center);
    let y = area.height / 2;
    let inner = Rect::new(area.x, y, area.width, 1);
    frame.render_widget(text, inner);
}

fn draw_dashboard(frame: &mut Frame, dash: &crate::app::DashboardState, state: &AppState) {
    use crate::app::Modal;

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
    let filtered: Vec<_> = sorted
        .into_iter()
        .filter(|(_, ap)| dash.filter.matches(ap))
        .collect();
    widgets::ap_list::draw(frame, body[0], &filtered, dash);
    widgets::detail::draw(frame, body[1], &filtered, dash);
    widgets::events::draw(frame, vertical[2], state, dash);
    widgets::keybinds::draw(frame, vertical[3], dash);

    match &dash.modal {
        Some(Modal::Deauth(modal)) => widgets::modal::draw_deauth_modal(frame, modal),
        Some(Modal::Filter { selected }) => {
            widgets::modal::draw_filter_modal(frame, *selected, &dash.filter);
        }
        Some(Modal::Help) => widgets::modal::draw_help_modal(frame),
        None => {}
    }
}
