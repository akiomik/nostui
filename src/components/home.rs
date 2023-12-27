use std::{collections::HashMap, fmt::format, time::Duration};

use chrono::{DateTime, Local};
use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use nostr_sdk::prelude::Event;
use ratatui::{prelude::*, widgets::*};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, Frame};
use crate::{
    action::Action,
    config::{Config, KeyBindings},
};

#[derive(Default)]
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    events: Vec<Event>,
}

impl Home {
    pub fn new() -> Self {
        Self::default()
    }

    fn format_pubkey(&self, pubkey: String) -> String {
        let len = pubkey.len();
        let heading = &pubkey[0..5];
        let trail = &pubkey[(len - 5)..len];
        format!("{}:{}", heading, trail)
    }
}

impl Component for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::ReceiveEvent(ev) => self.events.push(ev),
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let items: Vec<ListItem> = self
            .events
            .iter()
            .map(|ev| {
                let created_at = DateTime::from_timestamp(ev.created_at.as_i64(), 0)
                    .expect("Invalid created_at")
                    .with_timezone(&Local)
                    .format("%H:%m:%d");

                let mut text = Text::default();
                text.extend(Text::raw(""));
                text.extend(Text::styled(
                    self.format_pubkey(ev.pubkey.to_string()),
                    Style::default().bold(),
                ));
                text.extend(Text::raw(ev.content.clone())); // TODO: wrap line
                text.extend(Text::styled(
                    created_at.to_string(),
                    Style::default().fg(Color::Gray),
                ));
                ListItem::new(text)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title("Timeline").borders(Borders::ALL))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop);

        f.render_widget(list, area);
        Ok(())
    }
}
