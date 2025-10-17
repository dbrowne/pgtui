use crate::{ActionProgress, ActionType, App, Config, LogLevel, PopupType, ui::ui};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{Terminal, backend::Backend};
use std::{process::Command, time::Duration};

pub async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    refresh_rx: &mut tokio::sync::mpsc::Receiver<()>,
    config: &Config,
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
