//! Configurable color theme for the TUI.
//!
//! Loads an optional TOML file at startup and exposes accessor functions
//! returning [`ratatui::style::Style`] values. Unspecified keys inherit
//! from built-in defaults; partial overrides merge (setting only `fg` on
//! `title` preserves the default `bold` modifier).

use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, ensure};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

/// Refuse to read theme files larger than 64 KiB.
const MAX_THEME_FILE_BYTES: u64 = 65_536;

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------

static THEME: OnceLock<Theme> = OnceLock::new();

/// Set the global theme. Subsequent calls are ignored (first write wins).
pub fn init(theme: Theme) {
    let _ = THEME.set(theme);
}

fn get() -> &'static Theme {
    static DEFAULT: OnceLock<Theme> = OnceLock::new();
    THEME
        .get()
        .unwrap_or_else(|| DEFAULT.get_or_init(Theme::default))
}

// ---------------------------------------------------------------------------
// Accessors — one per semantic style
// ---------------------------------------------------------------------------

/// Default border style (unfocused panes).
pub fn border() -> Style {
    get().border
}

/// Border style for the focused pane.
pub fn border_focused() -> Style {
    get().border_focused
}

/// Border style for action modals with side effects (e.g. deauth).
pub fn border_danger() -> Style {
    get().border_danger
}

/// Header bar text.
pub fn header() -> Style {
    get().header
}

/// Pane titles.
pub fn title() -> Style {
    get().title
}

/// Currently selected row.
pub fn selected() -> Style {
    get().selected
}

/// Keybind key highlight.
pub fn keybind_key() -> Style {
    get().keybind_key
}

/// Keybind description text.
pub fn keybind_desc() -> Style {
    get().keybind_desc
}

/// Revealed SSID (was hidden, now known).
pub fn revealed() -> Style {
    get().revealed
}

/// Wi-Fi Direct / P2P BSSID (OUI `50:6F:9A`).
pub fn wifi_direct() -> Style {
    get().wifi_direct
}

/// Dimmed placeholder text.
pub fn dim() -> Style {
    get().dim
}

// ---------------------------------------------------------------------------
// Resolved theme
// ---------------------------------------------------------------------------

/// Fully resolved theme with concrete [`Style`] values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    border: Style,
    border_focused: Style,
    border_danger: Style,
    header: Style,
    title: Style,
    selected: Style,
    keybind_key: Style,
    keybind_desc: Style,
    revealed: Style,
    wifi_direct: Style,
    dim: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border: Style::new().fg(Color::DarkGray),
            border_focused: Style::new().fg(Color::Cyan),
            border_danger: Style::new().fg(Color::Red),
            header: Style::new().fg(Color::White),
            title: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            selected: Style::new().bg(Color::DarkGray).fg(Color::White),
            keybind_key: Style::new().fg(Color::Cyan),
            keybind_desc: Style::new().fg(Color::DarkGray),
            revealed: Style::new().fg(Color::Green),
            wifi_direct: Style::new().fg(Color::Yellow),
            dim: Style::new().fg(Color::DarkGray),
        }
    }
}

// ---------------------------------------------------------------------------
// TOML schema (deserialization target)
// ---------------------------------------------------------------------------

/// Raw TOML representation — every field is optional so partial overrides
/// work naturally.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    border: Option<StyleDef>,
    border_focused: Option<StyleDef>,
    border_danger: Option<StyleDef>,
    header: Option<StyleDef>,
    title: Option<StyleDef>,
    selected: Option<StyleDef>,
    keybind_key: Option<StyleDef>,
    keybind_desc: Option<StyleDef>,
    revealed: Option<StyleDef>,
    wifi_direct: Option<StyleDef>,
    dim: Option<StyleDef>,
}

