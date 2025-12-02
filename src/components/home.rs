use std::cmp::Reverse;
use std::collections::{hash_map::Entry, HashMap};

use color_eyre::eyre::Result;
use nostr_sdk::nostr::{Alphabet, SingleLetterTag, TagKind, TagStandard};
use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets, widgets::*};
use sorted_vec::ReverseSortedSet;
use tokio::sync::mpsc::UnboundedSender;
use tui_textarea::TextArea;
use tui_widget_list::{ListBuilder, ListView};

use super::{Component, Frame};
use crate::text::shorten_hex;
use crate::{
    action::Action,
    config::Config,
    nostr::{nip10::ReplyTagsBuilder, Profile, SortableEvent},
    widgets::ScrollableList,
    widgets::TextNote,
};

#[derive(Default)]
pub struct Home<'a> {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    list_state: tui_widget_list::ListState,
    notes: ReverseSortedSet<SortableEvent>,
    profiles: HashMap<PublicKey, Profile>,
    reactions: HashMap<EventId, Vec<Event>>,
    reposts: HashMap<EventId, Vec<Event>>,
    zap_receipts: HashMap<EventId, Vec<Event>>,
    show_input: bool,
    input: TextArea<'a>,
    reply_to: Option<Event>,
}

impl Home<'_> {
    pub fn new() -> Self {
        Self::default()
    }

    fn find_last_event_tag(&self, ev: &Event) -> Option<Tag> {
        ev.tags
            .iter()
            .filter(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .next_back()
            .cloned()
    }

    fn add_note(&mut self, event: Event) {
        let note = Reverse(SortableEvent::new(event));
        self.notes.find_or_insert(note);

        // Keep selected position
        let selection = self.list_state.selected.map(|i| i + 1);
        self.list_state.select(selection);
    }

    fn add_profile(&mut self, event: Event) {
        if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
            let profile = Profile::new(event.pubkey, event.created_at, metadata);
            if let Some(existing_profile) = self.profiles.get(&event.pubkey) {
                if existing_profile.created_at > profile.created_at {
                    return;
                }
            }

            self.profiles.insert(event.pubkey, profile);
        }
    }

    fn append_reaction(&mut self, reaction: Event) {
        // reactions grouped by event_id
        if let Some(tag) = self.find_last_event_tag(&reaction) {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                match self.reactions.entry(*event_id) {
                    Entry::Vacant(e) => {
                        e.insert(vec![reaction]);
                    }
                    Entry::Occupied(mut e) => {
                        let vec = e.get_mut();
                        if !vec.iter().any(|r| r.id == reaction.id) {
                            vec.push(reaction);
                        }
                    }
                }
            }
        }
    }

    fn append_repost(&mut self, repost: Event) {
        // reposts grouped by event_id
        if let Some(tag) = self.find_last_event_tag(&repost) {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                match self.reposts.entry(*event_id) {
                    Entry::Vacant(e) => {
                        e.insert(vec![repost]);
                    }
                    Entry::Occupied(mut e) => {
                        let vec = e.get_mut();
                        if !vec.iter().any(|r| r.id == repost.id) {
                            vec.push(repost);
                        }
                    }
                }
            }
        }
    }

    fn append_zap_receipt(&mut self, zap_receipt: Event) {
        // zap receipts grouped by event_id
        if let Some(tag) = self.find_last_event_tag(&zap_receipt) {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                match self.zap_receipts.entry(*event_id) {
                    Entry::Vacant(e) => {
                        e.insert(vec![zap_receipt]);
                    }
                    Entry::Occupied(mut e) => {
                        let vec = e.get_mut();
                        if !vec.iter().any(|z| z.id == zap_receipt.id) {
                            vec.push(zap_receipt);
                        }
                    }
                }
            }
        }
    }

    fn text_note(&self, event: Event, area: Rect, padding: Padding) -> TextNote {
        let default_reactions = Vec::new();
        let default_reposts = Vec::new();
        let default_zap_receipts = Vec::new();
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

    fn get_note(&self, i: usize) -> Option<&Event> {
        self.notes.get(i).map(|note| &note.0.event)
    }

    fn clear_input(&mut self) {
        self.input.select_all();
        self.input.delete_str(usize::MAX);
    }
}

