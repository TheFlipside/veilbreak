//! Full-screen modal dialogs for the pre-session setup flow.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use veilbreak_core::interface::WirelessInterface;

use crate::app::SetupScreen;
use crate::theme;

/// Draws the appropriate setup modal for the current setup step.
pub fn draw_setup(frame: &mut Frame, setup: &SetupScreen) {
    let area = frame.area();

    match setup {
        SetupScreen::InterfaceSelect {
            interfaces,
            selected,
        } => draw_interface_select(frame, area, interfaces, *selected),
        SetupScreen::ModeConfirm {
            interface,
            dual_card,
        } => draw_mode_confirm(frame, area, interface, *dual_card),
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn draw_interface_select(
    frame: &mut Frame,
    area: Rect,
    interfaces: &[WirelessInterface],
    selected: usize,
) {
    let height = u16::try_from(interfaces.len() + 4)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let modal = centered_rect(64, height, area);

    let block = Block::default()
        .title(" Select Monitoring Interface ")
        .title_style(theme::TITLE)
        .borders(Borders::ALL)
        .border_style(theme::BORDER_FOCUSED);

    let items: Vec<ListItem> = interfaces
        .iter()
        .enumerate()
        .map(|(i, iface)| {
            let monitor_tag = if iface.monitor_capable { " [mon]" } else { "" };
            let prefix = if i == selected { "\u{25b6} " } else { "  " };
            let style = if i == selected {
                theme::SELECTED
            } else {
                Style::default()
            };
            ListItem::new(format!(
                "{prefix}{} ({}) {}{monitor_tag}",
                iface.name, iface.phy, iface.addr,
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, modal);
}

fn draw_mode_confirm(
    frame: &mut Frame,
    area: Rect,
    interface: &WirelessInterface,
    dual_card: bool,
) {
    let modal = centered_rect(54, 8, area);
    let block = Block::default()
        .title(" Confirm Mode ")
        .title_style(theme::TITLE)
        .borders(Borders::ALL)
        .border_style(theme::BORDER_FOCUSED);

    let mode_text = if dual_card {
        "Dual-card mode: host connectivity preserved."
    } else {
        "Single-card mode: host connectivity will be lost!"
    };

    let text = format!(
        "\n  Interface: {}\n  {mode_text}\n\n  Press Enter to continue, Esc to cancel.",
        interface.name,
    );

    let content = Paragraph::new(text).block(block);
    frame.render_widget(content, modal);
}
