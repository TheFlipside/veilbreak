//! Header bar showing interface, channel, capture size, and elapsed time.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use veilbreak_core::AppState;

use crate::app::DashboardState;
use crate::theme;

/// Renders the header bar showing interface, channel, capture size, and elapsed time.
pub fn draw(frame: &mut Frame, area: Rect, state: &AppState, dash: &DashboardState) {
    let elapsed = state.elapsed_secs();
    let mins = elapsed / 60;
    let secs = elapsed % 60;

    let iface = dash.interface_name.as_deref().unwrap_or("\u{2014}");
    let ch = state
        .current_channel
        .map_or_else(|| "\u{2014}".to_owned(), |c| c.to_string());

    let cap_size = format_bytes(state.capture_size);
    let ap_count = state.access_points.len();

    let band_label = dash.band.label();
    let text = format!(
        " iface: {iface}  band: {band_label}  ch: {ch}  capture: {cap_size}  APs: {ap_count}  elapsed: {mins:02}:{secs:02}",
    );

    let header = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" veilbreak ")
                .title_style(theme::title())
                .border_style(theme::border()),
        )
        .style(theme::header());

    frame.render_widget(header, area);
}

#[allow(clippy::cast_precision_loss)] // display-only, sub-byte precision irrelevant
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB");
    }
    let mb = kb / 1024.0;
    format!("{mb:.1} MB")
}
