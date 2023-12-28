use std::collections::hash_map::VacantEntry;
use std::collections::VecDeque;
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::format,
    time::Duration,
};

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
    list_state: ListState,
    events: VecDeque<Event>,
    reactions: HashMap<EventId, Vec<Event>>,
    reposts: HashMap<EventId, Vec<Event>>,
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

    fn find_last_event_tag(&self, ev: &Event) -> Option<Tag> {
        ev.tags
            .iter()
            .filter(|tag| {
                matches!(
                    tag,
                    Tag::Event {
                        event_id,
                        relay_url,
                        marker,
                    }
                )
            })
            .last()
            .cloned()
    }

    fn append_reaction(&mut self, reaction: Event) {
        // reactions grouped by event_id
        if let Some(Tag::Event {
            event_id,
            relay_url,
            marker,
        }) = self.find_last_event_tag(&reaction)
        {
            if let Entry::Vacant(e) = self.reactions.entry(event_id) {
                e.insert(vec![reaction]);
            } else {
                self.reactions
                    .get_mut(&event_id)
                    .expect("failed to get reactions")
                    .push(reaction);
            }
        }
    }

    fn append_repost(&mut self, repost: Event) {
        // reposts grouped by event_id
        if let Some(Tag::Event {
            event_id,
            relay_url,
            marker,
        }) = self.find_last_event_tag(&repost)
        {
            if let Entry::Vacant(e) = self.reposts.entry(event_id) {
                e.insert(vec![repost]);
            } else {
                self.reposts
                    .get_mut(&event_id)
                    .expect("failed to get reactions")
                    .push(repost);
            }
        }
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
                Kind::TextNote => self.events.push_front(ev),
                Kind::Reaction => self.append_reaction(ev),
                Kind::Repost => self.append_repost(ev), // TODO: show reposts on feed
                _ => {}
            },
            Action::ScrollUp => {
                let selection = match self.list_state.selected() {
                    _ if self.events.is_empty() => None,
                    Some(i) if i > 1 => Some(i - 1),
                    _ => Some(0),
                };
                self.list_state.select(selection);
            }
            Action::ScrollDown => {
                let selection = match self.list_state.selected() {
                    _ if self.events.is_empty() => None,
                    Some(i) if i < self.events.len() - 1 => Some(i + 1),
                    Some(_) => Some(self.events.len() - 1),
                    None if self.events.len() > 1 => Some(1),
                    None => Some(0),
                };
                self.list_state.select(selection);
            }
            Action::ScrollTop => {
                let selection = match self.list_state.selected() {
                    _ if self.events.is_empty() => None,
                    _ => Some(0),
                };
                self.list_state.select(selection);
            }
            Action::ScrollBottom => {
                let selection = match self.list_state.selected() {
                    _ if self.events.is_empty() => None,
                    _ => Some(self.events.len() - 1),
                };
                self.list_state.select(selection);
            }
            Action::React => {
                if let (Some(i), Some(tx)) = (self.list_state.selected(), &self.command_tx) {
                    let event = self.events.get(i).expect("failed to get target event");
                    tx.send(Action::SendReaction(event.id))?;
                }
            }
            Action::Repost => {
                if let (Some(i), Some(tx)) = (self.list_state.selected(), &self.command_tx) {
                    let event = self.events.get(i).expect("failed to get target event");
                    tx.send(Action::SendRepost(event.id))?;
                }
            }
            Action::Unselect => {
                self.list_state.select(None);
            }
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
                let default_reactions = vec![];
                let default_reposts = vec![];
                let reactions = self.reactions.get(&ev.id).unwrap_or(&default_reactions);
                let reposts = self.reposts.get(&ev.id).unwrap_or(&default_reposts);

                let mut text = Text::default();
                text.extend(Text::styled(
                    self.format_pubkey(ev.pubkey.to_string()),
                    Style::default().bold(),
                ));
                text.extend(Text::raw(ev.content.clone())); // TODO: wrap line
                text.extend(Text::styled(
                    created_at.to_string(),
                    Style::default().fg(Color::Gray),
                ));
                let line = Line::from(vec![
                    Span::styled(
                        format!("{}Likes", reactions.len()),
                        Style::default().fg(Color::LightRed),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{}Reposts", reposts.len()),
                        Style::default().fg(Color::LightGreen),
                    ),
                ]);
                text.extend::<Text>(line.into());
                text.extend(Text::raw("─".repeat(area.width as usize)));

                ListItem::new(text)
            })
            .collect();

        let list = List::new(items.clone())
            .block(Block::default().title("Timeline").borders(Borders::ALL))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().reversed())
            .direction(ListDirection::TopToBottom);

        f.render_stateful_widget(list, area, &mut self.list_state);

        Ok(())
    }
}
