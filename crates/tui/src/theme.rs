//! Color and style constants for the TUI.

use ratatui::style::{Color, Modifier, Style};

/// Default border style (unfocused panes).
pub const BORDER: Style = Style::new().fg(Color::DarkGray);

/// Border style for the focused pane.
pub const BORDER_FOCUSED: Style = Style::new().fg(Color::Cyan);

/// Border style for action modals with side effects (e.g. deauth).
pub const BORDER_DANGER: Style = Style::new().fg(Color::Red);

/// Header bar text.
pub const HEADER: Style = Style::new().fg(Color::White);

/// Pane titles.
pub const TITLE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);

/// Currently selected row.
pub const SELECTED: Style = Style::new().bg(Color::DarkGray).fg(Color::White);

/// Keybind key highlight.
pub const KEYBIND_KEY: Style = Style::new().fg(Color::Cyan);

/// Keybind description text.
pub const KEYBIND_DESC: Style = Style::new().fg(Color::DarkGray);

/// Revealed SSID (was hidden, now known).
pub const REVEALED: Style = Style::new().fg(Color::Green);

/// Dimmed placeholder text.
pub const DIM: Style = Style::new().fg(Color::DarkGray);
