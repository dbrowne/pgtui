use crate::{AppState, Config};
use ratatui::Frame;
use ratatui::layout::Rect;

pub mod actions;
pub mod history;
pub mod logs;
pub mod status;

pub trait Tab {
    fn render(f: &mut Frame, area: Rect, state: &AppState, config: &Config);
}

// Re-export for convenience
pub use actions::ActionsTab;
pub use history::HistoryTab;
pub use logs::LogsTab;
pub use status::StatusTab;
