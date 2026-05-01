//! Input validation for values originating from external sources (CSV, pcap).
//!
//! BSSIDs and interface names must pass validation before being used as
//! subprocess arguments to prevent injection.

/// Returns `true` if `s` is a valid BSSID (`AA:BB:CC:DD:EE:FF` form).
#[must_use]
pub fn is_valid_bssid(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 17
        && bytes.iter().enumerate().all(|(i, &b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Returns `true` if `s` is a valid phy identifier (`phyN` where N is one or more digits).
#[must_use]
pub fn is_valid_phy_name(s: &str) -> bool {
    s.strip_prefix("phy")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// Returns `true` if `s` is a valid Linux network interface name.
#[must_use]
pub fn is_valid_interface_name(s: &str) -> bool {
    let len = s.len();
    (1..=15).contains(&len)
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Returns `true` if `ch` is a valid 802.11 channel number (1–196).
#[must_use]
pub const fn is_valid_channel(ch: u32) -> bool {
    ch >= 1 && ch <= 196
}

/// Returns `true` if the BSSID belongs to the Wi-Fi Alliance OUI (`50:6F:9A`),
/// used by Wi-Fi Direct / P2P group owners.
///
/// Only returns `true` for well-formed BSSIDs (validated by [`is_valid_bssid`]).
#[must_use]
pub fn is_p2p_bssid(bssid: &str) -> bool {
    is_valid_bssid(bssid) && bssid[..8].eq_ignore_ascii_case("50:6F:9A")
}

/// Maximum length in bytes for an ESSID (IEEE 802.11 limit).
pub const MAX_ESSID_LEN: usize = 32;

/// Strips characters unsafe for terminal display from a string.
///
/// Removes ASCII control characters (0x00–0x1F, 0x7F), C1 control codes
/// (U+0080–U+009F), Unicode bidirectional overrides, and other invisible
/// formatting codepoints that could mislead the TUI operator.
#[must_use]
pub fn sanitize_display_string(s: &str) -> String {
    s.chars()
        .filter(|&c| !c.is_ascii_control() && !is_unsafe_unicode(c))
        .collect()
}

const fn is_unsafe_unicode(c: char) -> bool {
    matches!(c as u32,
        0x007F          // DEL
        | 0x0080..=0x009F // C1 control codes
        | 0x00AD        // soft hyphen
        | 0x200B..=0x200F // zero-width and Bidi marks
        | 0x2028..=0x202E // line/paragraph separators + Bidi overrides
        | 0x2066..=0x2069 // Bidi isolates
    )
}

/// Truncates a string to `max_bytes`, ensuring the cut falls on a UTF-8
/// character boundary.
#[must_use]
pub fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_bssids() {
        assert!(is_valid_bssid("AA:BB:CC:DD:EE:FF"));
        assert!(is_valid_bssid("aa:bb:cc:dd:ee:ff"));
        assert!(is_valid_bssid("00:11:22:33:44:55"));
        assert!(is_valid_bssid("aA:bB:cC:dD:eE:fF"));
    }

    #[test]
    fn invalid_bssids() {
        assert!(!is_valid_bssid(""));
        assert!(!is_valid_bssid("AA:BB:CC:DD:EE"));
        assert!(!is_valid_bssid("AA:BB:CC:DD:EE:FF:00"));
        assert!(!is_valid_bssid("GG:BB:CC:DD:EE:FF"));
        assert!(!is_valid_bssid("AA-BB-CC-DD-EE-FF"));
        assert!(!is_valid_bssid("AABBCCDDEEFF"));
    }

    #[test]
    fn valid_phy_names() {
        assert!(is_valid_phy_name("phy0"));
        assert!(is_valid_phy_name("phy1"));
        assert!(is_valid_phy_name("phy42"));
    }

    #[test]
    fn invalid_phy_names() {
        assert!(!is_valid_phy_name(""));
        assert!(!is_valid_phy_name("phy"));
        assert!(!is_valid_phy_name("phy0a"));
        assert!(!is_valid_phy_name("wlan0"));
        assert!(!is_valid_phy_name("phy#0"));
        assert!(!is_valid_phy_name("phy -1"));
    }

    #[test]
    fn valid_interface_names() {
        assert!(is_valid_interface_name("wlan0"));
        assert!(is_valid_interface_name("wlan0mon"));
        assert!(is_valid_interface_name("eth0"));
        assert!(is_valid_interface_name("wlp2s0"));
        assert!(is_valid_interface_name("my-iface_01"));
    }

    #[test]
    fn invalid_interface_names() {
        assert!(!is_valid_interface_name(""));
        assert!(!is_valid_interface_name(
            "this_name_is_way_too_long_for_linux"
        ));
        assert!(!is_valid_interface_name("iface name"));
        assert!(!is_valid_interface_name("iface;rm"));
        assert!(!is_valid_interface_name("wlan$0"));
    }

    #[test]
    fn sanitize_strips_control_chars() {
        assert_eq!(
            sanitize_display_string("hello\x1b[31mworld"),
            "hello[31mworld"
        );
        assert_eq!(sanitize_display_string("clean"), "clean");
        assert_eq!(sanitize_display_string("\x00\x07\x1b"), "");
        assert_eq!(sanitize_display_string("tab\there"), "tabhere");
    }

    #[test]
    fn sanitize_strips_del_and_c1() {
        assert_eq!(sanitize_display_string("a\x7Fb"), "ab");
        assert_eq!(sanitize_display_string("a\u{0080}b"), "ab");
        assert_eq!(sanitize_display_string("a\u{009F}b"), "ab");
    }

    #[test]
    fn sanitize_strips_bidi_overrides() {
        assert_eq!(sanitize_display_string("a\u{202A}b\u{202C}c"), "abc");
        assert_eq!(sanitize_display_string("a\u{2066}b\u{2069}c"), "abc");
        assert_eq!(sanitize_display_string("a\u{200F}b"), "ab");
    }

    #[test]
    fn sanitize_preserves_unicode() {
        assert_eq!(sanitize_display_string("café ☕"), "café ☕");
    }

    #[test]
    fn truncate_within_limit() {
        assert_eq!(truncate_utf8("short", 32), "short");
    }

    #[test]
    fn truncate_at_limit() {
        let long = "a]".repeat(20);
        let result = truncate_utf8(&long, 32);
        assert!(result.len() <= 32);
    }

    #[test]
    fn truncate_respects_char_boundary() {
        let s = "aé";
        let result = truncate_utf8(s, 2);
        assert_eq!(result, "a");
    }

    #[test]
    fn p2p_bssids() {
        assert!(is_p2p_bssid("50:6F:9A:01:00:00"));
        assert!(is_p2p_bssid("50:6f:9a:FF:EE:DD"));
        assert!(is_p2p_bssid("50:6F:9A:00:00:00"));
        assert!(!is_p2p_bssid("AA:BB:CC:DD:EE:FF"));
        assert!(!is_p2p_bssid("50:6F:9B:01:00:00"));
        assert!(!is_p2p_bssid(""));
        assert!(!is_p2p_bssid("50:6F"));
        assert!(!is_p2p_bssid("50:6F:9A"));
        assert!(!is_p2p_bssid("50:6F:9A:01:00:00:extra"));
    }

    #[test]
    fn valid_channels() {
        assert!(is_valid_channel(1));
        assert!(is_valid_channel(6));
        assert!(is_valid_channel(14));
        assert!(is_valid_channel(36));
        assert!(is_valid_channel(149));
        assert!(is_valid_channel(196));
    }

    #[test]
    fn invalid_channels() {
        assert!(!is_valid_channel(0));
        assert!(!is_valid_channel(197));
        assert!(!is_valid_channel(u32::MAX));
    }
}
