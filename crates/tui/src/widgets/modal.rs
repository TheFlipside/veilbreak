//! Modal dialogs for the setup flow and runtime overlays (deauth target picker).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use veilbreak_core::interface::WirelessInterface;

use crate::app::{DeauthModal, SetupScreen};
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

/// Draws the deauth target selection modal as a centered overlay.
pub fn draw_deauth_modal(frame: &mut Frame, modal: &DeauthModal) {
    let area = frame.area();

    let item_count = 1 + modal.clients.len();
    let height = u16::try_from(item_count + 5)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let modal_area = centered_rect(50, height, area);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(format!(" Deauth: {} ", modal.bssid))
        .title_style(theme::TITLE)
        .borders(Borders::ALL)
        .border_style(theme::BORDER_DANGER);

    let mut items: Vec<ListItem> = Vec::with_capacity(item_count + 1);

    let broadcast_prefix = if modal.selected == 0 {
        "\u{25b6} "
    } else {
        "  "
    };
    let broadcast_style = if modal.selected == 0 {
        theme::SELECTED
    } else {
        Style::default()
    };
    items.push(
        ListItem::new(format!("{broadcast_prefix}Broadcast (all clients)")).style(broadcast_style),
    );

    for (i, (mac, power)) in modal.clients.iter().enumerate() {
        let idx = i + 1;
        let prefix = if modal.selected == idx {
            "\u{25b6} "
        } else {
            "  "
        };
        let style = if modal.selected == idx {
            theme::SELECTED
        } else {
            Style::default()
        };
        items.push(ListItem::new(format!("{prefix}{mac}  {power} dBm")).style(style));
    }

    items.push(ListItem::new(""));
    items.push(ListItem::new(Line::from(vec![
        Span::raw("  ["),
        Span::styled("Enter", theme::KEYBIND_KEY),
        Span::raw("] send  ["),
        Span::styled("Esc", theme::KEYBIND_KEY),
        Span::raw("] cancel"),
    ])));

    let list = List::new(items).block(block);
    frame.render_widget(list, modal_area);
}
