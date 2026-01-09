use crate::domain::collections::EventSet;
use crate::domain::nostr::Profile;
use crate::presentation::widgets::{name_with_handle::NameWithHandle, shrink_text::ShrinkText};
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

    fn find_amount(&self, ev: &Event) -> Option<TagStandard> {
        ev.tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::A,
            )))
            .last()
            .cloned()
    }

    fn find_reply_tag(&self) -> Option<TagStandard> {
        self.event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
            .cloned()
    }

    pub fn zap_amount(&self) -> u64 {
        self.zap_receipts.iter().fold(0, |acc, ev| {
            if let Some(TagStandard::Amount { millisats, .. }) = self.find_amount(ev) {
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
    #[allow(clippy::unwrap_used)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut text = Text::default();

        if let Some(TagStandard::Event { event_id, .. }) = self.find_reply_tag() {
            let note1 = event_id.to_bech32().unwrap(); // Infallible
            text.extend(Text::styled(
                format!("Reply to {note1}"),
                Style::default().fg(Color::Cyan),
            ));
        }

        let name_with_handle =
            NameWithHandle::new(self.event.pubkey, &self.profile, self.highlight);
        text.extend::<Text>(name_with_handle.into());

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
    use nostr_sdk::JsonUtil;
    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::*;
    use crate::domain::nostr::Profile;

    #[fixture]
    #[allow(clippy::unwrap_used)]
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
    fn test_hex_username_highlighting_regression() -> Result<()> {
        // Create event without profile (will show hex username)
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Test note with hex username").sign_with_keys(&keys)?;

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
        assert_eq!(
            text_note_normal
                .profile
                .as_ref()
                .and_then(|profile| profile.display_name()),
            None
        );
        assert_eq!(
            text_note_normal
                .profile
                .as_ref()
                .and_then(|profile| profile.handle()),
            None
        );
        assert_eq!(
            text_note_highlighted
                .profile
                .as_ref()
                .and_then(|profile| profile.display_name()),
            None
        );
        assert_eq!(
            text_note_highlighted
                .profile
                .as_ref()
                .and_then(|profile| profile.handle()),
            None
        );

        // Render both and verify style differences
        let mut buffer_normal = Buffer::empty(area);
        let mut buffer_highlighted = Buffer::empty(area);

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

        Ok(())
    }

    #[test]
    fn test_named_user_highlighting_still_works() -> Result<()> {
        // Create event with profile (will show name)
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Test note with named user").sign_with_keys(&keys)?;

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
        assert!(text_note_normal
            .profile
            .as_ref()
            .and_then(|profile| profile.display_name())
            .is_some());
        assert!(text_note_highlighted
            .profile
            .as_ref()
            .and_then(|profile| profile.handle())
            .is_some());

        // Render and verify highlighting still works for named users
        let mut buffer_normal = Buffer::empty(area);
        let mut buffer_highlighted = Buffer::empty(area);

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

        Ok(())
    }
}
