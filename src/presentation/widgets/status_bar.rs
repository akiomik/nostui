use crate::{
    domain::{nostr::Profile, text::shorten_npub},
    model::status_bar::StatusBar,
};

use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets::Paragraph};

#[derive(Debug, Clone, PartialEq)]
pub struct ViewContext<'a> {
    pub user_pubkey: PublicKey,
    pub user_profile: Option<&'a Profile>,
}

pub struct StatusBarWidget<'a> {
    status_bar: StatusBar,
    ctx: ViewContext<'a>,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(status_bar: StatusBar, ctx: ViewContext<'a>) -> Self {
        Self { status_bar, ctx }
    }

    pub fn user_name(&self) -> String {
        if let Some(profile) = self.ctx.user_profile {
            profile.name()
        } else {
            let Ok(npub) = self.ctx.user_pubkey.to_bech32();
            shorten_npub(npub)
        }
    }
}

impl<'a> Widget for StatusBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::new(
            Direction::Vertical,
            [
                Constraint::Min(0),    // Main content area (not used by status bar)
                Constraint::Length(1), // User info line
                Constraint::Length(1), // Status message line
            ],
        )
        .split(area);

        // Render user info line
        let name_span = Span::styled(self.user_name(), Style::default().fg(Color::Gray).italic());
        Paragraph::new(name_span)
            .style(Style::default().bg(Color::Black))
            .render(layout[1], buf);

        // Render status message line
        let message = match self.status_bar.message() {
            Some(message) => message.clone(),
            None => "".to_string(),
        };
        Paragraph::new(message).render(layout[2], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::nostr::Profile, model::status_bar::Message};

    fn create_test_pubkey() -> PublicKey {
        // Use a fixed test public key
        PublicKey::from_hex("4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25")
            .expect("valid public key")
    }

    fn create_test_profile(
        pubkey: PublicKey,
        display_name: Option<&str>,
        name: Option<&str>,
    ) -> Profile {
        let mut metadata = Metadata::new();
        if let Some(dn) = display_name {
            metadata = metadata.display_name(dn);
        }
        if let Some(n) = name {
            metadata = metadata.name(n);
        }
        Profile::new(pubkey, Timestamp::now(), metadata)
    }

    #[test]
    fn test_view_context_clone() {
        let pubkey = create_test_pubkey();
        let profile = create_test_profile(pubkey, Some("Alice"), None);
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let cloned = ctx.clone();
        assert_eq!(ctx, cloned);
    }

    #[test]
    fn test_view_context_debug() {
        let pubkey = create_test_pubkey();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let debug_str = format!("{ctx:?}");
        assert!(debug_str.contains("ViewContext"));
        assert!(debug_str.contains("user_pubkey"));
        assert!(debug_str.contains("user_profile"));
    }

    #[test]
    fn test_status_bar_widget_new() {
        let pubkey = create_test_pubkey();
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar.clone(), ctx.clone());
        assert_eq!(widget.status_bar, status_bar);
        assert_eq!(widget.ctx, ctx);
    }

    #[test]
    fn test_user_name_with_display_name() {
        let pubkey = create_test_pubkey();
        let profile = create_test_profile(pubkey, Some("Alice"), Some("alice"));
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        assert_eq!(widget.user_name(), "Alice");
    }

    #[test]
    fn test_user_name_with_handle_only() {
        let pubkey = create_test_pubkey();
        let profile = create_test_profile(pubkey, None, Some("alice"));
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        assert_eq!(widget.user_name(), "@alice");
    }

    #[test]
    fn test_user_name_without_profile() {
        let pubkey = create_test_pubkey();
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let user_name = widget.user_name();
        // Should return shortened npub
        assert!(user_name.contains(":"));
        assert_eq!(user_name.len(), 11); // "xxxxx:xxxxx" format
    }

    #[test]
    fn test_user_name_with_empty_profile_metadata() {
        let pubkey = create_test_pubkey();
        let profile = create_test_profile(pubkey, None, None);
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let user_name = widget.user_name();
        // Should fallback to npub (not shortened in Profile::name())
        assert!(user_name.starts_with("npub1"));
    }

    #[test]
    fn test_user_name_with_empty_string_display_name() {
        let pubkey = create_test_pubkey();
        let profile = create_test_profile(pubkey, Some(""), Some("alice"));
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        // Empty display_name should be ignored, fallback to handle
        assert_eq!(widget.user_name(), "@alice");
    }

    #[test]
    fn test_user_name_with_empty_string_name() {
        let pubkey = create_test_pubkey();
        let profile = create_test_profile(pubkey, Some("Alice"), Some(""));
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        // Should use display_name
        assert_eq!(widget.user_name(), "Alice");
    }

    #[test]
    fn test_user_name_priority() {
        let pubkey = create_test_pubkey();
        // Test priority: display_name > handle > npub
        let profile = create_test_profile(pubkey, Some("Display Name"), Some("handle"));
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        assert_eq!(widget.user_name(), "Display Name");
    }

    #[test]
    fn test_render_does_not_panic() {
        let pubkey = create_test_pubkey();
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let area = Rect::new(0, 0, 80, 3);
        let mut buffer = Buffer::empty(area);

        // Render should not panic
        widget.render(area, &mut buffer);
    }

    #[test]
    fn test_render_with_message() {
        let pubkey = create_test_pubkey();
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::MessageChanged {
            label: "Info".to_string(),
            message: "Test message".to_string(),
        });
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let area = Rect::new(0, 0, 80, 3);
        let mut buffer = Buffer::empty(area);

        // Render with message should not panic
        widget.render(area, &mut buffer);

        // Check that message appears in buffer
        let message_line = buffer.content()[160..240]
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(message_line.contains("[Info] Test message"));
    }

    #[test]
    fn test_render_with_error_message() {
        let pubkey = create_test_pubkey();
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::ErrorMessageChanged {
            label: "Network".to_string(),
            message: "Connection failed".to_string(),
        });
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let area = Rect::new(0, 0, 80, 3);
        let mut buffer = Buffer::empty(area);

        // Render with error message should not panic
        widget.render(area, &mut buffer);

        // Check that error message appears in buffer
        let message_line = buffer.content()[160..240]
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(message_line.contains("[ERR: Network] Connection failed"));
    }

    #[test]
    fn test_render_with_profile() {
        let pubkey = create_test_pubkey();
        let profile = create_test_profile(pubkey, Some("Alice"), Some("alice"));
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: Some(&profile),
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let area = Rect::new(0, 0, 80, 3);
        let mut buffer = Buffer::empty(area);

        // Render with profile should not panic
        widget.render(area, &mut buffer);

        // Check that user name appears in buffer
        let user_line = buffer.content()[80..160]
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(user_line.contains("Alice"));
    }

    #[test]
    fn test_render_empty_message() {
        let pubkey = create_test_pubkey();
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let area = Rect::new(0, 0, 80, 3);
        let mut buffer = Buffer::empty(area);

        // Render without message should not panic
        widget.render(area, &mut buffer);

        // Message line should be empty
        let message_line = buffer.content()[160..240]
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert_eq!(message_line.trim(), "");
    }

    #[test]
    fn test_render_small_area() {
        let pubkey = create_test_pubkey();
        let status_bar = StatusBar::default();
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let area = Rect::new(0, 0, 20, 3);
        let mut buffer = Buffer::empty(area);

        // Render with small area should not panic
        widget.render(area, &mut buffer);
    }

    #[test]
    fn test_render_large_area() {
        let pubkey = create_test_pubkey();
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::MessageChanged {
            label: "Info".to_string(),
            message: "Test".to_string(),
        });
        let ctx = ViewContext {
            user_pubkey: pubkey,
            user_profile: None,
        };
        let widget = StatusBarWidget::new(status_bar, ctx);
        let area = Rect::new(0, 0, 200, 10);
        let mut buffer = Buffer::empty(area);

        // Render with large area should not panic
        widget.render(area, &mut buffer);
    }
}
