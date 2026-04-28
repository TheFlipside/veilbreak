# CLAUDE.md — Veilbreak

Project context for Claude Code. Read this before making changes.

---

## What this is

Veilbreak is a Rust TUI tool that orchestrates `airodump-ng`, `tshark`, and
`aireplay-ng` to reveal hidden WiFi SSIDs. It replaces a multi-terminal manual
workflow with a single keyboard-driven dashboard. See `design.md` at the repo
root for the full design rationale — that document is the source of truth for
architectural decisions; this file is for working conventions.

---

## Workspace layout

```text
crates/
├── core/    # Business logic, subprocess management, parsers. NO UI deps.
└── tui/     # ratatui app. Consumes core::AppEvent, dispatches core::Action.
justfile     # Dev task runner (lint, test, package). Run with `just <recipe>`.
```

**Hard rule:** `core` must not depend on `ratatui`, `crossterm`, or anything
UI-related. This separation is non-negotiable — it's what makes `core`
testable against pcap fixtures and keeps a future GUI front-end possible.
If a dependency feels like it might cross the line, it crosses the line.

---

## Stack

| Concern         | Crate                                |
| --------------- | ------------------------------------ |
| TUI             | `ratatui` + `crossterm`              |
| Async           | `tokio` (full features)              |
| Subprocesses    | `tokio::process`                     |
| JSON parsing    | `serde` + `serde_json`               |
| CSV parsing     | `csv`                                |
| CLI             | `clap` (derive)                      |
| Logging         | `tracing` + `tracing-subscriber`     |
| Errors (lib)    | `thiserror`                          |
| Errors (bin)    | `anyhow`                             |

When adding a new dependency: prefer crates already in the tree, justify the
addition in the commit message, and keep features minimal (`default-features
= false` where reasonable).

---

## Quality bar

- `cargo fmt --all -- --check` must pass.
- `cargo clippy --all-targets --all-features -- -D warnings` must pass.
  Lints enabled at the workspace level: `clippy::pedantic`, `clippy::nursery`.
  Allow lints individually with a `#[allow(...)]` and a comment explaining why
  — never silence them in `Cargo.toml` or with blanket `#[allow]` at the
  crate level.
- `cargo test --all` must pass.
- No `unwrap()` or `expect()` in non-test code. Use `?` with a proper error
  type, or `unwrap_or_else` / `unwrap_or_default` with intent.
- No `println!` / `eprintln!` outside `main.rs` and tests. Use `tracing`.
- Every public item in `core` has a doc comment.

The post-edit lint hook in the user's global Claude Code config covers
clippy and fmt — let it run, don't bypass it.

---

## Error handling

- `core` defines its errors with `thiserror` per module (e.g.
  `core::airodump::Error`, `core::tshark::Error`) and re-exports a top-level
  `core::Error` enum that wraps them.
- `tui` and `main.rs` use `anyhow::Result` for ergonomics.
- Errors that cross the `AppEvent` boundary become `AppEvent::Error(String)`
  with a human-readable message — the TUI is not the place for full error
  chains. Log the full chain via `tracing::error!` at the boundary.

---

## Async conventions

- The app event loop is a single `tokio::select!` over receivers. Keep it
  that way — a single consumer mutating state linearly is easier to reason
  about than locks.
- Subprocess controllers are owned tasks that hold the `Child` and emit
  events via an `mpsc::Sender<AppEvent>`. They expose a small command API
  (`stop()`, `pause()`, etc.) via a separate `mpsc` going the other way.
- `Drop` impls on subprocess controllers must kill the child. Orphaned
  `airodump-ng` processes are very annoying to debug. Test this.
- No `tokio::spawn` without storing the `JoinHandle` somewhere reachable
  from app shutdown. Detached tasks are forbidden.

---

## Subprocess interface contracts

These are the parsing assumptions Veilbreak relies on. If they change,
update the parsers and add a fixture test.

- **`airodump-ng -w <prefix> --output-format pcap,csv`** — produces
  `<prefix>-01.cap` (pcap) and `<prefix>-01.csv` (live AP + client CSV,
  rewritten frequently). The CSV has two sections separated by a blank
  line: APs first, then "Station MAC" header followed by clients. Parser
  must handle the file being mid-write (truncated last line is normal).
