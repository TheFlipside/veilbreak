# Veilbreak — dev task runner
#
# Install just:        cargo install just      (or: apt install just / pacman -S just)
# List all recipes:    just            (the default recipe)
# Run a recipe:        just <name>

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Default: list all recipes.
default:
    @just --list

# Format check + clippy with warnings as errors.
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings

# Apply rustfmt in place.
fmt:
    cargo fmt --all

# Run all tests across the workspace.
test:
    cargo test --all

# Lint + test. Run before pushing.
check: lint test

# Run the TUI live (needs root for monitor mode + injection).
run:
    sudo -E cargo run -p veilbreak-tui

# Run the TUI against a captured pcap; no root, no live capture.
replay PCAP:
    cargo run -p veilbreak-tui -- --replay {{PCAP}}

# Build a release tarball under target/dist/.
package:
    cargo build --release
    mkdir -p target/dist
    tar czf target/dist/veilbreak-$(git describe --tags --always --dirty).tar.gz \
        -C target/release veilbreak-tui

# Remove build artifacts.
clean:
    cargo clean
    rm -rf target/dist

# Tail the most recent capture session log (assumes default session dir).
logs:
    @latest=$(ls -1dt ~/.local/share/veilbreak/sessions/*/ 2>/dev/null | head -n1); \
    if [ -z "$latest" ]; then echo "no sessions found"; exit 1; fi; \
    tail -F "$latest/veilbreak.log"
