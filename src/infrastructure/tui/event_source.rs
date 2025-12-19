use std::{collections::VecDeque, sync::Arc};

use tokio::sync::Mutex;

use crate::infrastructure::tui::{self, TuiLike};

pub enum EventSource {
    Real(Arc<Mutex<dyn TuiLike + Send>>),
    Test(VecDeque<tui::Event>),
}

impl EventSource {
    pub fn real(tui: Arc<Mutex<dyn TuiLike + Send>>) -> Self {
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
