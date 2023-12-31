use std::collections::hash_map::VacantEntry;
use std::collections::{HashSet, VecDeque};
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
    text,
    widgets::text_note::TextNote,
};

#[derive(Default)]
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    list_state: ListState,
    notes: VecDeque<Event>,
    reactions: HashMap<EventId, HashSet<Event>>,
    reposts: HashMap<EventId, HashSet<Event>>,
    zap_receipts: HashMap<EventId, HashSet<Event>>,
}

impl Home {
    pub fn new() -> Self {
        Self::default()
    }

    fn find_last_event_tag(&self, ev: &Event) -> Option<Tag> {
        ev.tags
            .iter()
            .filter(|tag| matches!(tag, Tag::Event { .. }))
            .last()
            .cloned()
    }

    fn append_reaction(&mut self, reaction: Event) {
        // reactions grouped by event_id
        if let Some(Tag::Event { event_id, .. }) = self.find_last_event_tag(&reaction) {
            match self.reactions.entry(event_id) {
                Entry::Vacant(e) => {
                    e.insert(HashSet::from([reaction]));
                }
                Entry::Occupied(mut e) => {
                    e.get_mut().insert(reaction);
                }
            }
        }
    }

    fn append_repost(&mut self, repost: Event) {
        // reposts grouped by event_id
        if let Some(Tag::Event { event_id, .. }) = self.find_last_event_tag(&repost) {
            match self.reposts.entry(event_id) {
                Entry::Vacant(e) => {
                    e.insert(HashSet::from([repost]));
                }
                Entry::Occupied(mut e) => {
                    e.get_mut().insert(repost);
                }
            };
        };
    }

    fn append_zap_receipt(&mut self, zap_receipt: Event) {
        // zap receipts grouped by event_id
        if let Some(Tag::Event { event_id, .. }) = self.find_last_event_tag(&zap_receipt) {
            match self.zap_receipts.entry(event_id) {
                Entry::Vacant(e) => {
                    e.insert(HashSet::from([zap_receipt]));
                }
                Entry::Occupied(mut e) => {
                    e.get_mut().insert(zap_receipt);
                }
            }
        }
    }

    fn text_note(&self, event: Event, area: Rect, padding: Padding) -> TextNote {
        let default_reactions = HashSet::new();
        let default_reposts = HashSet::new();
        let default_zap_receipts = HashSet::new();
        let reactions = self.reactions.get(&event.id).unwrap_or(&default_reactions);
        let reposts = self.reposts.get(&event.id).unwrap_or(&default_reposts);
        let zap_receipts = self
            .zap_receipts
            .get(&event.id)
            .unwrap_or(&default_zap_receipts);
        TextNote::new(
            event,
            reactions.clone(),
            reposts.clone(),
            zap_receipts.clone(),
            area,
            padding,
        )
    }

    fn scroll_up(&mut self) {
        let selection = match self.list_state.selected() {
            _ if self.notes.is_empty() => None,
            Some(i) if i > 1 => Some(i - 1),
            _ => Some(0),
        };
        self.list_state.select(selection);
    }

    fn scroll_down(&mut self) {
        let selection = match self.list_state.selected() {
            _ if self.notes.is_empty() => None,
            Some(i) if i < self.notes.len() - 1 => Some(i + 1),
            Some(_) => Some(self.notes.len() - 1),
            None if self.notes.len() > 1 => Some(1),
            None => Some(0),
        };
        self.list_state.select(selection);
    }

    fn scroll_to_top(&mut self) {
        let selection = match self.list_state.selected() {
            _ if self.notes.is_empty() => None,
            _ => Some(0),
        };
        self.list_state.select(selection);
    }

    fn scroll_to_bottom(&mut self) {
        let selection = match self.list_state.selected() {
            _ if self.notes.is_empty() => None,
            _ => Some(self.notes.len() - 1),
        };
        self.list_state.select(selection);
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
                Kind::TextNote => {
                    self.notes.push_front(ev);
                    let selection = self.list_state.selected().map(|i| i + 1);
                    self.list_state.select(selection);
                }
                Kind::Reaction => self.append_reaction(ev),
                Kind::Repost => self.append_repost(ev), // TODO: show reposts on feed
                Kind::ZapReceipt => self.append_zap_receipt(ev),
                _ => {}
            },
            Action::ScrollUp => self.scroll_up(),
            Action::ScrollDown => self.scroll_down(),
            Action::ScrollToTop => self.scroll_to_top(),
            Action::ScrollToBottom => self.scroll_to_bottom(),
            Action::React => {
                if let (Some(i), Some(tx)) = (self.list_state.selected(), &self.command_tx) {
                    let event = self.notes.get(i).expect("failed to get target event");
                    tx.send(Action::SendReaction(event.id))?;
                }
            }
            Action::Repost => {
                if let (Some(i), Some(tx)) = (self.list_state.selected(), &self.command_tx) {
                    let event = self.notes.get(i).expect("failed to get target event");
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
        let padding = Padding::new(1, 1, 1, 1);
        let items: Vec<ListItem> = self
            .notes
            .iter()
            .map(|ev| ListItem::new(self.text_note(ev.clone(), area, padding)))
            .collect();

        let list = List::new(items.clone())
            .block(Block::default().title("Timeline").padding(padding))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().reversed())
            .direction(ListDirection::TopToBottom);

        f.render_stateful_widget(list, area, &mut self.list_state);

        Ok(())
    }
}
