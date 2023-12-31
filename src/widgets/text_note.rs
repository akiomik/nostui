use chrono::{DateTime, Local};
use nostr_sdk::{Event, Tag};
use ratatui::{prelude::*, widgets::*};

use crate::widgets::shrink_text::ShrinkText;

#[derive(Clone, Debug)]
pub struct TextNote {
    pub event: Event,
    pub reactions: Vec<Event>,
    pub reposts: Vec<Event>,
    pub zap_receipts: Vec<Event>,
    pub area: Rect,
    pub padding: Padding, // Only use to calc width/height
}

impl TextNote {
    pub fn new(
        event: Event,
        reactions: Vec<Event>,
        reposts: Vec<Event>,
        zap_receipts: Vec<Event>,
        area: Rect,
        padding: Padding,
    ) -> Self {
        TextNote {
            event,
            reactions,
            reposts,
            zap_receipts,
            area,
            padding,
        }
    }

    pub fn pubkey(&self) -> String {
        let pubkey = self.event.pubkey.to_string();
        let len = pubkey.len();
        let heading = &pubkey[0..5];
        let trail = &pubkey[(len - 5)..len];
        format!("{}:{}", heading, trail)
    }

    pub fn created_at(&self) -> String {
        DateTime::from_timestamp(self.event.created_at.as_i64(), 0)
            .expect("Invalid created_at")
            .with_timezone(&Local)
            .format("%H:%m:%d")
            .to_string()
    }

    pub fn reactions_count(&self) -> usize {
        self.reactions.len()
    }

    pub fn reposts_count(&self) -> usize {
        self.reposts.len()
    }

    fn find_amount(&self, ev: &Event) -> Option<Tag> {
        ev.tags
            .iter()
            .filter(|tag| matches!(tag, Tag::Amount { millisats, bolt11 }))
            .last()
            .cloned()
    }

    pub fn zap_amount(&self) -> u64 {
        self.zap_receipts.iter().fold(0, |acc, ev| {
            if let Some(Tag::Amount { millisats, bolt11 }) = self.find_amount(ev) {
                acc + millisats
            } else {
                acc
            }
        })
    }

    fn content_width(&self) -> u16 {
        self.area
            .width
            .saturating_sub(self.padding.left + self.padding.right)
    }

    fn content_height(&self) -> u16 {
        // NOTE: 5 = name + content + created_at + stats + separator
        self.area
            .height
            .saturating_sub(self.padding.top + self.padding.bottom + 5)
    }
}

impl<'a> From<TextNote> for Text<'a> {
    fn from(value: TextNote) -> Self {
        let content: Text = ShrinkText::new(
            value.event.content.clone(),
            value.content_width() as usize,
            value.content_height() as usize,
        )
        .into();

        let mut text = Text::default();
        text.extend(Text::styled(value.pubkey(), Style::default().bold()));
        text.extend(content);
        text.extend(Text::styled(
            value.created_at(),
            Style::default().fg(Color::Gray),
        ));
        let line = Line::from(vec![
            Span::styled(
                format!("{}Likes", value.reactions_count()),
                Style::default().fg(Color::LightRed),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{}Reposts", value.reposts_count()),
                Style::default().fg(Color::LightGreen),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{}Sats", value.zap_amount() / 1000),
                Style::default().fg(Color::LightYellow),
            ),
        ]);
        text.extend::<Text>(line.into());
        text.extend(Text::styled(
            "─".repeat(value.content_width() as usize),
            Style::default().fg(Color::Gray),
        ));

        text
    }
}
