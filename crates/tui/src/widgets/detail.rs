//! Detail pane for the selected access point (right panel of the dashboard).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use veilbreak_core::AppState;

use crate::app::{DashboardState, FocusPane};
use crate::theme;

pub fn draw(frame: &mut Frame, area: Rect, _state: &AppState, dash: &DashboardState) {
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

    let content = Paragraph::new("  (no AP selected)")
        .style(theme::DIM)
        .block(block);

    frame.render_widget(content, area);
}
