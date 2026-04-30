# Recording the Demo GIF

This document describes how to reproduce `demo.gif` using
[VHS](https://github.com/charmbracelet/vhs) and the
[wifi-testlab](wifi-testlab/).

## Dependencies

VHS renders a virtual terminal via a headless browser and encodes frames
with ffmpeg.

### System packages

Debian / Ubuntu:

```bash
sudo apt install ffmpeg ttyd chromium
```

Arch Linux:

```bash
sudo pacman -S ffmpeg ttyd chromium
```

### Go toolchain

VHS is distributed as a Go module and requires Go **1.25.8+** to build.

If Go is not installed, grab the latest tarball from
<https://go.dev/dl/> and follow the
[install instructions](https://go.dev/doc/install):

```bash
# Example for x86_64 — check go.dev/dl for the current version.
wget https://go.dev/dl/go1.25.8.linux-amd64.tar.gz
sudo tar -C /usr/local -xzf go1.25.8.linux-amd64.tar.gz
echo 'export PATH=$PATH:/usr/local/go/bin:$HOME/go/bin' >> ~/.bashrc
source ~/.bashrc
go version
```

### VHS

```bash
go install github.com/charmbracelet/vhs@latest
```

Verify: `vhs --version`.

## Recording workflow

### 1. Build veilbreak

```bash
cargo build -p veilbreak-tui
```

### 2. Start the wifi-testlab

```bash
sudo ./wifi-testlab/setup.sh
sudo ./wifi-testlab/verify.sh   # optional — confirms all APs are up
```

This creates four virtual APs (one hidden, three visible) and an
associated client. See [wifi-testlab/README.md](wifi-testlab/README.md)
for details and prerequisites.

### 3. Allow passwordless sudo for the binary

VHS runs as your regular user but veilbreak needs root. A temporary
NOPASSWD sudoers entry avoids an ugly password prompt in the recording.

```bash
sudo visudo -f /etc/sudoers.d/veilbreak-demo
```

Add this single line (replace `<user>` with your username):

```text
<user> ALL=(root) NOPASSWD: <path-to-repo>/target/debug/veilbreak-tui
```

### 4. Record

```bash
vhs demo.tape
```

The output is written to `demo.gif` in the project root.

If the GIF looks wrong (SSID not revealed, bad timing), adjust the
`Sleep` durations in `demo.tape` and re-run. The two most sensitive
values are:

- The initial wait after entering the dashboard (~15 s) — all four APs
  need time to appear and the client needs to associate.
- The post-deauth wait (~10 s) — the client must reassociate and tshark
  must catch the probe response before the tape moves on.

### 5. Clean up

Remove the sudoers override and tear down the lab:

```bash
sudo rm /etc/sudoers.d/veilbreak-demo
sudo ./wifi-testlab/setup.sh --down
```

## Troubleshooting

### Chrome / Chromium singleton lock

If VHS fails with `Failed to create SingletonLock`, a stale lock file is
blocking the browser. Check whether it is a broken symlink:

```bash
ls -la ~/.config/google-chrome/SingletonLock
```

If the target does not exist (broken symlink from a crash or hostname
change), remove it:

```bash
rm ~/.config/google-chrome/SingletonLock
```

### Root-owned rod cache

If VHS was previously run with `sudo`, the browser cache at `/tmp/rod/`
may be owned by root. Remove it so your regular user can write to it:

```bash
sudo rm -rf /tmp/rod
```
