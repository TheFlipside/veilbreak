# Adapter Compatibility

Veilbreak requires a wireless adapter that supports **monitor mode** for
scanning and capture. **Frame injection** (needed for deauth) is a separate
capability that not all monitor-mode adapters provide.

## Testing Your Adapter

Put the adapter into monitor mode, then run:

```bash
sudo aireplay-ng --test wlan0mon
```

- `Injection is working!` — the adapter supports both monitor mode and
  injection. It can be used for scanning **and** deauth.
- Failure / timeout — the adapter supports monitor mode only. It can scan
  and capture but cannot send deauth frames.

## Tested Adapters

Status is one of:

- **Verified** — tested first-hand by a Veilbreak contributor.
- **Community** — widely reported working by the wireless security community
  (see [Sources](#sources)).

### Confirmed Working (Monitor + Injection)

| Adapter                        | Chipset           | USB ID      | Driver                 | Band             | Status    |
| ------------------------------ | ----------------- | ----------- | ---------------------- | ---------------- | --------- |
| Alfa AWUS036ACM                | MediaTek MT7612U  | `0e8d:7612` | `mt76x2u`              | 2.4 + 5 GHz      | Verified  |
| Alfa AWUS036NHA                | Atheros AR9271    | `0cf3:9271` | `ath9k_htc`            | 2.4 GHz          | Community |
| TP-Link TL-WN722N **v1 only**  | Atheros AR9271    | `0cf3:9271` | `ath9k_htc`            | 2.4 GHz          | Community |
| Alfa AWUS036NH                 | Ralink RT3070     | `148f:3070` | `rt2800usb`            | 2.4 GHz          | Community |
| Panda PAU09                    | Ralink RT5572     | `148f:5572` | `rt2800usb`            | 2.4 + 5 GHz      | Community |
| Alfa AWUS036ACHM               | MediaTek MT7610U  | `0e8d:7610` | `mt76x0u`              | 2.4 + 5 GHz      | Community |
| Alfa AWUS036ACH                | Realtek RTL8812AU | `0bda:8812` | `rtw88` (kernel 6.14+) | 2.4 + 5 GHz      | Community |

### Monitor Mode Only (No Injection)

| Adapter          | Chipset         | USB ID      | Driver    | Band             | Status   |
| ---------------- | --------------- | ----------- | --------- | ---------------- | -------- |
| Alfa AWUS036AXML | MediaTek MT7961 | `0e8d:7961` | `mt7921u` | 2.4 + 5 + 6 GHz  | Verified |

> **Note on MT7961 / MT7921:** Some community sources report injection
> working on the USB variant (MT7921AU) with kernels 6.6+. Our testing on
> kernel 6.17 with the AWUS036AXML consistently fails `aireplay-ng --test`.
> If you have different results, please open an issue.

### Known Incompatible

These chipsets are commonly encountered but **do not** reliably support
monitor mode or injection on Linux:

| Chipset            | Why                                                                                 |
| ------------------ | ----------------------------------------------------------------------------------- |
| Realtek RTL8188EUS | No proper monitor mode. Found in TP-Link TL-WN722N **v2/v3** (not the same as v1).  |
| MediaTek MT7601U   | Cheap generic dongles. No reliable injection.                                       |
| Broadcom (any)     | Proprietary firmware, poor Linux monitor mode support.                              |
| Intel (any)        | Integrated chips. Partial monitor mode via `iwlwifi`, but injection unreliable.     |

## Notes

- **Atheros AR9271** (`ath9k_htc`) — the classic 2.4 GHz pentesting chipset.
  In-kernel since Linux 2.6.35, rock-solid injection. Still the most
  recommended budget option for 2.4 GHz work. Beware that only the **v1**
  revision of the TP-Link TL-WN722N uses this chipset; v2 and v3 switched
  to the incompatible RTL8188EUS.

- **Ralink RT3070 / RT5572** (`rt2800usb`) — reliable in-kernel driver
  family. The RT5572 adds dual-band (5 GHz) support. Becoming harder to
  find new but still widely available second-hand.

- **MediaTek MT7610U** (`mt76x0u`) — compact WiFi 5 option with good
  5 GHz injection. Lower power than the MT7612U but adequate for close-range
  work. In-kernel since Linux 4.2.

- **MediaTek MT7612U** (`mt76x2u`) — the top recommendation for WiFi 5
  pentesting. Reliable monitor mode and injection on both bands. In-kernel
  since Linux 4.19. Ensure your regulatory domain is set correctly
  (`sudo iw reg set <CC>`) to unlock 5 GHz TX on DFS channels.

- **Realtek RTL8812AU** (`rtw88`) — historically required an out-of-tree
  DKMS driver (`aircrack-ng/rtl8812au`). As of **kernel 6.14+**, the
  in-kernel `rtw88` driver supports it with monitor mode and injection.
  On older kernels, use the
  [aircrack-ng out-of-tree driver](https://github.com/aircrack-ng/rtl8812au).

- **Regulatory domain** — a default `country 00` (world) regulatory domain
  restricts TX power and may disable transmission on 5 GHz DFS channels
  entirely. Check with `iw reg get` and set your country code if needed:
  `sudo iw reg set DE`.

## Sources

- [morrownr/USB-WiFi](https://github.com/morrownr/USB-WiFi) — the most
  actively maintained USB wireless adapter resource. Includes chipset
  tables, driver status, and recommended adapters for security testing.
- [morrownr - Recommended Adapters for Kali](https://github.com/morrownr/USB-WiFi/blob/main/home/Recommended_Adapters_for_Kali_Linux.md)
- [Airgeddon Wiki - Cards and Chipsets](https://github.com/v1s1t0r1sh3r3/airgeddon/wiki/Cards-and-Chipsets)
  — whitelist / greylist / blacklist of tested adapters.
- [Aircrack-ng - Compatible Cards](https://aircrack-ng.org/doku.php?id=compatible_cards)
  — the canonical reference, though less actively maintained than morrownr.
- [Aircrack-ng - Compatibility Drivers](https://aircrack-ng.org/doku.php?id=compatibility_drivers)
- [Kali Linux - Wireless Cards (NetHunter)](https://www.kali.org/docs/nethunter/wireless-cards/)

## Contributing

If you have tested an adapter not listed here, please open an issue or PR
with the adapter name, chipset, USB ID, driver, kernel version, and
`aireplay-ng --test` results.
