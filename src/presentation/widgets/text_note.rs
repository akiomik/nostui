use std::collections::HashMap;

use crate::{
    domain::{nostr::Profile, text::shorten_npub},
    model::timeline::text_note::TextNote,
    presentation::widgets::{
        name_with_handle::NameWithHandle, shrink_text::ShrinkText, text_note_stats::TextNoteStats,
    },
};

use nostr_sdk::prelude::*;
use ratatui::{
    prelude::*,
    widgets::{Padding, Paragraph},
};

#[derive(Debug, Clone, PartialEq)]
pub struct ViewContext<'a> {
    pub profiles: &'a HashMap<PublicKey, Profile>,
    pub live_status: Option<LiveStatus>,
    pub selected: bool,
}

pub struct TextNoteWidget<'a> {
    text_note: TextNote,
    ctx: ViewContext<'a>,
}

impl<'a> TextNoteWidget<'a> {
    pub fn new(text_note: TextNote, ctx: ViewContext<'a>) -> Self {
        Self { text_note, ctx }
    }

    pub fn mentioned_names(&self) -> Vec<String> {
        self.text_note
            .mentioned_pubkeys()
            .map(|pubkey| {
                self.ctx
                    .profiles
                    .get(pubkey)
                    .map(|p| p.name())
                    .unwrap_or_else(|| {
                        let Ok(npub) = pubkey.to_bech32();
                        shorten_npub(npub)
                    })
            })
            .collect()
    }

    pub fn calculate_height(&self, area: &Rect, padding: Padding) -> u16 {
        // Calculate available width for content
        let width = area.width.saturating_sub(padding.left + padding.right);

        // Calculate the number of fixed lines (non-content)
        let has_reply = self.text_note.find_reply_tag().is_some();
        let fixed_lines = if has_reply {
            5 // annotation + name + created_at + stats + separator
        } else {
            4 // name + created_at + stats + separator
        };

        // Calculate available height for content
        let available_height = area
            .height
            .saturating_sub(padding.top + padding.bottom + fixed_lines);

        // Calculate content height
        let content: Text = ShrinkText::new(
            self.text_note.content().clone(),
            width as usize,
            available_height as usize,
        )
        .into();

        // Total height = fixed lines + actual content height
        fixed_lines + content.height() as u16
    }
}