/// A single style definition in the TOML file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleDef {
    /// Foreground color (named or `#RRGGBB` hex).
    fg: Option<String>,
    /// Background color (named or `#RRGGBB` hex).
    bg: Option<String>,
    /// Bold modifier override. `None` inherits from the default.
    bold: Option<bool>,
    /// Italic modifier override.
    italic: Option<bool>,
    /// Underline modifier override.
    underline: Option<bool>,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Returns the platform-conventional default theme path.
///
/// Resolution order:
/// 1. `$XDG_CONFIG_HOME/veilbreak/theme.toml`
/// 2. Under `sudo`: `~<SUDO_USER>/.config/veilbreak/theme.toml`
/// 3. `$HOME/.config/veilbreak/theme.toml`
pub fn default_path() -> Option<PathBuf> {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| invoking_user_home().map(|h| h.join(".config")))?;
    Some(config_dir.join("veilbreak").join("theme.toml"))
}

/// Returns the home directory of the real (pre-sudo) user.
///
/// When running under `sudo`, `$HOME` is `/root`, but the user's config
/// lives in their own home. We look up `$SUDO_USER` via `getpwnam` to
/// resolve the original home directory.
///
/// Must be called before the tokio thread pool starts; `getpwnam(3)` is
/// not thread-safe.
fn invoking_user_home() -> Option<PathBuf> {
    if let Some(sudo_user) = std::env::var_os("SUDO_USER") {
        let user_bytes = sudo_user.as_encoded_bytes();
        if user_bytes.len() < 128 && user_bytes.iter().all(|&b| b != 0) {
            let mut buf = Vec::with_capacity(user_bytes.len() + 1);
            buf.extend_from_slice(user_bytes);
            buf.push(0);

            // SAFETY: `buf` is a valid NUL-terminated C string and
            // `getpwnam` returns a pointer to a static passwd struct.
            let pw = unsafe { libc::getpwnam(buf.as_ptr().cast::<libc::c_char>()) };
            if !pw.is_null() {
                // SAFETY: `pw` is non-null; `pw_dir` is checked for null
                // before dereferencing (POSIX does not guarantee non-null
                // `pw_dir` on all NSS backends).
                let dir_ptr = unsafe { (*pw).pw_dir };
                if !dir_ptr.is_null() {
                    // SAFETY: `dir_ptr` is a non-null, NUL-terminated C string
                    // owned by the static passwd buffer.
                    let home = unsafe { std::ffi::CStr::from_ptr(dir_ptr) };
                    if let Ok(s) = home.to_str() {
                        return Some(PathBuf::from(s));
                    }
                }
            }
        }
    }

    std::env::var_os("HOME").map(PathBuf::from)
}

