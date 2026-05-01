//! Modal dialogs for the setup flow and runtime overlays.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use veilbreak_core::Band;
use veilbreak_core::interface::WirelessInterface;

use veilbreak_core::validate::{is_valid_bssid, sanitize_display_string};

use crate::app::{DeauthModal, FilterState, SetupScreen};
use crate::theme;

/// Draws the appropriate setup modal for the current setup step.
pub fn draw_setup(frame: &mut Frame, setup: &SetupScreen) {
    let area = frame.area();

    match setup {
        SetupScreen::InterfaceSelect {
            interfaces,
            selected,
            ..
        } => draw_interface_select(frame, area, interfaces, *selected),
        SetupScreen::BandSelect { selected, .. } => draw_band_select(frame, area, *selected),
        SetupScreen::ModeConfirm {
            interface,
            dual_card,
            band,
        } => draw_mode_confirm(frame, area, interface, *dual_card, *band),
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

    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(" Select Monitoring Interface ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(theme::border_focused());

    let items: Vec<ListItem> = interfaces
        .iter()
        .enumerate()
        .map(|(i, iface)| {
            let monitor_tag = if iface.monitor_capable { " [mon]" } else { "" };
            let prefix = if i == selected { "\u{25b6} " } else { "  " };
            let style = if i == selected {
                theme::selected()
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

fn draw_band_select(frame: &mut Frame, area: Rect, selected: Band) {
    let bands = [Band::Bg, Band::A, Band::Abg];
    let height = u16::try_from(bands.len() + 4)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let modal = centered_rect(52, height, area);

    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(" Select Scan Band ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(theme::border_focused());

    let items: Vec<ListItem> = bands
        .iter()
        .map(|&band| {
            let prefix = if band == selected { "\u{25b6} " } else { "  " };
            let style = if band == selected {
                theme::selected()
            } else {
                Style::default()
            };
            ListItem::new(format!("{prefix}{}", band.label())).style(style)
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
    band: Band,
) {
    let modal = centered_rect(54, 9, area);

    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(" Confirm Mode ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(theme::border_focused());

    let mode_text = if dual_card {
        "Dual-card mode: host connectivity preserved."
    } else {
        "Single-card mode: host connectivity will be lost!"
    };

    let band_label = band.label();
    let text = format!(
        "\n  Interface: {}\n  Band: {band_label}\n  {mode_text}\n\n  Press Enter to continue, Esc to cancel.",
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

    debug_assert!(
        is_valid_bssid(&modal.bssid),
        "DeauthModal.bssid must be a validated BSSID"
    );
    let block = Block::default()
        .title(format!(
            " Deauth: {} ",
            sanitize_display_string(&modal.bssid)
        ))
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(theme::border_danger());

    let mut items: Vec<ListItem> = Vec::with_capacity(item_count + 2);

    let broadcast_prefix = if modal.selected == 0 {
        "\u{25b6} "
    } else {
        "  "
    };
    let broadcast_style = if modal.selected == 0 {
        theme::selected()
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
            theme::selected()
        } else {
            Style::default()
        };
        debug_assert!(
            is_valid_bssid(mac),
            "DeauthModal client MAC must be a validated MAC"
        );
        let safe_mac = sanitize_display_string(mac);
        items.push(ListItem::new(format!("{prefix}{safe_mac}  {power} dBm")).style(style));
    }

    items.push(ListItem::new(""));
    items.push(ListItem::new(Line::from(vec![
        Span::raw("  ["),
        Span::styled("Enter", theme::keybind_key()),
        Span::raw("] send  ["),
        Span::styled("Esc", theme::keybind_key()),
        Span::raw("] cancel"),
    ])));

    let list = List::new(items).block(block);
    frame.render_widget(list, modal_area);
}

const HELP_ENTRIES: &[(&str, &str)] = &[
    ("\u{2191}\u{2193} / j k", "Navigate list"),
    ("PgUp / PgDn", "Page scroll"),
    ("Tab / Shift+Tab", "Cycle focus pane"),
    ("Enter", "Select AP / confirm"),
    ("d", "Deauth selected AP"),
    ("s", "Cycle sort column"),
    ("f", "Toggle filters"),
    ("g / G", "Jump to first / last AP"),
    ("?", "This help"),
    ("q / Esc", "Quit / close modal"),
];

/// Draws the keybind reference help overlay.
pub fn draw_help_modal(frame: &mut Frame) {
    let area = frame.area();

    let height = u16::try_from(HELP_ENTRIES.len() + 5)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let modal_area = centered_rect(56, height, area);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Keybind Reference ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(theme::border_focused());

    let mut items: Vec<ListItem> = HELP_ENTRIES
        .iter()
        .map(|(key, desc)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {key:<20}"), theme::keybind_key()),
                Span::raw(*desc),
            ]))
        })
        .collect();

    items.push(ListItem::new(""));
    items.push(ListItem::new(Line::from(vec![
        Span::raw("  ["),
        Span::styled("Esc", theme::keybind_key()),
        Span::raw("] close"),
    ])));

    let list = List::new(items).block(block);
    frame.render_widget(list, modal_area);
}

/// Draws the AP list filter modal.
pub fn draw_filter_modal(frame: &mut Frame, selected: usize, filter: &FilterState) {
    let area = frame.area();
    let modal_area = centered_rect(44, 8, area);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Filters ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(theme::border_focused());

    let hidden_prefix = if selected == 0 { "\u{25b6} " } else { "  " };
    let band_prefix = if selected == 1 { "\u{25b6} " } else { "  " };
    let hidden_style = if selected == 0 {
        theme::selected()
    } else {
        Style::default()
    };
    let band_style = if selected == 1 {
        theme::selected()
    } else {
        Style::default()
    };

    let hidden_value = if filter.hidden_only { "ON" } else { "OFF" };

    let items = vec![
        ListItem::new(format!("{hidden_prefix}Hidden/revealed:  {hidden_value}"))
            .style(hidden_style),
        ListItem::new(format!("{band_prefix}Band:  {}", filter.band.label())).style(band_style),
        ListItem::new(""),
        ListItem::new(Line::from(vec![
            Span::raw("  ["),
            Span::styled("Enter", theme::keybind_key()),
            Span::raw("] toggle  ["),
            Span::styled("Esc", theme::keybind_key()),
            Span::raw("] close"),
        ])),
    ];

    let list = List::new(items).block(block);
    frame.render_widget(list, modal_area);
}
