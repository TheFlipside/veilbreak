//! Header bar showing interface, channel, capture size, and elapsed time.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use veilbreak_core::AppState;

use crate::theme;

pub fn draw(frame: &mut Frame, area: Rect, state: &AppState) {
    let elapsed = state.elapsed_secs();
    let mins = elapsed / 60;
    let secs = elapsed % 60;

    let text = format!(
        " iface: \u{2014}  ch: \u{2014}  capture: {} B  elapsed: {mins:02}:{secs:02}",
        state.capture_size,
    );

    let header = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" veilbreak ")
                .title_style(theme::TITLE)
                .border_style(theme::BORDER),
        )
        .style(theme::HEADER);

    frame.render_widget(header, area);
}
