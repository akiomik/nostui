use ratatui::prelude::*;

use crate::{domain::nostr::Profile, presentation::widgets::public_key::PublicKey};

pub struct NameWithHandle {
    pubkey: nostr_sdk::PublicKey,
    display_name: Option<String>,
    handle: Option<String>,
    highlighted: bool,
}

impl NameWithHandle {
    pub fn new(pubkey: nostr_sdk::PublicKey, profile: &Option<Profile>, highlighted: bool) -> Self {
        Self {
            pubkey,
            display_name: profile
                .as_ref()
                .and_then(|profile| profile.display_name().cloned()),
            handle: profile.as_ref().and_then(|profile| profile.handle()),
            highlighted,
        }
    }

    pub fn display_name_style(&self) -> Style {
        if self.highlighted {
            Style::default().bold().reversed()
        } else {
            Style::default().bold()
        }
    }

    pub fn handle_style(&self) -> Style {
        if self.display_name.is_none() && self.highlighted {
            Style::default().italic().reversed()
        } else {
            Style::default().italic().fg(Color::Gray)
        }
    }
}

impl From<NameWithHandle> for Text<'static> {
    fn from(widget: NameWithHandle) -> Self {
        let display_name_style = widget.display_name_style();
        let handle_style = widget.handle_style();
        let pubkey = widget.pubkey;

        match (widget.display_name, widget.handle) {
            (Some(display_name), Some(handle))
                if handle.strip_prefix('@').is_some_and(|h| h != display_name) =>
            {
                Line::from(vec![
                    Span::styled(display_name, display_name_style),
                    Span::raw(" "),
                    Span::styled(handle, handle_style),
                ])
                .into()
            }
            (Some(display_name), _) => Span::styled(display_name, display_name_style).into(),
            (_, Some(handle)) => Span::styled(handle, handle_style).into(),
            (_, _) => Span::styled(PublicKey::new(pubkey).shortened(), display_name_style).into(),
        }
    }
}

impl Widget for NameWithHandle {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let text: Text = self.into();
        text.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nostr::Profile;
    use nostr_sdk::prelude::*;
    use pretty_assertions::assert_eq;
    use ratatui::buffer::Buffer;
    use ratatui::prelude::{Color, Modifier, Rect};

    #[test]
    fn test_from_no_profile() {
        // Test From trait with no profile
        let keys = Keys::generate();
        let widget = NameWithHandle::new(keys.public_key(), &None, false);

        let text: Text = widget.into();

        // Should contain shortened hex format
        let text_str = text.to_string();
        assert!(text_str.contains(':'), "Expected hex format with colon");
        assert_eq!(text_str.len(), 11, "Should be shortened format (5:5)");
    }

