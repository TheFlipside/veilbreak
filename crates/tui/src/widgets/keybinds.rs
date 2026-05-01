//! Context-sensitive keybind hint bar at the bottom of the dashboard.

use ratatui::prelude::*;
use ratatui::text::{Line, Span};

use crate::app::{DashboardState, FocusPane};
use crate::theme;

pub fn draw(frame: &mut Frame, area: Rect, dash: &DashboardState) {
    let hints: &[(&str, &str)] = match dash.focus {
        FocusPane::ApList => &[
            ("Tab", "focus"),
            ("\u{2191}\u{2193}/jk", "nav"),
            ("Enter", "select"),
            ("d", "deauth"),
            ("s", "sort"),
            ("f", "filter"),
            ("?", "help"),
            ("q", "quit"),
        ],
        FocusPane::Detail => &[
            ("Tab", "focus"),
            ("d", "deauth"),
            ("f", "filter"),
            ("?", "help"),
            ("q", "quit"),
        ],
        FocusPane::EventLog => &[
            ("Tab", "focus"),
            ("\u{2191}\u{2193}/jk", "scroll"),
            ("f", "filter"),
            ("?", "help"),
            ("q", "quit"),
        ],
    };

    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            [
                Span::raw(" ["),
                Span::styled(*key, theme::keybind_key()),
                Span::raw("] "),
                Span::styled(*desc, theme::keybind_desc()),
            ]
        })
        .collect();

    frame.render_widget(Line::from(spans), area);
}
