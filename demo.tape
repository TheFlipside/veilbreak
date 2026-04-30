# VHS tape for recording the veilbreak demo GIF.
#
# Prerequisites:
#   1. Build:           cargo build -p veilbreak-tui
#   2. Start testlab:   sudo ./wifi-testlab/setup.sh
#   3. NOPASSWD rule:   sudo visudo -f /etc/sudoers.d/veilbreak-demo
#        <user> ALL=(root) NOPASSWD: <path-to-repo>/target/debug/veilbreak-tui
#   4. Record:          vhs demo.tape
#   5. Clean up:        sudo rm /etc/sudoers.d/veilbreak-demo

Output demo.gif
Set Shell "bash"
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Builtin Dark"
Set Padding 20
Set TypingSpeed 60ms

# ── Launch veilbreak ──────────────────────────────────────────────
Type "sudo ./target/debug/veilbreak-tui"
Enter
Sleep 3s

# ── Interface selection ───────────────────────────────────────────
Enter
Sleep 1s

# ── Band selection ────────────────────────────────────────────────
Enter
Sleep 1s

# ── Mode confirmation ─────────────────────────────────────────────
Enter
Sleep 2s

# ── Wait for APs to populate ─────────────────────────────────────
Sleep 15s

# ── Browse the list briefly so viewer sees all APs ───────────────
Down
Sleep 600ms
Down
Sleep 600ms
Down
Sleep 600ms
Down
Sleep 2s

# ── Enable hidden/revealed filter ────────────────────────────────
# This isolates the hidden AP so the reveal is front and center.
Type "f"
Sleep 1s
Enter
Sleep 1s
Escape
Sleep 2s

# ── Deauth to force SSID reveal ─────────────────────────────────
# The hidden AP is now the only entry, already selected.
Type "d"
Sleep 1s

# Select the specific client (Down from broadcast).
Down
Sleep 500ms
Enter
Sleep 1s

# ── Wait for the client to reassociate and SSID to be revealed ──
# tshark catches the probe response and the SSID turns green.
Sleep 10s

# ── Hold on the revealed SSID so the viewer can appreciate it ───
Sleep 5s

# ── Quit ──────────────────────────────────────────────────────────
Type "q"
Sleep 1s
