use anyhow::{Context, Result};
use chrono::Local;
use crate::setup::Config;
use crate::structs::*;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::{
    env,
    os::unix::process::ExitStatusExt,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct App {
    pub state: Arc<Mutex<AppState>>,
    pub db_pool: Option<Pool<Postgres>>,
}


impl App {
    // Pass config as parameter to methods that need it
    pub async fn new(config: &Config) -> Result<Self> {
        let state = Arc::new(Mutex::new(AppState::default()));
        let db_pool = Self::try_connect_db(config).await.ok();

        Ok(Self { state, db_pool })
    }

    pub async fn try_connect_db(config: &Config) -> Result<Pool<Postgres>> {
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

    pub async fn refresh_status(&mut self, config: &Config) {
        // Update container status
        let container_status = self.check_container_status(config);
        let port_status = self.check_port_status(config);

        // Update database status
        let db_status = if let Some(pool) = &self.db_pool {
            self.fetch_db_status(pool, config)
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
            if let Ok(pool) = Self::try_connect_db(config).await {
                let status = self
                    .fetch_db_status(&pool, config)
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

    pub async fn fetch_db_status(&self, pool: &Pool<Postgres>, config: &Config) -> Result<DatabaseStatus> {
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

    pub fn check_container_status(&self, config: &Config) -> ContainerStatus {
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

    pub fn check_port_status(&self, config: &Config) -> PortStatus {
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

    pub fn execute_action(&mut self, action: ActionType, config: &Config) -> Result<String> {
        // Just copy the entire execute_action body from main.rs
        // Replace CONFIG.get() with config parameter

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

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        let mut state = self.state.lock().unwrap();
        state.logs.push(LogEntry {
            timestamp: Local::now(),
            level,
            message,
        });
    }

    pub fn execute_action_async(
        &mut self,
        action: ActionType,
        config: Config,  // Take ownership for the async task
    ) -> tokio::sync::mpsc::Receiver<ActionProgress> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let state_clone = self.state.clone();

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