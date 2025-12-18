use crate::infrastructure::tui;
use std::collections::VecDeque;

pub enum EventSource {
    Real(std::sync::Arc<tokio::sync::Mutex<dyn tui::TuiLike + Send>>),
    Test(VecDeque<tui::Event>),
}

impl EventSource {
    pub fn real(tui: std::sync::Arc<tokio::sync::Mutex<dyn tui::TuiLike + Send>>) -> Self {
        EventSource::Real(tui)
    }
    pub fn test(events: impl IntoIterator<Item = tui::Event>) -> Self {
        EventSource::Test(events.into_iter().collect())
    }
    pub async fn next(&mut self) -> Option<tui::Event> {
        match self {
            EventSource::Real(tui) => {
                let mut guard = tui.lock().await;
                guard.next().await
            }
            EventSource::Test(queue) => queue.pop_front(),
        }
    }
}