    #[test]
    fn test_from_display_name_only() {
        // Test From trait with display_name only
        let keys = Keys::generate();
        let metadata = Metadata::new().display_name("Alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), false);

        let text: Text = widget.into();

        assert_eq!(text.to_string(), "Alice");
    }

    #[test]
    fn test_from_display_name_and_handle() {
        // Test From trait with both display_name and handle
        let keys = Keys::generate();
        let metadata = Metadata::new().display_name("Alice Smith").name("alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), false);

        let text: Text = widget.into();

        let text_str = text.to_string();
        assert!(text_str.contains("Alice Smith"));
        assert!(text_str.contains("@alice"));
    }

    #[test]
    fn test_from_preserves_styles() {
        // Test that From preserves style information
        let keys = Keys::generate();
        let metadata = Metadata::new().display_name("Alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), true);

        let text: Text = widget.into();

        // Check that the text has lines with styled spans
        assert!(!text.lines.is_empty(), "Text should have lines");
        assert!(
            !text.lines[0].spans.is_empty(),
            "Line should have spans with styles"
        );

        // Verify the style contains BOLD and REVERSED
        let first_span = &text.lines[0].spans[0];
        assert!(
            first_span.style.add_modifier.contains(Modifier::BOLD),
            "Should be bold"
        );
        assert!(
            first_span.style.add_modifier.contains(Modifier::REVERSED),
            "Should be reversed when highlighted"
        );
    }

    #[test]
    fn test_no_profile_shows_hex() {
        // No profile - should show shortened hex public key
        let keys = Keys::generate();
        let widget = NameWithHandle::new(keys.public_key(), &None, false);

        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // Verify that content is displayed (shortened hex format: xxxxx:yyyyy)
        let content = buffer
            .content()
            .iter()
            .take(11)
            .map(|cell| cell.symbol())
            .collect::<String>();

        // Should contain the colon separator from shortened format
        assert!(content.contains(':'), "Expected hex format with colon");
    }

    #[test]
    fn test_display_name_only() {
        // Profile with display_name but no name (no handle)
        let keys = Keys::generate();
        let metadata = Metadata::new().display_name("Alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), false);

        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // Verify display name is shown
        let content = buffer
            .content()
            .iter()
            .take(5)
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert_eq!(content, "Alice");

        // Verify style is bold
        assert_eq!(buffer.content()[0].modifier, Modifier::BOLD);
    }

    #[test]
    fn test_handle_only() {
        // Profile with name (handle) but no display_name
        let keys = Keys::generate();
        let metadata = Metadata::new().name("alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), false);

        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // Verify handle is shown with @ prefix
        let content = buffer
            .content()
            .iter()
            .take(6)
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert_eq!(content, "@alice");

        // Verify style is italic and gray
        assert_eq!(buffer.content()[0].modifier, Modifier::ITALIC);
        assert_eq!(buffer.content()[0].fg, Color::Gray);
    }

    #[test]
    fn test_display_name_and_handle_different() {
        // Profile with both display_name and handle, and they're different
        let keys = Keys::generate();
        let metadata = Metadata::new().display_name("Alice Smith").name("alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), false);

        let area = Rect::new(0, 0, 30, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // Verify both display_name and handle are shown
        let content = buffer
            .content()
            .iter()
            .take(24)
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(
            content.contains("Alice Smith"),
            "Should contain display name"
        );
        assert!(content.contains("@alice"), "Should contain handle");

        // Verify first part is bold (display_name)
        assert_eq!(buffer.content()[0].modifier, Modifier::BOLD);
    }

    #[test]
    fn test_display_name_and_handle_same() {
        // Profile where display_name equals handle (without @)
        // Should only show display_name
        let keys = Keys::generate();
        let metadata = Metadata::new().display_name("alice").name("alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), false);

        let area = Rect::new(0, 0, 30, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // Verify only display_name is shown (not duplicated with handle)
        let content = buffer
            .content()
            .iter()
            .take(10)
            .map(|cell| cell.symbol())
            .collect::<String>();

        // Should show "alice" but not "@alice" after it
        assert!(content.starts_with("alice"));
        // Count occurrences - should only appear once
        let alice_count = content.matches("alice").count();
        assert_eq!(alice_count, 1, "Should only show name once");
    }

    #[test]
    fn test_highlighted_with_profile() {
        // Test highlighting when profile exists
        let keys = Keys::generate();
        let metadata = Metadata::new().display_name("Alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), true);

        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // Verify style is bold AND reversed
        let first_cell_modifier = buffer.content()[0].modifier;
        assert!(
            first_cell_modifier.contains(Modifier::BOLD),
            "Should be bold"
        );
        assert!(
            first_cell_modifier.contains(Modifier::REVERSED),
            "Should be reversed when highlighted"
        );
    }

    #[test]
    fn test_highlighted_without_profile() {
        // Test highlighting when no profile (hex display)
        let keys = Keys::generate();
        let widget = NameWithHandle::new(keys.public_key(), &None, true);

        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // Verify style is bold AND reversed (hex should also be highlighted)
        let first_cell_modifier = buffer.content()[0].modifier;
        assert!(
            first_cell_modifier.contains(Modifier::BOLD),
            "Hex should be bold"
        );
        assert!(
            first_cell_modifier.contains(Modifier::REVERSED),
            "Hex should be reversed when highlighted"
        );
    }

    #[test]
    fn test_highlighted_handle_only() {
        // Test highlighting when only handle exists (no display_name)
        let keys = Keys::generate();
        let metadata = Metadata::new().name("alice");
        let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);

        let widget = NameWithHandle::new(keys.public_key(), &Some(profile), true);

        let area = Rect::new(0, 0, 20, 1);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);

        // When no display_name but handle exists, should be italic AND reversed
        let first_cell_modifier = buffer.content()[0].modifier;
        assert!(
            first_cell_modifier.contains(Modifier::ITALIC),
            "Handle should be italic"
        );
        assert!(
            first_cell_modifier.contains(Modifier::REVERSED),
            "Handle should be reversed when highlighted and no display_name"
        );
    }
}
