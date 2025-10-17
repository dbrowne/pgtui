use super::Tab;
use crate::{AppState, Config};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Line, Span, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub struct StatusTab;

impl Tab for StatusTab {
    fn render(f: &mut Frame, area: Rect, state: &AppState, config: &Config) {
        {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(49), Constraint::Percentage(50)])
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
    }
}
