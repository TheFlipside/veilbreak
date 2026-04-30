//! Detail pane for the selected access point (right panel of the dashboard).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use veilbreak_core::state::AccessPoint;
use veilbreak_core::validate;

use crate::app::{DashboardState, FocusPane};
use crate::theme;

/// Renders the detail pane for the currently selected AP.
pub fn draw(frame: &mut Frame, area: Rect, sorted: &[(&str, &AccessPoint)], dash: &DashboardState) {
    let border_style = if dash.focus == FocusPane::Detail {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };

    let block = Block::default()
        .title(" Details ")
        .title_style(theme::TITLE)
        .borders(Borders::ALL)
        .border_style(border_style);

    let selected = sorted.get(dash.selected_ap()).map(|&(_, ap)| ap);

    let Some(ap) = selected else {
        let content = Paragraph::new("  (no AP selected)")
            .style(theme::DIM)
            .block(block);
        frame.render_widget(content, area);
        return;
    };

    let ssid_display = match &ap.ssid {
        Some(ssid) => format!("<revealed: {}>", validate::sanitize_display_string(ssid)),
        None if ap.hidden => "<hidden>".to_owned(),
        None => "(unknown)".to_owned(),
    };
    let enc_display = validate::sanitize_display_string(&ap.encryption);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  BSSID:  ", theme::DIM),
            Span::raw(&ap.bssid),
        ]),
        Line::from(vec![
            Span::styled("  SSID:   ", theme::DIM),
            Span::raw(ssid_display),
        ]),
        Line::from(vec![
            Span::styled("  Chan:   ", theme::DIM),
            Span::raw(ap.channel.to_string()),
            Span::raw("    "),
            Span::styled("Enc: ", theme::DIM),
            Span::raw(enc_display),
        ]),
        Line::from(vec![
            Span::styled("  Power:  ", theme::DIM),
            Span::raw(format!("{} dBm", ap.power)),
            Span::raw("  "),
            Span::styled("Beacons: ", theme::DIM),
            Span::raw(ap.beacon_count.to_string()),
        ]),
        Line::raw(""),
    ];

    if ap.clients.is_empty() {
        lines.push(Line::styled("  (no associated clients)", theme::DIM));
    } else {
        lines.push(Line::styled("  Associated clients:", theme::DIM));
        for client in ap.clients.values() {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::raw(&client.mac),
                Span::raw("  "),
                Span::styled(format!("{} dBm", client.power), theme::DIM),
            ]));
        }
    }

    let content = Paragraph::new(lines).block(block);
    frame.render_widget(content, area);
}
