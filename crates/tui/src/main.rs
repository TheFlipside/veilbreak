use std::io;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use tracing_subscriber::EnvFilter;

mod app;
mod input;
mod theme;
mod ui;
mod widgets;

/// Veilbreak — reveal hidden `WiFi` SSIDs from one screen.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Replay a captured pcap instead of live capture.
    #[arg(long, value_name = "PCAP")]
    replay: Option<String>,
    /// Wi-Fi band for scanning: bg (2.4 GHz), a (5 GHz), or abg (both).
    /// Skips the band selection prompt when specified.
    #[arg(long, value_name = "BAND")]
    band: Option<veilbreak_core::Band>,
    /// Directory for session output (captures, logs). Created automatically if omitted.
    #[arg(long, value_name = "DIR")]
    output_dir: Option<PathBuf>,
}

/// Atomically creates a temp directory using `mkdtemp(3)`, avoiding the
/// TOCTOU race inherent in separate path-generation + `mkdir` calls.
fn create_temp_dir() -> Result<PathBuf> {
    let template = std::env::temp_dir().join("veilbreak-XXXXXX");
    let mut template_bytes = template.into_os_string().into_vec();
    template_bytes.push(0); // NUL-terminate for C
    let ptr = template_bytes.as_mut_ptr().cast::<libc::c_char>();
    // SAFETY: `template_bytes` is a valid NUL-terminated C string with a
    // writable XXXXXX suffix. `mkdtemp` replaces the suffix atomically and
    // creates the directory with mode 0700.
    let result = unsafe { libc::mkdtemp(ptr) };
    if result.is_null() {
        anyhow::bail!("mkdtemp failed: {}", std::io::Error::last_os_error());
    }
    template_bytes.pop(); // remove NUL terminator
    Ok(PathBuf::from(std::ffi::OsString::from_vec(template_bytes)))
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn resolve_output_dir(user_dir: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = user_dir {
        let canonical = dir.canonicalize().map_err(|e| {
            anyhow::anyhow!("--output-dir cannot be resolved: {}: {e}", dir.display())
        })?;
        anyhow::ensure!(
            canonical.is_dir(),
            "--output-dir must be an existing directory: {}",
            canonical.display()
        );
        Ok(canonical)
    } else {
        create_temp_dir()
    }
}

fn init_logging(output_dir: &Path) -> Result<()> {
    if std::env::var_os("RUST_LOG").is_none() {
        return Ok(());
    }

    let path = output_dir.join("veilbreak.log");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(Mutex::new(file))
        .init();

    Ok(())
}

async fn run_tui(
    replay: Option<String>,
    band: Option<veilbreak_core::Band>,
    output_dir: &Path,
) -> Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app::run(&mut terminal, replay, band, output_dir).await;

    let _ = terminal.show_cursor();
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let output_dir = resolve_output_dir(cli.output_dir)?;
    init_logging(&output_dir)?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        original_hook(info);
    }));

    enable_raw_mode()?;
    let result = run_tui(cli.replay, cli.band, &output_dir).await;
    restore_terminal();

    let safe_path =
        veilbreak_core::validate::sanitize_display_string(&output_dir.display().to_string());
    eprintln!("session output: {safe_path}");

    result
}
