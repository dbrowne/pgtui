use super::Tab;
use crate::{AppState, Config};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

pub struct HistoryTab;

impl Tab for HistoryTab {
    fn render(f: &mut Frame, area: Rect, state: &AppState, _config: &Config) {
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
}