/// Loads and validates a theme from a TOML file.
///
/// # Safety considerations
///
/// - Opens with `O_NOFOLLOW | O_CLOEXEC` on the original path, rejecting
///   symlinks before any data is read (important since veilbreak runs as
///   root).
/// - All checks (`is_file`, size limit) use `fstat` on the open fd, not a
///   separate `stat` call, eliminating TOCTOU races.
/// - Read is capped via `take()` to enforce the size limit regardless of
///   concurrent file growth.
/// - Uses `deny_unknown_fields` to reject unrecognised keys.
/// - Validates every color string against a strict allowlist of named colors
///   and `#RRGGBB` hex patterns.
///
/// # Errors
///
/// Returns an error if the file cannot be read, exceeds the size limit,
/// contains invalid TOML, has unknown keys, or specifies unrecognised
/// color values.
pub fn load_from_file(path: &Path) -> anyhow::Result<Theme> {
    let display_path = path.display().to_string();

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .with_context(|| format!("cannot open theme file: {display_path}"))?;

    let metadata = file
        .metadata()
        .with_context(|| format!("cannot stat theme file: {display_path}"))?;

    ensure!(
        metadata.is_file(),
        "theme path is not a regular file: {display_path}",
    );

    ensure!(
        metadata.len() <= MAX_THEME_FILE_BYTES,
        "theme file too large ({} bytes, max {MAX_THEME_FILE_BYTES}): {display_path}",
        metadata.len(),
    );

    let mut contents = String::new();
    file.take(MAX_THEME_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .with_context(|| format!("cannot read theme file: {display_path}"))?;

    ensure!(
        contents.len() as u64 <= MAX_THEME_FILE_BYTES,
        "theme file too large (read {} bytes, max {MAX_THEME_FILE_BYTES}): {display_path}",
        contents.len(),
    );

    let theme_file: ThemeFile = toml::from_str(&contents)
        .with_context(|| format!("invalid theme TOML in {display_path}"))?;

    resolve_theme(&theme_file).with_context(|| format!("invalid style in {display_path}"))
}

// ---------------------------------------------------------------------------
// Resolution (TOML → Theme)
// ---------------------------------------------------------------------------

fn resolve_theme(file: &ThemeFile) -> anyhow::Result<Theme> {
    let d = Theme::default();
    Ok(Theme {
        border: resolve_style(file.border.as_ref(), d.border)?,
        border_focused: resolve_style(file.border_focused.as_ref(), d.border_focused)?,
        border_danger: resolve_style(file.border_danger.as_ref(), d.border_danger)?,
        header: resolve_style(file.header.as_ref(), d.header)?,
        title: resolve_style(file.title.as_ref(), d.title)?,
        selected: resolve_style(file.selected.as_ref(), d.selected)?,
        keybind_key: resolve_style(file.keybind_key.as_ref(), d.keybind_key)?,
        keybind_desc: resolve_style(file.keybind_desc.as_ref(), d.keybind_desc)?,
        revealed: resolve_style(file.revealed.as_ref(), d.revealed)?,
        wifi_direct: resolve_style(file.wifi_direct.as_ref(), d.wifi_direct)?,
        dim: resolve_style(file.dim.as_ref(), d.dim)?,
    })
}

fn resolve_style(def: Option<&StyleDef>, fallback: Style) -> anyhow::Result<Style> {
    let Some(def) = def else {
        return Ok(fallback);
    };

    let mut style = fallback;

    if let Some(fg) = &def.fg {
        style =
            style.fg(parse_color(fg).ok_or_else(|| anyhow::anyhow!("unrecognised color: {fg:?}"))?);
    }

    if let Some(bg) = &def.bg {
        style =
            style.bg(parse_color(bg).ok_or_else(|| anyhow::anyhow!("unrecognised color: {bg:?}"))?);
    }

    apply_modifier(&mut style, Modifier::BOLD, def.bold);
    apply_modifier(&mut style, Modifier::ITALIC, def.italic);
    apply_modifier(&mut style, Modifier::UNDERLINED, def.underline);

    Ok(style)
}

const fn apply_modifier(style: &mut Style, modifier: Modifier, flag: Option<bool>) {
    match flag {
        Some(true) => *style = style.add_modifier(modifier),
        Some(false) => *style = style.remove_modifier(modifier),
        None => {}
    }
}

// ---------------------------------------------------------------------------
// Color parsing
// ---------------------------------------------------------------------------

/// Parses a color string into a [`Color`].
///
/// Accepts ratatui named colors (case-insensitive, with `_` or no separator)
/// and `#RRGGBB` hex notation. Returns `None` for anything else.
fn parse_color(s: &str) -> Option<Color> {
    let lower = s.to_lowercase();
    match lower.as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "white" => Some(Color::White),
        "dark_gray" | "darkgray" | "dark_grey" | "darkgrey" => Some(Color::DarkGray),
        "light_red" | "lightred" => Some(Color::LightRed),
        "light_green" | "lightgreen" => Some(Color::LightGreen),
        "light_yellow" | "lightyellow" => Some(Color::LightYellow),
        "light_blue" | "lightblue" => Some(Color::LightBlue),
        "light_magenta" | "lightmagenta" => Some(Color::LightMagenta),
        "light_cyan" | "lightcyan" => Some(Color::LightCyan),
        _ if lower.starts_with('#') && lower.len() == 7 => {
            let r = u8::from_str_radix(&lower[1..3], 16).ok()?;
            let g = u8::from_str_radix(&lower[3..5], 16).ok()?;
            let b = u8::from_str_radix(&lower[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_named_colors() {
        assert_eq!(parse_color("red"), Some(Color::Red));
        assert_eq!(parse_color("Red"), Some(Color::Red));
        assert_eq!(parse_color("dark_gray"), Some(Color::DarkGray));
        assert_eq!(parse_color("DarkGray"), Some(Color::DarkGray));
        assert_eq!(parse_color("darkgrey"), Some(Color::DarkGray));
        assert_eq!(parse_color("light_cyan"), Some(Color::LightCyan));
    }

    #[test]
    fn parse_hex_colors() {
        assert_eq!(parse_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("#FF00AA"), Some(Color::Rgb(255, 0, 170)));
        assert_eq!(parse_color("#000000"), Some(Color::Rgb(0, 0, 0)));
        assert_eq!(parse_color("#ffffff"), Some(Color::Rgb(255, 255, 255)));
    }

    #[test]
    fn reject_invalid_colors() {
        assert_eq!(parse_color(""), None);
        assert_eq!(parse_color("rainbow"), None);
        assert_eq!(parse_color("#fff"), None);
        assert_eq!(parse_color("#GGGGGG"), None);
        assert_eq!(parse_color("#ff00ff00"), None);
        assert_eq!(parse_color("red; rm -rf /"), None);
        assert_eq!(parse_color("../../../etc/passwd"), None);
    }

    #[test]
    fn empty_theme_file_gives_defaults() {
        let file = ThemeFile::default();
        let theme = resolve_theme(&file).unwrap();
        assert_eq!(theme, Theme::default());
    }

    #[test]
    fn partial_override_preserves_unset_styles() {
        let toml_str = r#"
[title]
fg = "yellow"
"#;
        let file: ThemeFile = toml::from_str(toml_str).unwrap();
        let theme = resolve_theme(&file).unwrap();
        assert_eq!(theme.title.fg, Some(Color::Yellow));
        assert_eq!(theme.border, Theme::default().border);
        assert_eq!(theme.revealed, Theme::default().revealed);
    }

    #[test]
    fn partial_override_inherits_modifiers() {
        let toml_str = r#"
[title]
fg = "yellow"
"#;
        let file: ThemeFile = toml::from_str(toml_str).unwrap();
        let theme = resolve_theme(&file).unwrap();
        let expected = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        assert_eq!(theme.title, expected);
    }

    #[test]
    fn explicit_bold_false_removes_modifier() {
        let toml_str = r"
[title]
bold = false
";
        let file: ThemeFile = toml::from_str(toml_str).unwrap();
        let theme = resolve_theme(&file).unwrap();
        assert_eq!(theme.title.fg, Some(Color::Cyan));
        assert!(!theme.title.add_modifier.contains(Modifier::BOLD));
        assert!(theme.title.sub_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn hex_color_in_toml() {
        let toml_str = r##"
[border]
fg = "#586e75"
"##;
        let file: ThemeFile = toml::from_str(toml_str).unwrap();
        let theme = resolve_theme(&file).unwrap();
        assert_eq!(theme.border.fg, Some(Color::Rgb(0x58, 0x6e, 0x75)));
    }

    #[test]
    fn unknown_field_rejected() {
        let toml_str = r#"
[bogus_style]
fg = "red"
"#;
        let result: Result<ThemeFile, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_style_field_rejected() {
        let toml_str = r#"
[border]
fg = "red"
blink = true
"#;
        let result: Result<ThemeFile, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_color_value_rejected() {
        let toml_str = r#"
[border]
fg = "neon_pink"
"#;
        let file: ThemeFile = toml::from_str(toml_str).unwrap();
        assert!(resolve_theme(&file).is_err());
    }
}
