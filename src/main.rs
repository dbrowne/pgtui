use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pgtui::ui::ui;
use pgtui::{ActionProgress, ActionType, App, Config, LogLevel, PopupType};

use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
};
use std::{io, process::Command, time::Duration};
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
    let res = run_app(&mut terminal, &mut app, &mut rx, &config).await;

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

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    refresh_rx: &mut tokio::sync::mpsc::Receiver<()>,
    config: &Config, // Add config parameter
) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(50); // Fast tick for smooth animation
    let mut action_progress_rx: Option<tokio::sync::mpsc::Receiver<ActionProgress>> = None;

    loop {
        terminal.draw(|f| ui(f, app, config))?;

        // Check for action progress updates
        if let Some(ref mut rx) = action_progress_rx {
            if let Ok(progress) = rx.try_recv() {
                match progress {
                    ActionProgress::Started => {
                        // Already showing loading popup
                    }
                    ActionProgress::Update(msg) => {
                        {
                            let mut state = app.state.lock().unwrap();
                            state.show_popup = Some(PopupType::Loading(msg.clone()));
                        } // Drop the lock before calling add_log
                        app.add_log(LogLevel::Info, msg);
                    }
                    ActionProgress::Completed(msg) => {
                        {
                            let mut state = app.state.lock().unwrap();
                            state.is_loading = false;
                            state.show_popup = Some(PopupType::Success(msg.clone()));
                        } // Drop the lock before calling add_log
                        app.add_log(LogLevel::Success, msg);
                        action_progress_rx = None;
                    }
                    ActionProgress::Failed(msg) => {
                        {
                            let mut state = app.state.lock().unwrap();
                            state.is_loading = false;
                            state.show_popup = Some(PopupType::Error(msg.clone()));
                        } // Drop the lock before calling add_log
                        app.add_log(LogLevel::Error, msg);
                        action_progress_rx = None;
                    }
                }
            }
        }

        // Handle events with timeout for animation
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        tokio::select! {
            _ = refresh_rx.recv() => {
                if !app.state.lock().unwrap().is_loading {
                    app.refresh_status(config).await;  // Pass config
                }
            }
            _ = tokio::time::sleep(timeout) => {
                last_tick = std::time::Instant::now();
            }
        }

        if event::poll(Duration::from_millis(0))? {
            let mut state = app.state.lock().unwrap();

            if let Event::Key(key) = event::read()? {
                // Handle popup input first
                if let Some(ref popup) = state.show_popup.clone() {
                    match popup {
                        PopupType::Confirm(_, action_type) => {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let action = action_type.clone();

                                    // Show loading popup
                                    let loading_msg = match &action {
                                        ActionType::GenerateDocs => {
                                            "Starting documentation generation..."
                                        }
                                        ActionType::StartServices => "Starting Docker services...",
                                        ActionType::StopServices => "Stopping Docker services...",
                                        ActionType::CleanAll => {
                                            "Cleaning all containers and volumes..."
                                        }
                                        ActionType::DeleteVolumes => {
                                            "Deleting volumes and networks..."
                                        }
                                        ActionType::ViewDocs => "Opening documentation...",
                                    };

                                    state.show_popup =
                                        Some(PopupType::Loading(loading_msg.to_string()));
                                    state.is_loading = true;
                                    state.loading_message = loading_msg.to_string();
                                    drop(state);

                                    // Start async action execution - pass cloned config
                                    action_progress_rx =
                                        Some(app.execute_action_async(action, config.clone()));
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    state.show_popup = None;
                                }
                                _ => {}
                            }
                        }
                        PopupType::Loading(_) => {
                            // Don't allow interaction during loading
                        }
                        _ => {
                            if !state.is_loading {
                                state.show_popup = None;
                            }
                        }
                    }
                    continue;
                }

                // Don't process other input if loading
                if state.is_loading {
                    continue;
                }

                // Normal input handling
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('?') => {
                        state.show_popup = Some(PopupType::Help);
                    }
                    KeyCode::Char('r') => {
                        drop(state);
                        app.refresh_status(config).await; // Pass config
                    }
                    KeyCode::Tab => {
                        state.selected_tab = (state.selected_tab + 1) % 4;
                    }
                    KeyCode::BackTab => {
                        state.selected_tab = if state.selected_tab == 0 {
                            3
                        } else {
                            state.selected_tab - 1
                        };
                    }
                    KeyCode::Up => {
                        if state.selected_tab == 1 && state.selected_action > 0 {
                            state.selected_action -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if state.selected_tab == 1 && state.selected_action < 8 {
                            state.selected_action += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if state.selected_tab == 1 {
                            let action = match state.selected_action {
                                0 => ActionType::StartServices,
                                1 => ActionType::StopServices,
                                2 => ActionType::CleanAll,
                                3 => ActionType::DeleteVolumes,
                                4 => ActionType::GenerateDocs,
                                5 => ActionType::ViewDocs,
                                6 => {
                                    drop(state);
                                    app.refresh_status(config).await; // Pass config
                                    continue;
                                }
                                7 => {
                                    drop(state);
                                    let config = CONFIG.get().expect("Config not initialized");
                                    // Try to open psql in a new terminal window
                                    let psql_cmd = format!(
                                        "docker exec -it {} psql -U {} -d {}",
                                        config.container_name, config.db_user, config.db_name
                                    );

                                    let terminal_opened =
                                        // Try gnome-terminal
                                        Command::new("gnome-terminal")
                                            .args(&["--", "bash", "-c", &format!("{} ; read -p 'Press Enter to close...'", psql_cmd)])
                                            .spawn()
                                            .is_ok()
                                            // Try konsole (KDE)
                                            || Command::new("konsole")
                                            .args(&["-e", "bash", "-c", &format!("{} ; read -p 'Press Enter to close...'", psql_cmd)])
                                            .spawn()
                                            .is_ok()
                                            // Try xfce4-terminal
                                            || Command::new("xfce4-terminal")
                                            .args(&["-e", &format!("bash -c '{} ; read -p \"Press Enter to close...\"'", psql_cmd)])
                                            .spawn()
                                            .is_ok()
                                            // Try xterm as fallback
                                            || Command::new("xterm")
                                            .args(&["-e", &format!("bash -c '{} ; read -p \"Press Enter to close...\"'", psql_cmd)])
                                            .spawn()
                                            .is_ok()
                                            // Try kitty
                                            || Command::new("kitty")
                                            .args(&["bash", "-c", &format!("{} ; read -p 'Press Enter to close...'", psql_cmd)])
                                            .spawn()
                                            .is_ok()
                                            // Try alacritty
                                            || Command::new("alacritty")
                                            .args(&["-e", "bash", "-c", &format!("{} ; read -p 'Press Enter to close...'", psql_cmd)])
                                            .spawn()
                                            .is_ok();

                                    if terminal_opened {
                                        app.add_log(
                                            LogLevel::Success,
                                            "Opened psql in new terminal window".to_string(),
                                        );
                                    } else {
                                        app.add_log(
                                            LogLevel::Error,
                                            format!(
                                                "Could not open terminal. Please run manually: docker exec -it {} psql -U {} -d {}",
                                                config.container_name, config.db_user, config.db_name
                                            ),
                                        );
                                    }
                                    continue;
                                }
                                8 => {
                                    drop(state);
                                    let config = CONFIG.get().expect("Config not initialized");
                                    // Open Docker logs in new terminal

                                    // Build the docker logs command using configured docker-compose command
                                    let logs_cmd = format!(
                                        "{} logs -f {}",
                                        config.docker_compose_command, config.compose_service
                                    );

                                    let script = format!(
                                        "echo '=== Docker Container Logs ==='; \
                                        echo 'Press Ctrl+C to stop following logs...'; \
                                        echo ''; \
                                        {}; \
                                        echo ''; \
                                        echo 'Logs stopped. Press Enter to close this window...'; \
                                        read",
                                        logs_cmd
                                    );

                                    let terminal_opened = Command::new("gnome-terminal")
                                        .args(&[
                                            "--title",
                                            "Docker Logs",
                                            "--",
                                            "bash",
                                            "-c",
                                            &script,
                                        ])
                                        .spawn()
                                        .is_ok()
                                        || Command::new("konsole")
                                            .args(&[
                                                "--title",
                                                "Docker Logs",
                                                "-e",
                                                "bash",
                                                "-c",
                                                &script,
                                            ])
                                            .spawn()
                                            .is_ok()
                                        || Command::new("xfce4-terminal")
                                            .args(&[
                                                "--title",
                                                "Docker Logs",
                                                "-e",
                                                &format!("bash -c '{}'", script),
                                            ])
                                            .spawn()
                                            .is_ok()
                                        || Command::new("xterm")
                                            .args(&[
                                                "-title",
                                                "Docker Logs",
                                                "-e",
                                                "bash",
                                                "-c",
                                                &script,
                                            ])
                                            .spawn()
                                            .is_ok()
                                        || Command::new("kitty")
                                            .args(&[
                                                "--title",
                                                "Docker Logs",
                                                "bash",
                                                "-c",
                                                &script,
                                            ])
                                            .spawn()
                                            .is_ok()
                                        || Command::new("alacritty")
                                            .args(&[
                                                "--title",
                                                "Docker Logs",
                                                "-e",
                                                "bash",
                                                "-c",
                                                &script,
                                            ])
                                            .spawn()
                                            .is_ok();

                                    if terminal_opened {
                                        app.add_log(
                                            LogLevel::Success,
                                            "Opened Docker logs in new terminal (Ctrl+C to stop)"
                                                .to_string(),
                                        );
                                    } else {
                                        app.add_log(
                                            LogLevel::Error,
                                            "Could not open terminal for logs.".to_string(),
                                        );
                                    }
                                    continue;
                                }
                                _ => continue,
                            };

                            let msg = match action {
                                ActionType::StartServices => "Start database services?",
                                ActionType::StopServices => "Stop all services?",
                                ActionType::CleanAll => "Remove all containers and volumes?",
                                ActionType::DeleteVolumes => {
                                    "Delete all volumes? This will remove all data!"
                                }
                                ActionType::GenerateDocs => "Generate database documentation?",
                                ActionType::ViewDocs => "Open documentation in browser?",
                            };

                            state.show_popup = Some(PopupType::Confirm(msg.to_string(), action));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
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
