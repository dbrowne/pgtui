use anyhow::Result;
use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pgtui::{App, Config, LogLevel, runner};

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
        create_sample_env_file()?;
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

fn create_sample_env_file() -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let content = r#"# Database Controller Configuration
# This TUI can work in two modes:
# 1. Local mode (default): Creates local docker files automatically
# 2. External mode: Uses existing docker setup

# Use local Docker files (true = create/use local files, false = use external)
USE_LOCAL_DOCKER=true

# External Docker configuration (only used if USE_LOCAL_DOCKER=false)
DOCKER_ENV=
DOCKER_COMPOSE_FILE=docker-compose.yml

# Database connection settings
DB_NAME=sec_master
DB_HOST=localhost
DB_PORT=6433
DB_USER=ts_user
DB_PASS=dev_pw

# Container configuration
CONTAINER_NAME=ts_pg_av_dev
NETWORK_NAME=av_network_dev
COMPOSE_SERVICE=av_timescaledb
VOLUME_PREFIX=av_

# PgAdmin configuration
PGADMIN_PORT=5050
PGADMIN_DEFAULT_EMAIL=admin@admin.com
PGADMIN_DEFAULT_PASSWORD=admin

# SchemaSpy configuration for database documentation
# Note: ~ will be expanded to your home directory
SCHEMASPY_JAR=~/local/bin/schemaspy-6.2.4.jar
POSTGRES_DRIVER=~/local/bin/postgresql-42.7.7.jar
OUTPUT_DIR=~/db_relations
"#;

    let mut file = File::create(".env.sample")?;
    file.write_all(content.as_bytes())?;

    // Also create a .env if it doesn't exist
    if !std::path::Path::new(".env").exists() {
        let mut env_file = File::create(".env")?;
        env_file.write_all(content.as_bytes())?;
    }

    Ok(())
}
