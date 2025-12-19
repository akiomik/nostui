pub mod event_source;
pub mod real;
pub mod test;

use std::future::Future;
use std::io::{stdout, Stdout};
use std::pin::Pin;

use color_eyre::eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::prelude::*;
use serde::{Deserialize, Serialize};

pub type IO = Stdout;
pub fn io() -> IO {
    stdout()
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
    fn resize(&mut self, area: Rect) -> Result<()>;
    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<Event>> + Send + '_>>;
}
