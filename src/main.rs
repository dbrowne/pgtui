use anyhow::Result;
use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pgtui::{App, Config, LogLevel, misc, runner};

use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::time;

// Global config (loaded once at startup) - keep this for UI functions
static CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration from environment
    let config = Config::from_env()?;
    CONFIG
        .set(config.clone())
        .expect("Failed to set global config");

    // Create a sample .env file if it doesn't exist
    if !std::path::Path::new(".env").exists() {
        misc::create_sample_env_file()?;
        println!("Created sample .env file. Please review and adjust the settings.");
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Clear the terminal and hide cursor
    terminal.clear()?;
    terminal.hide_cursor()?;

    // Create app - pass config reference
    let mut app = App::new(&config).await?;
    app.add_log(
        LogLevel::Info,
        "Database Controller TUI started".to_string(),
    );
    app.add_log(
        LogLevel::Info,
        format!(
            "Database: {}@{}:{}/{}",
            config.db_user, config.db_host, config.db_port, config.db_name
        ),
    );

    // Log if local docker files were created
    if std::path::Path::new("docker-env.sh").exists()
        && std::path::Path::new(".env.docker").exists()
        && std::path::Path::new("docker-compose.yml").exists()
    {
        app.add_log(
            LogLevel::Success,
            "Using local Docker configuration files".to_string(),
        );
    }

    // Initial status refresh - pass config
    app.refresh_status(&config).await;

    // Create a channel for async refresh
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // Spawn refresh task
    let refresh_tx = tx.clone();
    let refresh_handle = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let _ = refresh_tx.send(()).await;
        }
    });

    // Main loop - pass config
    let res = runner::run_app(&mut terminal, &mut app, &mut rx, &config).await;

    // Cleanup - this is important!
    refresh_handle.abort();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        crossterm::cursor::Show
    )?;
    terminal.show_cursor()?;

    // Clear any remaining escape sequences
    println!("\r\n");

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}