impl Component for Home<'_> {
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
            Action::ScrollUp => {
                if !self.show_input {
                    self.scroll_up()
                }
            }
            Action::ScrollDown => {
                if !self.show_input {
                    self.scroll_down()
                }
            }
            Action::ScrollToTop => {
                if !self.show_input {
                    self.scroll_to_top()
                }
            }
            Action::ScrollToBottom => {
                if !self.show_input {
                    self.scroll_to_bottom()
                }
            }
            Action::React => {
                if let (false, Some(i), Some(tx)) =
                    (self.show_input, self.list_state.selected, &self.command_tx)
                {
                    let event = self.get_note(i).expect("failed to get target event");
                    tx.send(Action::SendReaction(event.clone()))?;
                }
            }
            Action::Repost => {
                if let (false, Some(i), Some(tx)) =
                    (self.show_input, self.list_state.selected, &self.command_tx)
                {
                    let event = self.get_note(i).expect("failed to get target event");
                    tx.send(Action::SendRepost(event.clone()))?;
                }
            }
            Action::Unselect => {
                self.list_state.select(None);
                self.show_input = false;
                self.reply_to = None;
            }
            Action::NewTextNote => {
                self.reply_to = None;
                self.show_input = true;
            }
            Action::ReplyTextNote => {
                if let Some(i) = self.selected() {
                    let selected = self.get_note(i).unwrap();
                    self.reply_to = Some(selected.clone());
                    self.show_input = true;
                }
            }
            Action::SubmitTextNote => {
                if let (true, Some(tx)) = (self.show_input, &self.command_tx) {
                    let content = self.input.lines().join("\n");
                    if !content.is_empty() {
                        let tags = if let Some(ref reply_to) = self.reply_to {
                            ReplyTagsBuilder::build(reply_to.clone())
                        } else {
                            vec![]
                        };
                        tx.send(Action::SendTextNote(content, tags))?;
                        self.reply_to = None;
                        self.show_input = false;
                        self.clear_input();
                    }
                }
            }
            Action::Key(key) => {
                if self.show_input {
                    self.input.input(crossterm::event::Event::Key(key));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let padding = Padding::new(1, 1, 1, 3);

        // Pre-generate all TextNote items with proper heights
        let mut items: Vec<(TextNote, u16)> = Vec::new();
        for ev in &self.notes {
            let text_note = self.text_note(ev.0.event.clone(), area, padding);
            let height = text_note.calculate_height();
            items.push((text_note, height));
        }

        let item_count = items.len();

        let builder = ListBuilder::new(move |context| {
            let mut item = items[context.index].clone();
            item.0.highlight = context.is_selected;
            (item.0, item.1)
        });

        let list = ListView::new(builder, item_count)
            .block(widgets::Block::default().title("Timeline").padding(padding))
            .style(Style::default().fg(Color::White));

        f.render_stateful_widget(list, area, &mut self.list_state);

        if self.show_input {
            let mut input_area = f.area();
            input_area.height /= 2;
            input_area.y = input_area.height;
            input_area.height -= 2;
            f.render_widget(Clear, input_area);

            let block = if let Some(ref reply_to) = self.reply_to {
                let name = if let Some(profile) = self.profiles.get(&reply_to.pubkey) {
                    profile.name()
                } else {
                    shorten_hex(&reply_to.pubkey.to_string())
                };

                widgets::Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Replying to {name}: Press ESC to close"))
            } else {
                widgets::Block::default()
                    .borders(Borders::ALL)
                    .title("New note: Press ESC to close")
            };
            self.input.set_block(block);
            f.render_widget(&self.input, input_area);
        }

        Ok(())
    }
}

impl ScrollableList<Event> for Home<'_> {
    fn select(&mut self, index: Option<usize>) {
        self.list_state.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.list_state.selected
    }

    fn len(&self) -> usize {
        self.notes.len()
    }

    fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}
