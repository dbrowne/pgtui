use crate::{App, AppState, Config, LogLevel, PopupType};
use chrono::Local;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap,
};

pub fn ui(f: &mut Frame, app: &App, config: &Config) {
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
        0 => render_status_tab(f, chunks[1], &state, config),
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
fn render_status_tab(f: &mut Frame, area: Rect, state: &AppState, config: &Config) {
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
