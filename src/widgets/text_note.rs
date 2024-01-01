use std::collections::HashSet;

use chrono::{DateTime, Local};
use nostr_sdk::{Event, Metadata, Tag};
use ratatui::{prelude::*, widgets::*};

use crate::widgets::shrink_text::ShrinkText;

#[derive(Clone, Debug)]
pub struct TextNote {
    pub event: Event,
    pub profile: Option<Metadata>,
    pub reactions: HashSet<Event>,
    pub reposts: HashSet<Event>,
    pub zap_receipts: HashSet<Event>,
    pub area: Rect,
    pub padding: Padding, // Only use to calc width/height
}

impl TextNote {
    pub fn new(
        event: Event,
        profile: Option<Metadata>,
        reactions: HashSet<Event>,
        reposts: HashSet<Event>,
        zap_receipts: HashSet<Event>,
        area: Rect,
        padding: Padding,
    ) -> Self {
        TextNote {
            event,
            profile,
            reactions,
            reposts,
            zap_receipts,
            area,
            padding,
        }
    }

    pub fn display_name(&self) -> Option<String> {
        if let Some(profile) = self.profile.clone() {
            if let Some(display_name) = profile.display_name {
                if !display_name.is_empty() {
                    return Some(display_name);
                }
            }

            if let Some(name) = profile.name {
                if !name.is_empty() {
                    return None;
                }
            }
        }

        Some(self.pubkey())
    }

    pub fn name(&self) -> Option<String> {
        if let Some(profile) = self.profile.clone() {
            if let Some(name) = profile.name {
                if !name.is_empty() {
                    match self.display_name() {
                        Some(display_name) if name == display_name => return None,
                        _ => return Some(format!("@{name}")),
                    }
                }
            }
        }

        None
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
            .format("%T")
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

        let display_name = value
            .display_name()
            .map(|name| format!("{name} "))
            .unwrap_or_default();
        let name = value
            .name()
            .map(|name| format!("{name} "))
            .unwrap_or_default();

        let mut text = Text::default();
        let name_line = Line::from(vec![
            Span::styled(display_name, Style::default().bold()),
            Span::styled(name, Style::default().italic().fg(Color::Gray)),
        ]);
        text.extend::<Text>(name_line.into());
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

#[cfg(test)]
mod tests {
    use nostr_sdk::{EventBuilder, JsonUtil, Keys};
    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::*;

    #[fixture]
    fn event() -> Event {
        Event::from_json(
            r#"{
                "kind":1,
                "sig":"a8d944e323439d16f867d59f0fb5c4b6f9c1302c887ab45c546b1fe38d58bf20263c79b1ffa86258a7607578a29c46f2613b286fb81efb45e2b2524a350a4f51",
                "id":"fcd6707cf1943d6f3ffa3c382bddb966027f98ddca15511a897a51ccfe160cd6",
                "pubkey":"4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
                "tags":[],
                "content":"初force pushめでたい",
                "created_at":1704091367
            }"#,
        ).unwrap()
    }

    #[fixture]
    fn area() -> Rect {
        Rect::new(0, 0, 0, 0)
    }

    #[fixture]
    fn padding() -> Padding {
        Padding::new(0, 0, 0, 0)
    }

    #[rstest]
    fn test_pubkey(event: Event, area: Rect, padding: Padding) {
        let note = TextNote::new(
            event,
            None,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            area,
            padding,
        );
        assert_eq!(note.pubkey(), "4d39c:aae25");
    }

    #[rstest]
    #[case(None, Some(String::from("4d39c:aae25")))]
    #[case(Some(Metadata::new()), Some(String::from("4d39c:aae25")))]
    #[case(Some(Metadata::new().name("foo")), None)]
    #[case(Some(Metadata::new().display_name("foo")), Some(String::from("foo")))]
    #[case(Some(Metadata::new().display_name("")), Some(String::from("4d39c:aae25")))]
    #[case(Some(Metadata::new().display_name("").name("")), Some(String::from("4d39c:aae25")))]
    #[case(Some(Metadata::new().display_name("").name("hoge")), None)]
    fn test_display_name(
        #[case] metadata: Option<Metadata>,
        #[case] expected: Option<String>,
        event: Event,
        area: Rect,
        padding: Padding,
    ) {
        let note = TextNote::new(
            event,
            metadata,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            area,
            padding,
        );
        assert_eq!(note.display_name(), expected);
    }

    #[rstest]
    #[case(None, None)]
    #[case(Some(Metadata::new()), None)]
    #[case(Some(Metadata::new().name("foo")), Some(String::from("@foo")))]
    #[case(Some(Metadata::new().display_name("foo")), None)]
    #[case(Some(Metadata::new().name("")), None)]
    #[case(Some(Metadata::new().name("foo").display_name("foo")), None)]
    fn test_name(
        #[case] metadata: Option<Metadata>,
        #[case] expected: Option<String>,
        event: Event,
        area: Rect,
        padding: Padding,
    ) {
        let note = TextNote::new(
            event,
            metadata,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            area,
            padding,
        );
        assert_eq!(note.name(), expected);
    }

    #[rstest]
    fn test_created_at(event: Event) {
        let note = TextNote::new(
            event,
            None,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            Rect::new(0, 0, 0, 0),
            Padding::new(0, 0, 0, 0),
        );
        assert_eq!(note.created_at(), "15:42:47");
    }
}
