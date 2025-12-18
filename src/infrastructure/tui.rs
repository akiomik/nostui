pub mod event_source;
pub mod real;
pub mod test;

use color_eyre::eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent};

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

pub type IO = std::io::Stdout;
pub fn io() -> IO {
    std::io::stdout()
}
pub type Frame<'a> = ratatui::Frame<'a>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    Init,
    Quit,
    Error,
    Closed,
    Tick,
    Render,
    FocusGained,
    FocusLost,
    Paste(String),
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
}

pub trait TuiLike: Send {
    fn enter(&mut self) -> Result<()>;
    fn exit(&mut self) -> Result<()>;
    fn draw(&mut self, f: &mut dyn FnMut(&mut Frame<'_>)) -> Result<()>;
    fn resize(&mut self, area: ratatui::prelude::Rect) -> Result<()>;
    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<Event>> + Send + '_>>;
}