- **`tshark -i <iface> -r <pcap> -Y <filter> -T ek`** — emits one JSON
  object per line, NDJSON-style. Each line is either an `index` record or a
  packet record. Skip `index` records. Field names use the dotted form
  (e.g. `layers.wlan.wlan_wlan_bssid`).
- **`aireplay-ng --deauth N -a <bssid> [-c <client>] <iface>`** — exits
  after sending the requested frames. Capture stderr; non-zero exit usually
  means the interface isn't in monitor mode or doesn't support injection.

Test fixtures for each live in `tests/fixtures/` and are referenced by
`core` unit tests. Do not add new dependencies to fetch live data in tests.

---

## Privilege and safety

- Veilbreak runs as root in v1. Code must still be defensive — no shell
  string interpolation when spawning processes, ever. Use `Command::arg()`
  with separate arguments. BSSIDs and interface names from external sources
  (CSV, pcap) must be validated against a strict regex before being passed
  back into `Command::arg()`.
- BSSID validation: `^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$`.
- Interface name validation: `^[a-zA-Z0-9_-]{1,15}$`.
- Never write to `/etc`, `/var`, or anywhere outside the user's session
  directory and the configured output directory. Veilbreak modifies
  interface state via `iw` / `ip`, nothing else.
- Single-card mode triggers a NetworkManager takedown. The code path that
  does this must also have a corresponding restore path in `Drop` — losing
  host connectivity because Veilbreak panicked is unacceptable.

---

## Testing

- `core` has unit tests against fixture pcaps and CSVs in `tests/fixtures/`.
- Integration tests in `crates/core/tests/` exercise the parsers
  end-to-end against fixtures.
- TUI rendering uses `ratatui::backend::TestBackend` for snapshot tests of
  key states (empty dashboard, populated AP list, modal open).
- No tests should require root or a real wireless card. If a test needs a
  subprocess, mock it at the parser boundary — the parser eats stdout, so
  tests feed canned stdout.

Run everything with `cargo test --all`.

---

## Useful commands

Common tasks live in the `justfile`. Run `just --list` to see all recipes.

```bash
# Listed recipes
just                 # alias for `just --list`
just lint            # cargo fmt --check + cargo clippy -D warnings
just test            # cargo test --all
just package         # release build + tarball
just run             # cargo run -p veilbreak-tui (live, needs sudo)
just replay <pcap>   # cargo run -p veilbreak-tui -- --replay <pcap>

# Direct cargo (when you don't want to go through just)
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
sudo -E cargo run -p veilbreak-tui
cargo run -p veilbreak-tui -- --replay tests/fixtures/sample.pcap
```

The post-edit lint hook in the global Claude Code config runs the
underlying `cargo fmt` and `cargo clippy` directly, not via `just`, so
recipe naming changes won't break the hook.

---

## Conventions for changes

- **Never** rewrite a working module wholesale to "clean it up." Targeted
  edits only. If a module needs a rewrite, open an issue first.
- New `AppEvent` variants need: a producer that emits them, a handler in
  the app loop, and a test covering the state transition.
- Module-level docs (`//!`) at the top of every file in `core` explaining
  what the module owns. One paragraph is enough.
- Commits follow Conventional Commits (`feat:`, `fix:`, `refactor:`,
  `docs:`, `test:`, `chore:`). Scope is the crate name where useful:
  `feat(core): parse association request frames`.
- Branches off `main`, no force-push to `main`.

---

## Things not to do

- Do not introduce a parallel `tshark` capture. `airodump-ng` is the sole
  capture process. `tshark` only reads the pcap airodump produces.
- Do not add a "kill all interfering processes" path equivalent to
  `airmon-ng check kill`. Dual-card mode exists specifically to avoid this.
  Single-card mode warns the user but does not kill NetworkManager from
  inside Veilbreak — the user opts in by accepting the warning, and the
  takedown is via the documented `nmcli device set ... managed no` path
  with a guaranteed restore on exit.
- Do not log raw frame contents at `info` level. Capture data is sensitive
  even in a personal lab. `debug` and below only.
- Do not embed credentials, MACs, or SSIDs from real networks in test
  fixtures. Generate synthetic pcaps or use clearly-fake values
  (`AA:BB:CC:DD:EE:FF` style).

---

## When in doubt

- Check `design.md` for architectural intent.
- Use `/scout` to explore before making changes to unfamiliar modules.
- Use `/review` on diffs before commit.
- Use `/audit` before merging anything that touches subprocess spawning,
  privilege, or interface state.
