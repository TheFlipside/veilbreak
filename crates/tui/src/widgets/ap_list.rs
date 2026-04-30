//! Access point list pane (left panel of the dashboard).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, HighlightSpacing, Row, Table};
use veilbreak_core::state::AccessPoint;

use crate::app::{DashboardState, FocusPane};
use crate::theme;

/// Renders the AP list table into the given area.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    sorted: &[(&str, &AccessPoint)],
    dash: &mut DashboardState,
) {
    let border_style = if dash.focus == FocusPane::ApList {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };

    let block = Block::default()
        .title(format!(" Access Points [{}] ", dash.sort))
        .title_style(theme::TITLE)
        .borders(Borders::ALL)
        .border_style(border_style);

    if sorted.is_empty() {
        let empty = Table::new(
            vec![Row::new(vec!["  (no access points)"])],
            [Constraint::Fill(1)],
        )
        .style(theme::DIM)
        .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let header =
        Row::new(vec!["BSSID", "CH", "PWR", "ENC", "CLI", "BCN", "SSID"]).style(theme::HEADER);

    let rows: Vec<Row> = sorted
        .iter()
        .map(|&(_, ap)| {
            let (ssid_display, ssid_style) = match (ap.ssid.as_deref(), ap.revealed) {
                (Some(s), true) => (s, theme::REVEALED),
                (Some(s), false) => (s, Style::default()),
                (None, _) => (if ap.hidden { "<hid>" } else { "" }, Style::default()),
            };
            Row::new(vec![
                Cell::from(ap.bssid.clone()),
                if ap.channel > 0 {
                    Cell::from(ap.channel.to_string())
                } else {
                    Cell::from("\u{2014}")
                },
                Cell::from(ap.power.to_string()),
                Cell::from(ap.encryption.clone()),
                Cell::from(ap.clients.len().to_string()),
                Cell::from(ap.beacon_count.to_string()),
                Cell::from(ssid_display.to_owned()).style(ssid_style),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(17),
        Constraint::Length(3),
        Constraint::Length(4),
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Fill(1),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .highlight_symbol("\u{25b6} ")
        .row_highlight_style(theme::SELECTED)
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, &mut dash.table_state);
}
