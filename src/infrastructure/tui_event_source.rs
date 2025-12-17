use crate::infrastructure::tui;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum TuiEvent {
    Quit,
    Tick,
    Render,
    Resize(u16, u16),
    Key(crossterm::event::KeyEvent),
    FocusGained,
    FocusLost,
    Paste(String),
    Mouse(crossterm::event::MouseEvent),
    Init,
    Error,
    Closed,
}

impl From<tui::Event> for TuiEvent {
    fn from(e: tui::Event) -> Self {
        match e {
            tui::Event::Quit => TuiEvent::Quit,
            tui::Event::Tick => TuiEvent::Tick,
            tui::Event::Render => TuiEvent::Render,
            tui::Event::Resize(w, h) => TuiEvent::Resize(w, h),
            tui::Event::Key(k) => TuiEvent::Key(k),
            tui::Event::FocusGained => TuiEvent::FocusGained,
            tui::Event::FocusLost => TuiEvent::FocusLost,
            tui::Event::Paste(s) => TuiEvent::Paste(s),
            tui::Event::Mouse(m) => TuiEvent::Mouse(m),
            tui::Event::Init => TuiEvent::Init,
            tui::Event::Error => TuiEvent::Error,
            tui::Event::Closed => TuiEvent::Closed,
        }
    }
}

pub enum EventSource {
    Real(std::sync::Arc<tokio::sync::Mutex<dyn tui::TuiLike + Send>>),
    Test(VecDeque<TuiEvent>),
}

impl EventSource {
    pub fn real(tui: std::sync::Arc<tokio::sync::Mutex<dyn tui::TuiLike + Send>>) -> Self {
        EventSource::Real(tui)
    }
    pub fn test(events: impl IntoIterator<Item = TuiEvent>) -> Self {
        EventSource::Test(events.into_iter().collect())
    }
    pub async fn next(&mut self) -> Option<TuiEvent> {
        match self {
            EventSource::Real(tui) => {
                let mut guard = tui.lock().await;
                guard.next().await.map(Into::into)
            }
            EventSource::Test(queue) => queue.pop_front(),
        }
    }
}
