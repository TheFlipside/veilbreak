# wifi-testlab

Virtual WiFi lab for developing and testing veilbreak without physical
hardware. Uses `mac80211_hwsim` to create virtual 802.11 radios — one
hidden AP, one associated client, and one monitor interface for veilbreak.

See [GUIDE.md](GUIDE.md) for detailed usage instructions and architecture.

## Prerequisites

- Linux with `mac80211_hwsim` module (bare-metal or VM, not WSL)
- `hostapd`, `wpa_supplicant`, `iw`, `ip`
- Root access

```bash
# Debian / Ubuntu
sudo apt install hostapd wpasupplicant iw linux-modules-extra-$(uname -r)

# Arch
sudo pacman -S hostapd wpa_supplicant iw
```

## Usage

```bash
sudo ./setup.sh              # start the lab
sudo ./verify.sh             # check all components are healthy

sudo cargo run -p veilbreak-tui   # run veilbreak from repo root
# → select the monitor interface printed by setup.sh
# → select 2.4 GHz band

sudo ./setup.sh --down       # stop and clean up
```

## Files

| File                           | Purpose                           |
| ------------------------------ | --------------------------------- |
| `setup.sh`                     | Lab lifecycle (up/down/status)    |
| `verify.sh`                    | Health check (5 assertions)       |
| `configs/hostapd.conf`         | Hidden AP settings (editable)     |
| `configs/wpa_supplicant.conf`  | Client settings (editable)        |
| `GUIDE.md`                     | Full walkthrough and architecture |
