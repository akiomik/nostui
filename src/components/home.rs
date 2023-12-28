use std::{collections::HashMap, fmt::format, time::Duration};

use chrono::{DateTime, Local};
use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use nostr_sdk::prelude::Event;
use nostr_sdk::{EventId, Kind, Tag};
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
    reactions: HashMap<EventId, Vec<Event>>,
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
            Action::ReceiveEvent(ev) => match ev.kind {
                Kind::TextNote | Kind::Repost => self.events.push(ev),
                Kind::Reaction => {
                    // reactions grouped by event_id
                    let maybe_reacted_event_tag = ev
                        .tags
                        .iter()
                        .filter(|tag| match tag {
                            Tag::Event {
                                event_id,
                                relay_url,
                                marker,
                            } => true,
                            _ => false,
                        })
                        .last();
                    match maybe_reacted_event_tag {
                        Some(Tag::Event {
                            event_id,
                            relay_url,
                            marker,
                        }) => {
                            if self.reactions.contains_key(&event_id) {
                                self.reactions
                                    .get_mut(&event_id)
                                    .expect("failed to get reactions")
                                    .push(ev);
                            } else {
                                self.reactions.insert(*event_id, vec![ev]);
                            }
                        }
                        _ => {}
                    };
                }
                _ => {}
            },
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
                let default_reactions = Vec::new();
                let reactions = self.reactions.get(&ev.id).unwrap_or(&default_reactions);

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
                text.extend(Text::styled(
                    format!("{}Liked", reactions.len()),
                    Style::default().fg(Color::Red),
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
