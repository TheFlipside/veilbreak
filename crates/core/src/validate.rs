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
}
