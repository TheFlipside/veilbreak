use std::io::{self, Read as _};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
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
}

fn random_hex() -> Result<String> {
    use std::fmt::Write;
    let mut buf = [0u8; 8];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    let mut hex = String::with_capacity(16);
    for b in buf {
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn init_logging() -> Result<()> {
    if std::env::var_os("RUST_LOG").is_none() {
        return Ok(());
    }

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open("/tmp/veilbreak.log")?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(Mutex::new(file))
        .init();

    Ok(())
}

async fn run_tui(replay: Option<String>) -> Result<()> {
    let output_dir = std::env::temp_dir().join(format!(
        "veilbreak-{}-{}",
        std::process::id(),
        random_hex()?
    ));
    std::fs::DirBuilder::new().mode(0o700).create(&output_dir)?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app::run(&mut terminal, replay, &output_dir).await;

    let _ = terminal.show_cursor();
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_logging()?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        original_hook(info);
    }));

    enable_raw_mode()?;
    let result = run_tui(cli.replay).await;
    restore_terminal();

    result
}
