use std::cmp::Reverse;
use std::collections::{hash_map::Entry, HashMap};
use std::collections::{HashSet, VecDeque};

use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets, widgets::*};
use sorted_vec::ReverseSortedSet;
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, Frame};
use crate::{
    action::Action, config::Config, nostr::SortableEvent, text, widgets::ScrollableList,
    widgets::TextNote,
};

#[derive(Default)]
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    list_state: ListState,
    notes: Vec<Event>,
    profiles: HashMap<XOnlyPublicKey, Metadata>,
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

    fn add_note(&mut self, event: Event) {
        self.notes.push(event);

        // Keep selected position
        let selection = self.list_state.selected().map(|i| i + 1);
        self.list_state.select(selection);
    }

    fn add_profile(&mut self, event: Event) {
        if let Ok(metadata) = Metadata::from_json(event.content) {
            self.profiles.insert(event.pubkey, metadata);
        }
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
        let profile = self.profiles.get(&event.pubkey);
        let reactions = self.reactions.get(&event.id).unwrap_or(&default_reactions);
        let reposts = self.reposts.get(&event.id).unwrap_or(&default_reposts);
        let zap_receipts = self
            .zap_receipts
            .get(&event.id)
            .unwrap_or(&default_zap_receipts);
        TextNote::new(
            event,
            profile.cloned(),
            reactions.clone(),
            reposts.clone(),
            zap_receipts.clone(),
            area,
            padding,
        )
    }

    fn sorted_notes(&self) -> ReverseSortedSet<SortableEvent> {
        ReverseSortedSet::from_unsorted(
            self.notes
                .iter()
                .map(|ev| Reverse(SortableEvent::new(ev.clone())))
                .collect(),
        )
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
                Kind::Metadata => self.add_profile(ev),
                Kind::TextNote => self.add_note(ev),
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
            .sorted_notes()
            .iter()
            .map(|ev| ListItem::new(self.text_note(ev.0.event.clone(), area, padding)))
            .collect();

        let list = List::new(items.clone())
            .block(widgets::Block::default().title("Timeline").padding(padding))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().reversed())
            .direction(ListDirection::TopToBottom);

        f.render_stateful_widget(list, area, &mut self.list_state);

        Ok(())
    }
}

impl ScrollableList<Event> for Home {
    fn select(&mut self, index: Option<usize>) {
        self.list_state.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn len(&self) -> usize {
        self.notes.len()
    }

    fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}