impl<'a> Widget for TextNoteWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let mut text = Text::default();

        if let Some(TagStandard::Event { event_id, .. }) = self.text_note.find_reply_tag() {
            let mentioned_names = self.mentioned_names();
            let reply_text = if mentioned_names.is_empty() {
                let Ok(note1) = event_id.to_bech32();
                format!("Reply to {note1}")
            } else {
                format!("Reply to {}", mentioned_names.join(", "))
            };

            text.extend(Text::from(Line::styled(
                reply_text,
                Style::default().fg(Color::Cyan),
            )));
        }

        let name_with_handle = NameWithHandle::new(
            self.text_note.author_pubkey(),
            self.ctx.profiles.get(&self.text_note.author_pubkey()),
            self.ctx.selected,
        );
        text.extend::<Text>(name_with_handle.into());

        let content: Text = ShrinkText::new(
            self.text_note.content().clone(),
            area.width as usize,
            area.height as usize,
        )
        .into();
        text.extend(content);

        let meta = match self.text_note.find_client_tag() {
            Some(TagStandard::Client { name, .. }) => {
                format!("{} | via {name}", self.text_note.created_at())
            }
            _ => self.text_note.created_at(),
        };
        text.extend(Text::from(Line::styled(
            meta,
            Style::default().fg(Color::Gray),
        )));

        let stats = TextNoteStats::new(
            self.text_note.reactions_count(),
            self.text_note.reposts_count(),
            self.text_note.zap_amount() / 1000,
        );
        text.extend::<Text>(stats.into());

        text.extend(Text::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(Color::Gray),
        ));

        Paragraph::new(text).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::nostr::{EventBuilder, Keys};
    use std::collections::HashMap;
    use std::error::Error;

    fn create_test_event(content: &str) -> Result<Event, Box<dyn Error>> {
        let keys = Keys::generate();
        Ok(EventBuilder::text_note(content).sign_with_keys(&keys)?)
    }

    fn create_test_event_with_tags(content: &str, tags: Vec<Tag>) -> Result<Event, Box<dyn Error>> {
        let keys = Keys::generate();
        let builder = EventBuilder::text_note(content).tags(tags);
        Ok(builder.sign_with_keys(&keys)?)
    }

    fn create_test_profile(name: &str, display_name: Option<&str>) -> Profile {
        let keys = Keys::generate();
        let metadata = if let Some(display_name) = display_name {
            Metadata::new().name(name).display_name(display_name)
        } else {
            Metadata::new().name(name)
        };
        Profile::new(keys.public_key(), Timestamp::now(), metadata)
    }

    #[test]
    fn test_view_context_equality() {
        let profiles = HashMap::new();

        let ctx1 = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let ctx2 = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        assert_eq!(ctx1, ctx2);
    }

    #[test]
    fn test_view_context_with_selection() {
        let profiles = HashMap::new();

        let ctx_selected = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: true,
        };

        let ctx_not_selected = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        assert_ne!(ctx_selected, ctx_not_selected);
    }

    #[test]
    fn test_mentioned_names_with_profiles() -> Result<(), Box<dyn Error>> {
        let mentioned_keys = Keys::generate();
        let p_tag = Tag::public_key(mentioned_keys.public_key());
        let event = create_test_event_with_tags("Mentioning someone", vec![p_tag])?;
        let text_note = TextNote::new(event);

        let mut profiles = HashMap::new();
        profiles.insert(
            mentioned_keys.public_key(),
            create_test_profile("alice", Some("Alice")),
        );

        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let mentioned_names = widget.mentioned_names();

        assert_eq!(mentioned_names.len(), 1);
        assert_eq!(mentioned_names[0], "Alice");

        Ok(())
    }

    #[test]
    fn test_mentioned_names_without_profiles() -> Result<(), Box<dyn Error>> {
        let mentioned_keys = Keys::generate();
        let p_tag = Tag::public_key(mentioned_keys.public_key());
        let event = create_test_event_with_tags("Mentioning someone", vec![p_tag])?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let mentioned_names = widget.mentioned_names();

        assert_eq!(mentioned_names.len(), 1);
        // Should return shortened npub when profile not found
        assert!(mentioned_names[0].contains(':'));

        Ok(())
    }

    #[test]
    fn test_mentioned_names_multiple_mentions() -> Result<(), Box<dyn Error>> {
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let keys3 = Keys::generate();

        let tags = vec![
            Tag::public_key(keys1.public_key()),
            Tag::public_key(keys2.public_key()),
            Tag::public_key(keys3.public_key()),
        ];

        let event = create_test_event_with_tags("Mentioning multiple people", tags)?;
        let text_note = TextNote::new(event);

        let mut profiles = HashMap::new();
        profiles.insert(keys1.public_key(), create_test_profile("alice", None));
        profiles.insert(keys2.public_key(), create_test_profile("bob", None));
        // keys3 intentionally not in profiles

        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let mentioned_names = widget.mentioned_names();

        assert_eq!(mentioned_names.len(), 3);
        // Profile::name() returns handle with @ when no display_name is set
        assert_eq!(mentioned_names[0], "@alice");
        assert_eq!(mentioned_names[1], "@bob");
        // Third should be shortened npub
        assert!(mentioned_names[2].contains(':'));

        Ok(())
    }

    #[test]
    fn test_mentioned_names_empty() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("No mentions")?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let mentioned_names = widget.mentioned_names();

        assert_eq!(mentioned_names.len(), 0);

        Ok(())
    }

    #[test]
    fn test_calculate_height_without_reply() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Short content")?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let area = Rect::new(0, 0, 80, 20);
        let padding = Padding::ZERO;

        let height = widget.calculate_height(&area, padding);

        // Without reply: name + content + created_at + stats + separator = at least 5 lines
        // (4 fixed + content height)
        assert!(height >= 4);

        Ok(())
    }

    #[test]
    fn test_calculate_height_with_reply() -> Result<(), Box<dyn Error>> {
        let original_event = create_test_event("Original")?;
        let reply_event =
            create_test_event_with_tags("Reply", vec![Tag::event(original_event.id)])?;
        let text_note = TextNote::new(reply_event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let area = Rect::new(0, 0, 80, 20);
        let padding = Padding::ZERO;

        let height = widget.calculate_height(&area, padding);

        // With reply: annotation + name + content + created_at + stats + separator = at least 6 lines
        // (5 fixed + content height)
        assert!(height >= 5);

        Ok(())
    }

    #[test]
    fn test_calculate_height_with_padding() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Test")?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let area = Rect::new(0, 0, 80, 20);
        let padding = Padding::new(1, 1, 2, 2);

        let height_with_padding = widget.calculate_height(&area, padding);

        let padding_zero = Padding::ZERO;
        let height_no_padding = widget.calculate_height(&area, padding_zero);

        // Height should be the same regardless of padding (padding only affects internal calculations)
        // The function returns content height, not including padding
        assert!(height_with_padding > 0);
        assert!(height_no_padding > 0);

        Ok(())
    }

    #[test]
    fn test_calculate_height_long_content() -> Result<(), Box<dyn Error>> {
        let long_content = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(10);
        let event = create_test_event(&long_content)?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let area = Rect::new(0, 0, 40, 20); // Narrow width forces wrapping
        let padding = Padding::ZERO;

        let height = widget.calculate_height(&area, padding);

        // Long content with narrow width should result in multiple lines
        assert!(height > 10);

        Ok(())
    }

    #[test]
    fn test_widget_new() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Test content")?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let content = text_note.content().clone();
        let widget = TextNoteWidget::new(text_note, ctx);

        // Widget should be created successfully
        assert_eq!(widget.text_note.content(), &content);

        Ok(())
    }

    #[test]
    fn test_render_does_not_panic() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Test render")?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        // Render should not panic
        widget.render(area, &mut buffer);

        Ok(())
    }

    #[test]
    fn test_render_with_reply_tag() -> Result<(), Box<dyn Error>> {
        let original_event = create_test_event("Original")?;
        let reply_event =
            create_test_event_with_tags("Reply content", vec![Tag::event(original_event.id)])?;
        let text_note = TextNote::new(reply_event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        // Render with reply tag should not panic
        widget.render(area, &mut buffer);

        Ok(())
    }

    #[test]
    fn test_render_with_client_tag() -> Result<(), Box<dyn Error>> {
        let client_tag = Tag::custom(TagKind::Client, vec!["TestClient", "https://test.com"]);
        let event = create_test_event_with_tags("Test", vec![client_tag])?;
        let text_note = TextNote::new(event);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
            live_status: None,
            selected: false,
        };

        let widget = TextNoteWidget::new(text_note, ctx);
        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        // Render with client tag should not panic
        widget.render(area, &mut buffer);

        Ok(())
    }
}
