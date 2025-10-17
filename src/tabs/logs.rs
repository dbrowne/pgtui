use super::Tab;
use crate::{AppState, Config, LogLevel};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

pub struct LogsTab;

impl Tab for LogsTab {
    fn render(f: &mut Frame, area: Rect, state: &AppState, _config: &Config) {
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
}
