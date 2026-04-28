//! Access point list pane (left panel of the dashboard).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Row, Table};
use veilbreak_core::state::AccessPoint;

use crate::app::{DashboardState, FocusPane};
use crate::theme;

/// Renders the AP list table into the given area.
pub fn draw(frame: &mut Frame, area: Rect, sorted: &[(&str, &AccessPoint)], dash: &DashboardState) {
    let border_style = if dash.focus == FocusPane::ApList {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };

    let sort_label = dash.sort;
    let block = Block::default()
        .title(format!(" Access Points [{sort_label}] "))
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
        Row::new(vec!["", "BSSID", "CH", "PWR", "ENC", "CLI", "BCN", "SSID"]).style(theme::HEADER);

    let rows: Vec<Row> = sorted
        .iter()
        .enumerate()
        .map(|(i, &(_, ap))| {
            let prefix = if i == dash.selected_ap {
                "\u{25b6}"
            } else {
                " "
            };
            let ssid_display = ap
                .ssid
                .as_deref()
                .unwrap_or(if ap.hidden { "<hid>" } else { "" });
            let style = if i == dash.selected_ap {
                theme::SELECTED
            } else {
                Style::default()
            };
            Row::new(vec![
                prefix.to_owned(),
                ap.bssid.clone(),
                ap.channel.to_string(),
                ap.power.to_string(),
                ap.encryption.clone(),
                ap.clients.len().to_string(),
                ap.beacon_count.to_string(),
                ssid_display.to_owned(),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(1),
        Constraint::Length(17),
        Constraint::Length(3),
        Constraint::Length(4),
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Fill(1),
    ];

    let table = Table::new(rows, widths).header(header).block(block);

    frame.render_widget(table, area);
}
