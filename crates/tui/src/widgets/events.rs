//! Event log pane showing session activity (bottom of the dashboard).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use veilbreak_core::AppState;

use crate::app::{DashboardState, FocusPane};
use crate::theme;

/// Renders the event log pane into the given area.
pub fn draw(frame: &mut Frame, area: Rect, state: &AppState, dash: &DashboardState) {
    let border_style = if dash.focus == FocusPane::EventLog {
        theme::border_focused()
    } else {
        theme::border()
    };

    let block = Block::default()
        .title(" Events ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(border_style);

    if state.event_log.is_empty() {
        let content = Paragraph::new("  (no events)")
            .style(theme::dim())
            .block(block);
        frame.render_widget(content, area);
        return;
    }

    let visible_height = area.height.saturating_sub(2) as usize;
    let max_scroll = state.event_log.len().saturating_sub(1);
    let scroll = dash.event_scroll.min(max_scroll);

    let lines: Vec<Line> = state
        .event_log
        .iter()
        .rev()
        .skip(scroll)
        .take(visible_height)
        .map(|entry| {
            let mins = entry.elapsed_secs / 60;
            let secs = entry.elapsed_secs % 60;
            Line::from(vec![
                Span::styled(format!(" {mins:02}:{secs:02}  "), theme::dim()),
                Span::raw(&entry.message),
            ])
        })
        .collect();

    let content = Paragraph::new(lines).block(block);
    frame.render_widget(content, area);
}
