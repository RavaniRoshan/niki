pub mod agents;
pub mod artifacts;
pub mod chat;
pub mod config;
pub mod cost;
pub mod diff;
pub mod fleet;
pub mod help;
pub mod history;
pub mod pipeline;
pub mod run;
pub mod session;
pub mod test_log;
pub mod verdict;

use std::collections::HashMap;

use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

// Re-export canonical types from state.rs so pages keep working with `super::X`.
pub use crate::display::state::{
    AppState, ChatLine, HoverTarget, Modal, PageId, RunState, StageInfo, StageStatus,
};

pub trait Page {
    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState);
    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool;
    fn title(&self) -> &str;
}

pub struct PageRouter {
    pub pages: HashMap<PageId, Box<dyn Page>>,
}

impl Default for PageRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl PageRouter {
    pub fn new() -> Self {
        let mut pages: HashMap<PageId, Box<dyn Page>> = HashMap::new();
        pages.insert(PageId::Run, Box::new(run::RunPage::new()));
        pages.insert(PageId::Pipeline, Box::new(pipeline::PipelinePage::new()));
        pages.insert(PageId::Agents, Box::new(agents::AgentsPage::new()));
        pages.insert(PageId::Diff, Box::new(diff::DiffPage::new()));
        pages.insert(PageId::Verdict, Box::new(verdict::VerdictPage::new()));
        pages.insert(PageId::Cost, Box::new(cost::CostPage::new()));
        pages.insert(PageId::Artifacts, Box::new(artifacts::ArtifactsPage::new()));
        pages.insert(PageId::History, Box::new(history::HistoryPage::new()));
        pages.insert(PageId::Config, Box::new(config::ConfigPage::new()));
        pages.insert(PageId::Help, Box::new(help::HelpPage::new()));
        pages.insert(PageId::TestLog, Box::new(test_log::TestLogPage::new()));
        pages.insert(PageId::Chat, Box::new(chat::ChatPage::new()));
        // Fleet and Session are rendered via their own render functions, not Page trait
        // (they use the new event/mission stores directly)
        Self { pages }
    }

    pub fn render_current(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if let Some(page) = self.pages.get(&state.current_page) {
            page.render(frame, area, state);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        if let Some(page) = self.pages.get_mut(&state.current_page) {
            page.handle_key(key, state)
        } else {
            false
        }
    }
}
