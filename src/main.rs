use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pgtui::setup::Config;
use pgtui::structs::*;
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
};
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::{
    env, io,
    os::unix::process::ExitStatusExt,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time;

// Global config (loaded once at startup)
static CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();

struct App {
    state: Arc<Mutex<AppState>>,
    db_pool: Option<Pool<Postgres>>,
}

impl App {
    async fn new() -> Result<Self> {
        let state = Arc::new(Mutex::new(AppState::default()));
        let db_pool = Self::try_connect_db().await.ok();

        Ok(Self { state, db_pool })
    }

    async fn try_connect_db() -> Result<Pool<Postgres>> {
        let config = CONFIG.get().expect("Config not initialized");
        let connection_str = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.db_user, config.db_pass, config.db_host, config.db_port, config.db_name
        );

        PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect(&connection_str)
            .await
            .context("Failed to connect to database")
    }

    async fn refresh_status(&mut self) {
        // Update container status
        let container_status = self.check_container_status();
        let port_status = self.check_port_status();

        // Update database status
        let db_status = if let Some(pool) = &self.db_pool {
            self.fetch_db_status(pool)
                .await
                .unwrap_or_else(|_| DatabaseStatus {
                    connected: false,
                    connection_count: 0,
                    database_size: "Disconnected".to_string(),
                    tables_count: 0,
                    last_check: Local::now(),
                    version: "Unknown".to_string(),
                    uptime: "Unknown".to_string(),
                })
        } else {
            // Try to reconnect
            if let Ok(pool) = Self::try_connect_db().await {
                let status = self
                    .fetch_db_status(&pool)
                    .await
                    .unwrap_or_else(|_| DatabaseStatus {
                        connected: false,
                        connection_count: 0,
                        database_size: "Error".to_string(),
                        tables_count: 0,
                        last_check: Local::now(),
                        version: "Unknown".to_string(),
                        uptime: "Unknown".to_string(),
                    });
                self.db_pool = Some(pool);
                status
            } else {
                DatabaseStatus {
                    connected: false,
                    connection_count: 0,
                    database_size: "Not Connected".to_string(),
                    tables_count: 0,
                    last_check: Local::now(),
                    version: "Unknown".to_string(),
                    uptime: "Unknown".to_string(),
                }
            }
        };

        let mut state = self.state.lock().unwrap();
        state.container_status = container_status;
        state.port_status = port_status;
        state.db_status = db_status;
        state.last_refresh = Local::now();
    }

    async fn fetch_db_status(&self, pool: &Pool<Postgres>) -> Result<DatabaseStatus> {
        let config = CONFIG.get().expect("Config not initialized");

        // Get database size
        let size_query = "SELECT pg_size_pretty(pg_database_size($1))";
        let size: (String,) = sqlx::query_as(size_query)
            .bind(&config.db_name)
            .fetch_one(pool)
            .await?;

        // Get connection count
        let conn_query = "SELECT count(*) FROM pg_stat_activity WHERE datname = $1";
        let conn_count: (i64,) = sqlx::query_as(conn_query)
            .bind(&config.db_name)
            .fetch_one(pool)
            .await?;

        // Get table count
        let table_query =
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public'";
        let table_count: (i64,) = sqlx::query_as(table_query).fetch_one(pool).await?;

        // Get version
        let version_query = "SELECT version()";
        let version: (String,) = sqlx::query_as(version_query).fetch_one(pool).await?;

        // Get uptime
        let uptime_query = "SELECT now() - pg_postmaster_start_time()";
        let uptime_interval: (Option<sqlx::postgres::types::PgInterval>,) =
            sqlx::query_as(uptime_query).fetch_one(pool).await?;

        let uptime = if let Some(interval) = uptime_interval.0 {
            let total_seconds = interval.microseconds / 1_000_000;
            let hours = (total_seconds / 3600) % 24;
            let minutes = (total_seconds / 60) % 60;
            format!(
                "{} days, {} hours, {} minutes",
                interval.days, hours, minutes
            )
        } else {
            "Unknown".to_string()
        };

        Ok(DatabaseStatus {
            connected: true,
            connection_count: conn_count.0 as i32,
            database_size: size.0,
            tables_count: table_count.0 as i32,
            last_check: Local::now(),
            version: version.0.split(' ').take(2).collect::<Vec<_>>().join(" "),
            uptime,
        })
    }

    fn check_container_status(&self) -> ContainerStatus {
        let config = CONFIG.get().expect("Config not initialized");

        let ps_output = Command::new("docker")
            .args(&["ps", "-a", "--format", "{{.Names}}"])
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: std::process::ExitStatus::from_raw(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });

        let running_containers = String::from_utf8_lossy(&ps_output.stdout);

        let network_output = Command::new("docker")
            .args(&["network", "ls", "--format", "{{.Name}}"])
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: std::process::ExitStatus::from_raw(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });

        let networks = String::from_utf8_lossy(&network_output.stdout);

        let volume_output = Command::new("docker")
            .args(&["volume", "ls", "--format", "{{.Name}}"])
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: std::process::ExitStatus::from_raw(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });

        let volumes = String::from_utf8_lossy(&volume_output.stdout)
            .lines()
            .filter(|v| v.contains("av_"))
            .map(|s| s.to_string())
            .collect();

        ContainerStatus {
            postgres_running: running_containers.contains(&config.container_name),
            pgadmin_running: running_containers.contains("pgadmin"),
            network_exists: networks.contains(&config.network_name),
            volumes,
        }
    }

    fn check_port_status(&self) -> PortStatus {
        let config = CONFIG.get().expect("Config not initialized");

        let db_port = Command::new("lsof")
            .args(&["-i", &format!(":{}", config.db_port)])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let pgadmin_port = Command::new("lsof")
            .args(&["-i", &format!(":{}", config.pgadmin_port)])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        PortStatus {
            db_port_available: !db_port,
            pgadmin_port_available: !pgadmin_port,
        }
    }

    fn execute_action(&mut self, action: ActionType) -> Result<String> {
        let config = CONFIG.get().expect("Config not initialized");

        // Add specific logging for documentation generation
        if matches!(action, ActionType::GenerateDocs) {
            self.add_log(
                LogLevel::Info,
                "Starting database documentation generation...".to_string(),
            );
            self.add_log(
                LogLevel::Info,
                format!("Using SchemaSpy: {}", config.schemaspy_jar),
            );
            self.add_log(
                LogLevel::Info,
                format!("Output directory: {}", config.output_dir),
            );
        }

        let result = match action {
            ActionType::StartServices => {
                // Use the configured docker-compose command
                Command::new("sh")
                    .args(&["-c", &format!("{} up -d", config.docker_compose_command)])
                    .output()
                    .context("Failed to start services")?
            }
            ActionType::StopServices => Command::new("sh")
                .args(&["-c", &format!("{} down", config.docker_compose_command)])
                .output()
                .context("Failed to stop services")?,
            ActionType::CleanAll => Command::new("sh")
                .args(&["-c", &format!("{} down -v", config.docker_compose_command)])
                .output()
                .context("Failed to clean all")?,
            ActionType::DeleteVolumes => {
                // Delete specific volumes and network
                let mut success = true;
                let mut outputs = Vec::new();

                // Delete volumes
                for volume in &["av_archive_dev", "av_pgadmin_dev", "av_pg_data_dev"] {
                    let vol_result = Command::new("docker")
                        .args(&["volume", "rm", volume])
                        .output();
                    if let Ok(output) = vol_result {
                        outputs.push(String::from_utf8_lossy(&output.stdout).to_string());
                        success = success && output.status.success();
                    }
                }

                // Delete network
                let net_result = Command::new("docker")
                    .args(&["network", "rm", "-f", &config.network_name])
                    .output();
                if let Ok(output) = net_result {
                    outputs.push(String::from_utf8_lossy(&output.stdout).to_string());
                    success = success && output.status.success();
                }

                std::process::Output {
                    status: if success {
                        std::process::ExitStatus::from_raw(0)
                    } else {
                        std::process::ExitStatus::from_raw(1)
                    },
                    stdout: outputs.join("\n").into_bytes(),
                    stderr: Vec::new(),
                }
            }
            ActionType::GenerateDocs => {
                // Direct SchemaSpy command
                let schemaspy_jar = config
                    .schemaspy_jar
                    .replace("~", &std::env::var("HOME").unwrap_or_default());
                let postgres_driver = config
                    .postgres_driver
                    .replace("~", &std::env::var("HOME").unwrap_or_default());
                let output_dir = config
                    .output_dir
                    .replace("~", &std::env::var("HOME").unwrap_or_default());

                // Check if JAR files exist
                if !std::path::Path::new(&schemaspy_jar).exists() {
                    return Err(anyhow::anyhow!(
                        "SchemaSpy JAR not found at: {}",
                        schemaspy_jar
                    ));
                }
                if !std::path::Path::new(&postgres_driver).exists() {
                    return Err(anyhow::anyhow!(
                        "PostgreSQL driver not found at: {}",
                        postgres_driver
                    ));
                }

                // Create output directory if it doesn't exist
                std::fs::create_dir_all(&output_dir)
                    .context(format!("Failed to create output directory: {}", output_dir))?;

                // Clean output directory
                self.add_log(LogLevel::Info, "Cleaning output directory...".to_string());
                if let Ok(entries) = std::fs::read_dir(&output_dir) {
                    for entry in entries.flatten() {
                        if let Ok(path) = entry.path().canonicalize() {
                            if path.is_file() {
                                std::fs::remove_file(path).ok();
                            } else if path.is_dir() {
                                std::fs::remove_dir_all(path).ok();
                            }
                        }
                    }
                }

                self.add_log(
                    LogLevel::Info,
                    "Connecting to database and analyzing schema...".to_string(),
                );

                Command::new("java")
                    .args(&[
                        "-jar",
                        &schemaspy_jar,
                        "-t",
                        "pgsql11",
                        "-dp",
                        &postgres_driver,
                        "-db",
                        &config.db_name,
                        "-host",
                        &config.db_host,
                        "-port",
                        &config.db_port.to_string(),
                        "-u",
                        &config.db_user,
                        "-p",
                        &config.db_pass,
                        "-o",
                        &output_dir,
                    ])
                    .output()
                    .context(
                        "Failed to generate documentation. Check that Java is installed and database is accessible.",
                    )?
            }
            ActionType::ViewDocs => {
                // Open documentation in browser
                let output_dir = config
                    .output_dir
                    .replace("~", &std::env::var("HOME").unwrap_or_default());
                let index_file = format!("{}/index.html", output_dir);

                // Check if documentation exists
                if !std::path::Path::new(&index_file).exists() {
                    return Err(anyhow::anyhow!(
                        "Documentation not found. Please generate it first."
                    ));
                }

                // Try different browsers
                let browsers = vec!["xdg-open", "google-chrome", "firefox", "chromium", "open"];
                let mut success = false;

                for browser in browsers {
                    if Command::new(browser).arg(&index_file).spawn().is_ok() {
                        success = true;
                        break;
                    }
                }

                std::process::Output {
                    status: if success {
                        std::process::ExitStatus::from_raw(0)
                    } else {
                        std::process::ExitStatus::from_raw(1)
                    },
                    stdout: format!("Opening {}", index_file).into_bytes(),
                    stderr: if !success {
                        b"Could not open browser".to_vec()
                    } else {
                        Vec::new()
                    },
                }
            }
        };

        let output = String::from_utf8_lossy(&result.stdout).to_string();
        let error = String::from_utf8_lossy(&result.stderr).to_string();

        let mut state = self.state.lock().unwrap();
        state.command_output = output.lines().map(|s| s.to_string()).collect();

        let success = result.status.success();
        state.action_history.push(ActionHistory {
            timestamp: Local::now(),
            action: format!("{:?}", action),
            success,
        });

        if success {
            state.logs.push(LogEntry {
                timestamp: Local::now(),
                level: LogLevel::Success,
                message: format!("Action {:?} completed successfully", action),
            });
            Ok(output)
        } else {
            state.logs.push(LogEntry {
                timestamp: Local::now(),
                level: LogLevel::Error,
                message: format!("Action {:?} failed: {}", action, error),
            });
            Err(anyhow::anyhow!("Command failed: {}", error))
        }
    }

    fn add_log(&mut self, level: LogLevel, message: String) {
        let mut state = self.state.lock().unwrap();
        state.logs.push(LogEntry {
            timestamp: Local::now(),
            level,
            message,
        });
    }

    fn execute_action_async(
        &mut self,
        action: ActionType,
    ) -> tokio::sync::mpsc::Receiver<ActionProgress> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let config = CONFIG.get().expect("Config not initialized").clone();
        let state_clone = self.state.clone();

        // Spawn async task to execute the action
        tokio::spawn(async move {
            let _ = tx.send(ActionProgress::Started).await;

            match action {
                ActionType::GenerateDocs => {
                    // Send progress updates for documentation generation
                    let _ = tx
                        .send(ActionProgress::Update(
                            "Checking SchemaSpy JAR file...".to_string(),
                        ))
                        .await;
                    tokio::time::sleep(Duration::from_millis(300)).await;

                    let schemaspy_jar = config
                        .schemaspy_jar
                        .replace("~", &std::env::var("HOME").unwrap_or_default());
                    let postgres_driver = config
                        .postgres_driver
                        .replace("~", &std::env::var("HOME").unwrap_or_default());
                    let output_dir = config
                        .output_dir
                        .replace("~", &std::env::var("HOME").unwrap_or_default());

                    if !std::path::Path::new(&schemaspy_jar).exists() {
                        let _ = tx
                            .send(ActionProgress::Failed(format!(
                                "SchemaSpy JAR not found at: {}",
                                schemaspy_jar
                            )))
                            .await;
                        return;
                    }

                    let _ = tx
                        .send(ActionProgress::Update(
                            "Checking PostgreSQL driver...".to_string(),
                        ))
                        .await;
                    tokio::time::sleep(Duration::from_millis(300)).await;

                    if !std::path::Path::new(&postgres_driver).exists() {
                        let _ = tx
                            .send(ActionProgress::Failed(format!(
                                "PostgreSQL driver not found at: {}",
                                postgres_driver
                            )))
                            .await;
                        return;
                    }

                    let _ = tx
                        .send(ActionProgress::Update(
                            "Creating output directory...".to_string(),
                        ))
                        .await;
                    tokio::fs::create_dir_all(&output_dir).await.ok();
                    tokio::time::sleep(Duration::from_millis(300)).await;

                    let _ = tx
                        .send(ActionProgress::Update(
                            "Cleaning previous documentation...".to_string(),
                        ))
                        .await;

                    // Use async file operations
                    if let Ok(mut entries) = tokio::fs::read_dir(&output_dir).await {
                        while let Ok(Some(entry)) = entries.next_entry().await {
                            if let Ok(path) = entry.path().canonicalize() {
                                if path.is_file() {
                                    tokio::fs::remove_file(path).await.ok();
                                } else if path.is_dir() {
                                    tokio::fs::remove_dir_all(path).await.ok();
                                }
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;

                    let _ = tx
                        .send(ActionProgress::Update(
                            "Connecting to database...".to_string(),
                        ))
                        .await;
                    tokio::time::sleep(Duration::from_millis(500)).await;

                    let _ = tx
                        .send(ActionProgress::Update(
                            "Analyzing database schema (this may take a while)...".to_string(),
                        ))
                        .await;

                    // Execute SchemaSpy using tokio::process for async execution
                    let mut child = tokio::process::Command::new("java")
                        .args(&[
                            "-jar",
                            &schemaspy_jar,
                            "-t",
                            "pgsql11",
                            "-dp",
                            &postgres_driver,
                            "-db",
                            &config.db_name,
                            "-host",
                            &config.db_host,
                            "-port",
                            &config.db_port.to_string(),
                            "-u",
                            &config.db_user,
                            "-p",
                            &config.db_pass,
                            "-o",
                            &output_dir,
                        ])
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .spawn();

                    match child {
                        Ok(mut process) => {
                            // Poll the process while sending progress updates
                            let mut elapsed = 0;
                            loop {
                                tokio::select! {
                                    status = process.wait() => {
                                        match status {
                                            Ok(exit_status) if exit_status.success() => {
                                                let _ = tx
                                                    .send(ActionProgress::Update(
                                                        "Generating HTML pages...".to_string(),
                                                    ))
                                                    .await;
                                                tokio::time::sleep(Duration::from_millis(500)).await;
                                                let _ = tx
                                                    .send(ActionProgress::Completed(
                                                        "Documentation generated successfully!".to_string(),
                                                    ))
                                                    .await;
                                            }
                                            Ok(_) => {
                                                let _ = tx
                                                    .send(ActionProgress::Failed(
                                                        "Documentation generation failed. Check database connection and credentials.".to_string()
                                                    ))
                                                    .await;
                                            }
                                            Err(e) => {
                                                let _ = tx
                                                    .send(ActionProgress::Failed(format!(
                                                        "Failed to run SchemaSpy: {}",
                                                        e
                                                    )))
                                                    .await;
                                            }
                                        }
                                        break;
                                    }
                                    _ = tokio::time::sleep(Duration::from_secs(2)) => {
                                        elapsed += 2;
                                        let _ = tx
                                            .send(ActionProgress::Update(
                                                format!("Still analyzing... ({} seconds elapsed)", elapsed),
                                            ))
                                            .await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx
                                .send(ActionProgress::Failed(format!(
                                    "Failed to start SchemaSpy: {}",
                                    e
                                )))
                                .await;
                        }
                    }
                }
                _ => {
                    // For other actions, execute using async commands
                    let _ = tx
                        .send(ActionProgress::Update("Executing command...".to_string()))
                        .await;

                    let result = match action {
                        ActionType::StartServices => {
                            tokio::process::Command::new("sh")
                                .args(&["-c", &format!("{} up -d", config.docker_compose_command)])
                                .output()
                                .await
                        }
                        ActionType::StopServices => {
                            tokio::process::Command::new("sh")
                                .args(&["-c", &format!("{} down", config.docker_compose_command)])
                                .output()
                                .await
                        }
                        ActionType::CleanAll => {
                            tokio::process::Command::new("sh")
                                .args(&[
                                    "-c",
                                    &format!("{} down -v", config.docker_compose_command),
                                ])
                                .output()
                                .await
                        }
                        ActionType::DeleteVolumes => {
                            // Delete volumes and network
                            let mut success = true;
                            for volume in &["av_archive_dev", "av_pgadmin_dev", "av_pg_data_dev"] {
                                if let Ok(output) = tokio::process::Command::new("docker")
                                    .args(&["volume", "rm", volume])
                                    .output()
                                    .await
                                {
                                    success = success && output.status.success();
                                }
                            }
                            tokio::process::Command::new("docker")
                                .args(&["network", "rm", "-f", &config.network_name])
                                .output()
                                .await
                                .ok();

                            Ok(std::process::Output {
                                status: if success {
                                    std::process::ExitStatus::from_raw(0)
                                } else {
                                    std::process::ExitStatus::from_raw(1)
                                },
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                            })
                        }
                        ActionType::ViewDocs => {
                            let output_dir = config
                                .output_dir
                                .replace("~", &std::env::var("HOME").unwrap_or_default());
                            let index_file = format!("{}/index.html", output_dir);

                            let browsers = vec!["xdg-open", "google-chrome", "firefox", "chromium"];
                            let mut success = false;
                            for browser in browsers {
                                if tokio::process::Command::new(browser)
                                    .arg(&index_file)
                                    .spawn()
                                    .is_ok()
                                {
                                    success = true;
                                    break;
                                }
                            }

                            Ok(std::process::Output {
                                status: if success {
                                    std::process::ExitStatus::from_raw(0)
                                } else {
                                    std::process::ExitStatus::from_raw(1)
                                },
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                            })
                        }
                        _ => unreachable!(),
                    };

                    match result {
                        Ok(output) if output.status.success() => {
                            let _ = tx
                                .send(ActionProgress::Completed(
                                    "Operation completed successfully!".to_string(),
                                ))
                                .await;
                        }
                        Ok(output) => {
                            let error = String::from_utf8_lossy(&output.stderr);
                            let _ = tx
                                .send(ActionProgress::Failed(format!(
                                    "Operation failed: {}",
                                    error
                                )))
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(ActionProgress::Failed(format!("Operation failed: {}", e)))
                                .await;
                        }
                    }
                }
            }

            // Update state to mark loading as done
            let mut state = state_clone.lock().unwrap();
            state.is_loading = false;
        });

        rx
    }
}

fn ui(f: &mut Frame, app: &App) {
    let state = app.state.lock().unwrap();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Header with tabs
    let titles: Vec<Line> = vec!["Status", "Actions", "Logs", "History"]
        .iter()
        .cloned()
        .map(|t| Line::from(vec![Span::styled(t, Style::default().fg(Color::White))]))
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Database Controller "),
        )
        .select(state.selected_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, chunks[0]);

    // Main content area
    match state.selected_tab {
        0 => render_status_tab(f, chunks[1], &state),
        1 => render_actions_tab(f, chunks[1], &state),
        2 => render_logs_tab(f, chunks[1], &state),
        3 => render_history_tab(f, chunks[1], &state),
        _ => {}
    }

    // Footer
    let footer_content = if state.is_loading {
        // Show loading indicator in footer when processing
        let spinner_frames = vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame_index = (Local::now().timestamp_millis() / 100) as usize % spinner_frames.len();
        let spinner = spinner_frames[frame_index];

        Line::from(vec![
            Span::styled(
                spinner,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(&state.loading_message, Style::default().fg(Color::Cyan)),
            Span::raw(" | Last refresh: "),
            Span::styled(
                state.last_refresh.format("%H:%M:%S").to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ])
    } else {
        Line::from(vec![
            Span::raw("Tab: Switch tabs | "),
            Span::styled(
                "q",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(": Quit | "),
            Span::styled("r", Style::default().fg(Color::Green)),
            Span::raw(": Refresh | "),
            Span::styled("?", Style::default().fg(Color::Cyan)),
            Span::raw(": Help | Last refresh: "),
            Span::styled(
                state.last_refresh.format("%H:%M:%S").to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ])
    };

    let footer = Paragraph::new(footer_content).block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, chunks[2]);

    // Render popup if active
    if let Some(ref popup) = state.show_popup {
        render_popup(f, popup);
    }
}

fn render_status_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let config = CONFIG.get().expect("Config not initialized");

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(0),
        ])
        .split(chunks[0]);

    // Database Status
    let db_style = if state.db_status.connected {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red)
    };

    let db_info = vec![
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                if state.db_status.connected {
                    "Connected"
                } else {
                    "Disconnected"
                },
                db_style,
            ),
        ]),
        Line::from(format!("Database: {}", config.db_name)),
        Line::from(format!("Version: {}", state.db_status.version)),
        Line::from(format!("Size: {}", state.db_status.database_size)),
        Line::from(format!("Tables: {}", state.db_status.tables_count)),
        Line::from(format!("Connections: {}", state.db_status.connection_count)),
        Line::from(format!("Uptime: {}", state.db_status.uptime)),
        Line::from(format!(
            "Last Check: {}",
            state.db_status.last_check.format("%H:%M:%S")
        )),
    ];

    let db_status = Paragraph::new(db_info)
        .block(
            Block::default()
                .title(" Database Status ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(db_status, left_chunks[0]);

    // Container Status
    let container_items = vec![
        Line::from(vec![
            Span::raw("PostgreSQL: "),
            Span::styled(
                if state.container_status.postgres_running {
                    "Running"
                } else {
                    "Stopped"
                },
                if state.container_status.postgres_running {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
        ]),
        Line::from(vec![
            Span::raw("PgAdmin: "),
            Span::styled(
                if state.container_status.pgadmin_running {
                    "Running"
                } else {
                    "Stopped"
                },
                if state.container_status.pgadmin_running {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
        ]),
        Line::from(vec![
            Span::raw("Network: "),
            Span::styled(
                if state.container_status.network_exists {
                    "Exists"
                } else {
                    "Not Found"
                },
                if state.container_status.network_exists {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            ),
        ]),
        Line::from(format!("Volumes: {}", state.container_status.volumes.len())),
    ];

    let container_status = Paragraph::new(container_items).block(
        Block::default()
            .title(" Container Status ")
            .borders(Borders::ALL),
    );

    f.render_widget(container_status, left_chunks[1]);

    // Port Status
    let port_items = vec![
        Line::from(vec![
            Span::raw(format!("DB Port {}: ", config.db_port)),
            Span::styled(
                if state.port_status.db_port_available {
                    "Available"
                } else {
                    "In Use"
                },
                if state.port_status.db_port_available {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
        ]),
        Line::from(vec![
            Span::raw(format!("PgAdmin Port {}: ", config.pgadmin_port)),
            Span::styled(
                if state.port_status.pgadmin_port_available {
                    "Available"
                } else {
                    "In Use"
                },
                if state.port_status.pgadmin_port_available {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
        ]),
    ];

    let port_status = Paragraph::new(port_items).block(
        Block::default()
            .title(" Port Status ")
            .borders(Borders::ALL),
    );

    f.render_widget(port_status, left_chunks[2]);

    // Right side - Command Output
    let output_text: Vec<Line> = state
        .command_output
        .iter()
        .map(|s| Line::from(s.as_str()))
        .collect();

    let command_output = Paragraph::new(output_text)
        .block(
            Block::default()
                .title(" Last Command Output ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(command_output, chunks[1]);
}

fn render_actions_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let actions = vec![
        (
            "↑ Start Services",
            "Start PostgreSQL and PgAdmin containers",
        ),
        ("↓ Stop Services", "Stop all running containers"),
        ("🧹 Clean All", "Remove containers, networks, and volumes"),
        ("🗑️  Delete Volumes", "Delete all persistent volumes"),
        (
            "📚 Generate Docs",
            "Generate database documentation with SchemaSpy",
        ),
        ("🌐 View Docs", "Open database documentation in browser"),
        ("🔄 Refresh Status", "Refresh all status information"),
        (
            "💻 Open PSQL",
            "Connect to database via psql (opens new terminal)",
        ),
        (
            "📜 View Docker Logs",
            "View container logs (opens new terminal)",
        ),
    ];

    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            let content = vec![
                Line::from(Span::styled(
                    *name,
                    if i == state.selected_action {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )),
                Line::from(Span::styled(
                    format!("  {}", desc),
                    Style::default().fg(Color::Gray),
                )),
            ];
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Available Actions (Enter to execute, ↑/↓ to navigate) ")
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_widget(list, area);
}

fn render_logs_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let logs: Vec<ListItem> = state
        .logs
        .iter()
        .rev()
        .take(50)
        .map(|log| {
            let style = match log.level {
                LogLevel::Info => Style::default().fg(Color::White),
                LogLevel::Warning => Style::default().fg(Color::Yellow),
                LogLevel::Error => Style::default().fg(Color::Red),
                LogLevel::Success => Style::default().fg(Color::Green),
            };

            let content = Line::from(vec![
                Span::styled(
                    log.timestamp.format("[%H:%M:%S] ").to_string(),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(
                    format!("{:?}: ", log.level),
                    style.add_modifier(Modifier::BOLD),
                ),
                Span::styled(&log.message, style),
            ]);

            ListItem::new(content)
        })
        .collect();

    let list = List::new(logs).block(
        Block::default()
            .title(" Application Logs ")
            .borders(Borders::ALL),
    );

    f.render_widget(list, area);
}

fn render_history_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(vec!["Time", "Action", "Status"])
        .style(Style::default().fg(Color::Yellow))
        .bottom_margin(1);

    let rows = state.action_history.iter().rev().take(20).map(|h| {
        let status_style = if h.success {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Red)
        };

        Row::new(vec![
            Cell::from(h.timestamp.format("%H:%M:%S").to_string()),
            Cell::from(h.action.clone()),
            Cell::from(if h.success { "Success" } else { "Failed" }).style(status_style),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(" Action History ")
            .borders(Borders::ALL),
    )
    .widths(&[
        Constraint::Length(10),
        Constraint::Min(20),
        Constraint::Length(10),
    ]);

    f.render_widget(table, area);
}

fn render_popup(f: &mut Frame, popup: &PopupType) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let (title, content, style) = match popup {
        PopupType::Confirm(msg, _) => (
            " Confirm Action ",
            vec![
                Line::from(""),
                Line::from(msg.as_str()),
                Line::from(""),
                Line::from("Press 'y' to confirm or 'n' to cancel"),
            ],
            Style::default().fg(Color::Yellow),
        ),
        PopupType::Error(msg) => (
            " Error ",
            vec![
                Line::from(""),
                Line::from(msg.as_str()),
                Line::from(""),
                Line::from("Press any key to continue"),
            ],
            Style::default().fg(Color::Red),
        ),
        PopupType::Success(msg) => (
            " Success ",
            vec![
                Line::from(""),
                Line::from(msg.as_str()),
                Line::from(""),
                Line::from("Press any key to continue"),
            ],
            Style::default().fg(Color::Green),
        ),
        PopupType::Loading(msg) => {
            // Create an animated loading indicator
            let spinner_frames = vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_index =
                (Local::now().timestamp_millis() / 100) as usize % spinner_frames.len();
            let spinner = spinner_frames[frame_index];

            (
                " Processing ",
                vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            spinner,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::raw(msg.as_str()),
                    ]),
                    Line::from(""),
                    Line::from("This may take a few moments..."),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Please wait",
                        Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::ITALIC),
                    )),
                ],
                Style::default().fg(Color::Cyan),
            )
        }
        PopupType::Help => (
            " Help ",
            vec![
                Line::from(""),
                Line::from("Keyboard Shortcuts:"),
                Line::from(""),
                Line::from("  Tab       - Switch between tabs"),
                Line::from("  ↑/↓       - Navigate actions"),
                Line::from("  Enter     - Execute selected action"),
                Line::from("  r         - Refresh status"),
                Line::from("  q         - Quit application"),
                Line::from("  ?         - Show this help"),
                Line::from(""),
                Line::from("Press any key to close"),
            ],
            Style::default().fg(Color::Cyan),
        ),
    };

    let popup = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(style),
        )
        .alignment(Alignment::Center);

    f.render_widget(popup, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

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

    // Create app
    let mut app = App::new().await?;
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

    // Initial status refresh
    app.refresh_status().await;

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

    // Main loop
    let res = run_app(&mut terminal, &mut app, &mut rx).await;

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
) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(50); // Fast tick for smooth animation
    let mut action_progress_rx: Option<tokio::sync::mpsc::Receiver<ActionProgress>> = None;

    loop {
        terminal.draw(|f| ui(f, app))?;

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
                    app.refresh_status().await;
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

                                    // Start async action execution
                                    action_progress_rx = Some(app.execute_action_async(action));
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
                        app.refresh_status().await;
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
                                    app.refresh_status().await;
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
