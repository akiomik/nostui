use crate::domain::collections::EventSet;
use crate::domain::nostr::Profile;
use crate::presentation::widgets::{public_key::PublicKey, shrink_text::ShrinkText};
use chrono::{DateTime, Local};
use nostr_sdk::nostr::{Alphabet, SingleLetterTag, TagKind, TagStandard};
use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets::*};
use thousands::Separable;

#[derive(Clone, Debug)]
pub struct TextNote {
    pub event: Event,
    pub profile: Option<Profile>,
    pub reactions: EventSet,
    pub reposts: EventSet,
    pub zap_receipts: EventSet,
    pub area: Rect,
    pub padding: Padding, // Only use to calc width/height
    pub highlight: bool,
    pub top_truncated_height: Option<usize>,
}

impl TextNote {
    pub fn new(
        event: Event,
        profile: Option<Profile>,
        reactions: EventSet,
        reposts: EventSet,
        zap_receipts: EventSet,
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
            highlight: false,
            top_truncated_height: None,
        }
    }

    pub fn display_name(&self) -> Option<String> {
        if let Some(profile) = self.profile.clone() {
            if let Some(display_name) = profile.metadata.display_name {
                if !display_name.is_empty() {
                    return Some(display_name);
                }
            }
        }

        None
    }

    pub fn name(&self) -> Option<String> {
        if let Some(profile) = self.profile.clone() {
            if let Some(name) = profile.metadata.name {
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

    pub fn created_at(&self) -> String {
        DateTime::from_timestamp(self.event.created_at.as_secs() as i64, 0)
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
            .filter(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A))
            })
            .next_back()
            .cloned()
    }

    fn find_reply_tag(&self) -> Option<Tag> {
        self.event
            .tags
            .iter()
            .filter(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .next_back()
            .cloned()
    }

    pub fn zap_amount(&self) -> u64 {
        self.zap_receipts.iter().fold(0, |acc, ev| {
            if let Some(tag) = self.find_amount(ev) {
                if let Some(TagStandard::Amount { millisats, .. }) = tag.as_standardized() {
                    acc + millisats
                } else {
                    acc
                }
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

    pub fn calculate_height(&self) -> u16 {
        let content: Text = ShrinkText::new(
            self.event.content.clone(),
            self.content_width() as usize,
            self.content_height() as usize,
        )
        .into();

        let height = if self.find_reply_tag().is_some() {
            // NOTE: 5 = annotation + name + created_at + stats + separator
            5 + content.height()
        } else {
            // NOTE: 4 = name + created_at + stats + separator
            4 + content.height()
        };

        height as u16
    }
}

impl Widget for TextNote {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut text = Text::default();

        if let Some(tag) = self.find_reply_tag() {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                let note1 = event_id.to_bech32().unwrap(); // Infallible
                text.extend(Text::styled(
                    format!("Reply to {note1}"),
                    Style::default().fg(Color::Cyan),
                ));
            }
        }

        let display_name = self.display_name();
        let name = self.name();

        let display_name_style = if self.highlight {
            Style::default().bold().reversed()
        } else {
            Style::default().bold()
        };

        let name_style = if display_name.is_none() && self.highlight {
            Style::default().italic().reversed()
        } else {
            Style::default().italic().fg(Color::Gray)
        };

        let name_line: Text = match (display_name, name) {
            (Some(display_name), Some(name)) => Line::from(vec![
                Span::styled(display_name, display_name_style),
                Span::raw(" "),
                Span::styled(name, name_style),
            ])
            .into(),
            (Some(display_name), _) => Span::styled(display_name, display_name_style).into(),
            (_, Some(name)) => Span::styled(name, name_style).into(),
            (_, _) => Span::styled(
                PublicKey::new(self.event.pubkey).shortened(),
                display_name_style,
            )
            .into(),
        };
        text.extend::<Text>(name_line);

        let content: Text = ShrinkText::new(
            self.event.content.clone(),
            self.content_width() as usize,
            self.content_height() as usize,
        )
        .into();
        text.extend(content);

        text.extend(Text::styled(
            self.created_at(),
            Style::default().fg(Color::Gray),
        ));
        let line = Line::from(vec![
            Span::styled(
                format!("{}Likes", self.reactions_count().separate_with_commas()),
                Style::default().fg(Color::LightRed),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{}Reposts", self.reposts_count().separate_with_commas()),
                Style::default().fg(Color::LightGreen),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{}Sats", (self.zap_amount() / 1000).separate_with_commas()),
                Style::default().fg(Color::LightYellow),
            ),
        ]);
        text.extend::<Text>(line.into());

        text.extend(Text::styled(
            "─".repeat(self.content_width() as usize),
            Style::default().fg(Color::Gray),
        ));

        if let Some(height) = self.top_truncated_height {
            let len = text.lines.len();
            let index = len.saturating_sub(height);
            let lines: Vec<Line> = Vec::from(&text.lines[index..]);
            Paragraph::new(lines).render(area, buf);
            return;
        }

        Paragraph::new(text).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use nostr_sdk::JsonUtil;
    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::*;
    use crate::domain::nostr::Profile;

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
    #[case(None, None)]
    #[case(Some(Metadata::new()), None)]
    #[case(Some(Metadata::new().name("foo")), None)]
    #[case(Some(Metadata::new().display_name("foo")), Some(String::from("foo")))]
    #[case(Some(Metadata::new().display_name("")), None)]
    #[case(Some(Metadata::new().display_name("").name("")), None)]
    #[case(Some(Metadata::new().display_name("").name("hoge")), None)]
    fn test_display_name(
        #[case] metadata: Option<Metadata>,
        #[case] expected: Option<String>,
        event: Event,
        area: Rect,
        padding: Padding,
    ) {
        let profile = metadata.map(|metadata| {
            Profile::new(
                nostr_sdk::PublicKey::from_str(
                    "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
                )
                .unwrap(),
                Timestamp::now(),
                metadata,
            )
        });

        let note = TextNote::new(
            event,
            profile,
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
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
        let profile = metadata.map(|metadata| {
            Profile::new(
                nostr_sdk::PublicKey::from_str(
                    "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
                )
                .unwrap(),
                Timestamp::now(),
                metadata,
            )
        });

        let note = TextNote::new(
            event,
            profile,
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
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
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Rect::new(0, 0, 0, 0),
            Padding::new(0, 0, 0, 0),
        );
        assert_eq!(note.created_at(), "15:42:47");
    }

    // Regression tests for hex username highlighting issue
    // These tests ensure that hex usernames (public keys without profiles) are properly highlighted when selected

    #[test]
    fn test_hex_username_highlighting_regression() {
        // Create event without profile (will show hex username)
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Test note with hex username")
            .sign_with_keys(&keys)
            .unwrap();

        let area = Rect::new(0, 0, 80, 10);

        // Test non-highlighted TextNote
        let mut text_note_normal = TextNote::new(
            event.clone(),
            None, // No profile - will show hex
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            area,
            Padding::new(1, 1, 1, 1),
        );
        text_note_normal.highlight = false;

        // Test highlighted TextNote
        let mut text_note_highlighted = TextNote::new(
            event,
            None,
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            area,
            Padding::new(1, 1, 1, 1),
        );
        text_note_highlighted.highlight = true;

        // Verify no profile exists (will show hex)
        assert_eq!(text_note_normal.display_name(), None);
        assert_eq!(text_note_normal.name(), None);
        assert_eq!(text_note_highlighted.display_name(), None);
        assert_eq!(text_note_highlighted.name(), None);

        // Render both and verify style differences
        let mut buffer_normal = ratatui::buffer::Buffer::empty(area);
        let mut buffer_highlighted = ratatui::buffer::Buffer::empty(area);

        text_note_normal.render(area, &mut buffer_normal);
        text_note_highlighted.render(area, &mut buffer_highlighted);

        // Count style differences - there should be at least some for the username line
        let style_differences = buffer_normal
            .content()
            .iter()
            .zip(buffer_highlighted.content().iter())
            .filter(|(cell1, cell2)| cell1.style() != cell2.style())
            .count();

        assert!(
            style_differences > 0,
            "Expected style differences between normal and highlighted hex username, but found none. This indicates a regression in hex username highlighting."
        );

        // Verify that the first line (username) has different styles
        let username_line_length = 11; // "xxxxx:yyyyy" format
        let first_line_differences = buffer_normal.content()
            [0..username_line_length.min(area.width as usize)]
            .iter()
            .zip(&buffer_highlighted.content()[0..username_line_length.min(area.width as usize)])
            .filter(|(cell1, cell2)| cell1.style() != cell2.style())
            .count();

        assert!(
            first_line_differences > 0,
            "Expected style differences in hex username line, but found none"
        );
    }

    #[test]
    fn test_named_user_highlighting_still_works() {
        // Create event with profile (will show name)
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Test note with named user")
            .sign_with_keys(&keys)
            .unwrap();

        let metadata = Metadata::new().display_name("Test User").name("testuser");

        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let area = Rect::new(0, 0, 80, 10);

        // Test highlighting with named user (regression check)
        let mut text_note_normal = TextNote::new(
            event.clone(),
            Some(profile.clone()),
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            area,
            Padding::new(1, 1, 1, 1),
        );
        text_note_normal.highlight = false;

        let mut text_note_highlighted = TextNote::new(
            event,
            Some(profile),
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            area,
            Padding::new(1, 1, 1, 1),
        );
        text_note_highlighted.highlight = true;

        // Verify profile exists (will show name)
        assert!(text_note_normal.display_name().is_some());
        assert!(text_note_highlighted.display_name().is_some());

        // Render and verify highlighting still works for named users
        let mut buffer_normal = ratatui::buffer::Buffer::empty(area);
        let mut buffer_highlighted = ratatui::buffer::Buffer::empty(area);

        text_note_normal.render(area, &mut buffer_normal);
        text_note_highlighted.render(area, &mut buffer_highlighted);

        let style_differences = buffer_normal
            .content()
            .iter()
            .zip(buffer_highlighted.content().iter())
            .filter(|(cell1, cell2)| cell1.style() != cell2.style())
            .count();

        assert!(
            style_differences > 0,
            "Expected style differences for named user highlighting, but found none. This indicates a regression."
        );
    }
}
