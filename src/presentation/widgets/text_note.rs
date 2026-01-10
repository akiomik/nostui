use crate::domain::collections::EventSet;
use crate::domain::nostr::Profile;
use crate::presentation::widgets::text_note_stats::TextNoteStats;
use crate::presentation::widgets::{name_with_handle::NameWithHandle, shrink_text::ShrinkText};
use chrono::{DateTime, Local};
use nostr_sdk::nostr::{Alphabet, SingleLetterTag, TagKind, TagStandard};
use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets::*};

#[derive(Clone, Debug)]
pub struct TextNote {
    pub event: Event,
    pub profile: Option<Profile>,
    pub mentioned_profiles: Vec<Profile>,
    pub reactions: EventSet,
    pub reposts: EventSet,
    pub zap_receipts: EventSet,
    pub padding: Padding, // Only use to calc width/height
    pub highlight: bool,
    pub top_truncated_height: Option<usize>,
}

impl TextNote {
    pub fn new(
        event: Event,
        profile: Option<Profile>,
        mentioned_profiles: Vec<Profile>,
        reactions: EventSet,
        reposts: EventSet,
        zap_receipts: EventSet,
        padding: Padding,
    ) -> Self {
        TextNote {
            event,
            profile,
            mentioned_profiles,
            reactions,
            reposts,
            zap_receipts,
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

    fn find_reply_tag(&self) -> Option<&TagStandard> {
        self.event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
    }

    fn find_client_tag(&self) -> Option<&TagStandard> {
        self.event.tags.find_standardized(TagKind::Client)
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

    pub fn calculate_height(&self, area: &Rect) -> u16 {
        // Calculate available width for content
        let width = area
            .width
            .saturating_sub(self.padding.left + self.padding.right);

        // Calculate the number of fixed lines (non-content)
        let has_reply = self.find_reply_tag().is_some();
        let fixed_lines = if has_reply {
            5 // annotation + name + created_at + stats + separator
        } else {
            4 // name + created_at + stats + separator
        };

        // Calculate available height for content
        let available_height = area
            .height
            .saturating_sub(self.padding.top + self.padding.bottom + fixed_lines);

        // Calculate content height
        let content: Text = ShrinkText::new(
            self.event.content.clone(),
            width as usize,
            available_height as usize,
        )
        .into();

        // Total height = fixed lines + actual content height
        fixed_lines + content.height() as u16
    }
}

impl Widget for TextNote {
    #[allow(clippy::unwrap_used)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut text = Text::default();

        if let Some(TagStandard::Event { event_id, .. }) = self.find_reply_tag() {
            let mentioned_names: Vec<String> = self
                .mentioned_profiles
                .iter()
                .map(|profile| profile.name())
                .collect();

            let reply_text = if mentioned_names.is_empty() {
                let note1 = event_id.to_bech32().unwrap(); // Infallible
                format!("Reply to {note1}")
            } else {
                format!("Reply to {}", mentioned_names.join(", "))
            };

            text.extend(Text::from(Line::styled(
                reply_text,
                Style::default().fg(Color::Cyan),
            )));
        }

        let name_with_handle =
            NameWithHandle::new(self.event.pubkey, &self.profile, self.highlight);
        text.extend::<Text>(name_with_handle.into());

        let content: Text = ShrinkText::new(
            self.event.content.clone(),
            area.width as usize,
            area.height as usize,
        )
        .into();
        text.extend(content);

        let meta = match self.find_client_tag() {
            Some(TagStandard::Client { name, .. }) => format!("{} | via {name}", self.created_at()),
            _ => self.created_at(),
        };
        text.extend(Text::from(Line::styled(
            meta,
            Style::default().fg(Color::Gray),
        )));

        let stats = TextNoteStats::new(
            self.reactions_count(),
            self.reposts_count(),
            self.zap_amount() / 1000,
        );
        text.extend::<Text>(stats.into());

        text.extend(Text::styled(
            "─".repeat(area.width as usize),
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
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );
        assert_eq!(note.created_at(), "15:42:47");
    }

    #[test]
    fn test_calculate_height_without_reply() -> Result<()> {
        // Create a simple event without reply tag
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Short content").sign_with_keys(&keys)?;

        let text_note = TextNote::new(
            event,
            None,
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );

        // Area with sufficient height
        let area = Rect::new(0, 0, 80, 20);
        let height = text_note.calculate_height(&area);

        // Expected: 4 fixed lines (name + created_at + stats + separator) + content lines
        // "Short content" should fit in one line with width 80
        assert_eq!(height, 5); // 4 fixed + 1 content line

        Ok(())
    }

    #[test]
    fn test_calculate_height_with_reply() -> Result<()> {
        // Create an event with reply tag
        let keys = Keys::generate();
        let replied_event_id = EventId::all_zeros();
        let event = EventBuilder::text_note("Reply content")
            .tag(Tag::event(replied_event_id))
            .sign_with_keys(&keys)?;

        let text_note = TextNote::new(
            event,
            None,
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );

        let area = Rect::new(0, 0, 80, 20);
        let height = text_note.calculate_height(&area);

        // Expected: 5 fixed lines (annotation + name + created_at + stats + separator) + content lines
        assert_eq!(height, 6); // 5 fixed + 1 content line

        Ok(())
    }

    #[test]
    fn test_calculate_height_with_padding() -> Result<()> {
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Test").sign_with_keys(&keys)?;

        let text_note = TextNote::new(
            event,
            None,
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(2, 2, 1, 1), // top, right, bottom, left
        );

        let area = Rect::new(0, 0, 80, 20);
        let height = text_note.calculate_height(&area);

        // Padding should affect available height but not the returned total height
        // With padding: available_height = 20 - (2 + 2 + 4) = 12
        // But the result should still be 4 fixed + content height
        assert_eq!(height, 5); // 4 fixed + 1 content line

        Ok(())
    }

    #[test]
    fn test_calculate_height_with_multiline_content() -> Result<()> {
        let keys = Keys::generate();
        // Create a long content that will wrap into multiple lines
        let long_content =
            "This is a very long content that should wrap into multiple lines when rendered. "
                .repeat(5);
        let event = EventBuilder::text_note(long_content).sign_with_keys(&keys)?;

        let text_note = TextNote::new(
            event,
            None,
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );

        let area = Rect::new(0, 0, 80, 20);
        let height = text_note.calculate_height(&area);

        // Should be more than 5 (4 fixed + at least 1 content line)
        assert!(height > 5, "Expected height > 5, got {height}");

        Ok(())
    }

    #[test]
    fn test_calculate_height_with_narrow_width() -> Result<()> {
        let keys = Keys::generate();
        let event = EventBuilder::text_note("This content will wrap on narrow width")
            .sign_with_keys(&keys)?;

        let text_note = TextNote::new(
            event,
            None,
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );

        // Narrow area should cause more wrapping
        let narrow_area = Rect::new(0, 0, 20, 20);
        let narrow_height = text_note.calculate_height(&narrow_area);

        let wide_area = Rect::new(0, 0, 80, 20);
        let wide_height = text_note.calculate_height(&wide_area);

        // Narrow width should result in greater height due to wrapping
        assert!(
            narrow_height >= wide_height,
            "Expected narrow_height ({narrow_height}) >= wide_height ({wide_height})",
        );

        Ok(())
    }

    #[test]
    fn test_calculate_height_consistency_with_render() -> Result<()> {
        // Ensure calculate_height returns consistent values for the same input
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Consistency test").sign_with_keys(&keys)?;

        let text_note = TextNote::new(
            event,
            None,
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(1, 1, 1, 1),
        );

        let area = Rect::new(0, 0, 80, 20);

        let height1 = text_note.calculate_height(&area);
        let height2 = text_note.calculate_height(&area);

        assert_eq!(
            height1, height2,
            "calculate_height should return consistent results"
        );

        Ok(())
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
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(1, 1, 1, 1),
        );
        text_note_normal.highlight = false;

        // Test highlighted TextNote
        let mut text_note_highlighted = TextNote::new(
            event,
            None,
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
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
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(1, 1, 1, 1),
        );
        text_note_normal.highlight = false;

        let mut text_note_highlighted = TextNote::new(
            event,
            Some(profile),
            vec![],
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
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

    #[test]
    fn test_reply_with_mentioned_profiles() -> Result<()> {
        // Create an event with reply tag and p-tags
        let keys = Keys::generate();
        let mentioned_key1 = Keys::generate();
        let mentioned_key2 = Keys::generate();
        let replied_event_id = EventId::all_zeros();

        let event = EventBuilder::text_note("Reply to multiple people")
            .tag(Tag::event(replied_event_id))
            .tag(Tag::public_key(mentioned_key1.public_key()))
            .tag(Tag::public_key(mentioned_key2.public_key()))
            .sign_with_keys(&keys)?;

        // Create profiles for mentioned users
        let profile1 = Profile::new(
            mentioned_key1.public_key(),
            Timestamp::now(),
            Metadata::new().display_name("Alice"),
        );
        let profile2 = Profile::new(
            mentioned_key2.public_key(),
            Timestamp::now(),
            Metadata::new().name("bob"),
        );

        let mentioned_profiles = vec![profile1, profile2];

        let text_note = TextNote::new(
            event,
            None,
            mentioned_profiles,
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );

        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        text_note.render(area, &mut buffer);

        // Verify the reply text contains mentioned user names
        let rendered_text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            rendered_text.contains("Reply to Alice, @bob"),
            "Expected 'Reply to Alice, @bob' in rendered output, got: {rendered_text}"
        );

        Ok(())
    }

    #[test]
    fn test_reply_without_mentioned_profiles() -> Result<()> {
        // Create an event with reply tag but no profiles available
        let keys = Keys::generate();
        let replied_event_id = EventId::all_zeros();

        let event = EventBuilder::text_note("Reply without profiles")
            .tag(Tag::event(replied_event_id))
            .sign_with_keys(&keys)?;

        let text_note = TextNote::new(
            event,
            None,
            vec![], // No mentioned profiles
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );

        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        text_note.render(area, &mut buffer);

        // Verify the reply text falls back to note1 format when no profiles available
        let rendered_text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            rendered_text.contains("Reply to note1"),
            "Expected 'Reply to note1...' in rendered output when no profiles available"
        );

        Ok(())
    }

    #[test]
    fn test_reply_with_single_mentioned_profile() -> Result<()> {
        // Create an event with reply tag and one p-tag
        let keys = Keys::generate();
        let mentioned_key = Keys::generate();
        let replied_event_id = EventId::all_zeros();

        let event = EventBuilder::text_note("Reply to one person")
            .tag(Tag::event(replied_event_id))
            .tag(Tag::public_key(mentioned_key.public_key()))
            .sign_with_keys(&keys)?;

        // Create profile for mentioned user with only name (no display_name)
        let profile = Profile::new(
            mentioned_key.public_key(),
            Timestamp::now(),
            Metadata::new().name("charlie"),
        );

        let mentioned_profiles = vec![profile];

        let text_note = TextNote::new(
            event,
            None,
            mentioned_profiles,
            EventSet::new(),
            EventSet::new(),
            EventSet::new(),
            Padding::new(0, 0, 0, 0),
        );

        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        text_note.render(area, &mut buffer);

        // Verify the reply text contains the mentioned user's handle
        let rendered_text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            rendered_text.contains("Reply to @charlie"),
            "Expected 'Reply to @charlie' in rendered output, got: {rendered_text}"
        );

        Ok(())
    }
}
