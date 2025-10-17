use crate::{AppState, Config};
use ratatui::Frame;
use ratatui::layout::Rect;

pub mod actions;
pub mod status;
// pub mod logs;
// pub mod history;

pub trait Tab {
    fn render(f: &mut Frame, area: Rect, state: &AppState, config: &Config);
}

// Re-export for convenience
pub use status::StatusTab;
// pub use actions::ActionsTab;
// pub use logs::LogsTab;
// pub use history::HistoryTab;
