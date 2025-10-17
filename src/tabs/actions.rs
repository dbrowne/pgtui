use super::Tab;
use crate::AppState;
use crate::Config;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

pub struct ActionsTab;

impl Tab for ActionsTab {
    fn render(f: &mut Frame, area: Rect, state: &AppState, _config: &Config) {
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
}
