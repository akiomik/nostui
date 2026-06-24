use std::collections::HashMap;

use nostr_sdk::prelude::*;
use ratatui::prelude::*;
use ratatui::widgets::Widget;

use crate::{domain::nostr::Profile, model::timeline::Timeline};

#[derive(Clone)]
pub struct ViewContext<'a> {
    pub profiles: &'a HashMap<PublicKey, Profile>,
}

#[derive(Clone)]
pub struct TabBarWidget<'a> {
    timeline: &'a Timeline,
    ctx: ViewContext<'a>,
}

impl<'a> TabBarWidget<'a> {
    pub fn new(timeline: &'a Timeline, ctx: ViewContext<'a>) -> Self {
        Self { timeline, ctx }
    }

    pub fn titles(&self) -> Vec<String> {
        self.timeline
            .tabs()
            .iter()
            .map(|tab| tab.tab_title(self.ctx.profiles))
            .collect()
    }
}

impl<'a> Widget for TabBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let tabs = ratatui::widgets::Tabs::new(self.titles())
            .select(self.timeline.active_tab_index())
            .style(Style::default().bg(Color::Black))
            .highlight_style(Style::default().reversed());

        tabs.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nostr::Profile;
    use crate::model::timeline::Message;
    use crate::model::timeline::{tab::TimelineTabType, Timeline};
    use nostr_sdk::nostr::Metadata;

    fn create_test_pubkey() -> PublicKey {
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
        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let cloned = ctx.clone();
        assert_eq!(ctx.profiles.len(), cloned.profiles.len());
    }

    #[test]
    fn test_tab_bar_widget_new() {
        let timeline = Timeline::default();
        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx.clone());
        assert_eq!(widget.timeline.tabs().len(), 1);
    }

    #[test]
    fn test_titles_single_home_tab() {
        let timeline = Timeline::default();
        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let titles = widget.titles();
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0], "Home");
    }

    #[test]
    fn test_titles_multiple_tabs() {
        let mut timeline = Timeline::default();
        let pubkey = create_test_pubkey();

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let titles = widget.titles();
        assert_eq!(titles.len(), 2);
        assert_eq!(titles[0], "Home");
        // Without profile, should show shortened npub
        assert!(titles[1].contains(':'));
        assert_eq!(titles[1].len(), 11); // "xxxxx:xxxxx" format
    }

    #[test]
    fn test_titles_with_profile_display_name() {
        let mut timeline = Timeline::default();
        let pubkey = create_test_pubkey();

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        let mut profiles = HashMap::new();
        profiles.insert(
            pubkey,
            create_test_profile(pubkey, Some("Alice"), Some("alice")),
        );

        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let titles = widget.titles();
        assert_eq!(titles.len(), 2);
        assert_eq!(titles[0], "Home");
        // handle() returns @name, not display_name
        assert_eq!(titles[1], "@alice");
    }

    #[test]
    fn test_titles_with_profile_handle_only() {
        let mut timeline = Timeline::default();
        let pubkey = create_test_pubkey();

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        let mut profiles = HashMap::new();
        profiles.insert(pubkey, create_test_profile(pubkey, None, Some("alice")));

        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let titles = widget.titles();
        assert_eq!(titles.len(), 2);
        assert_eq!(titles[0], "Home");
        assert_eq!(titles[1], "@alice");
    }

    #[test]
    fn test_titles_with_empty_profile_metadata() {
        let mut timeline = Timeline::default();
        let pubkey = create_test_pubkey();

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        let mut profiles = HashMap::new();
        profiles.insert(pubkey, create_test_profile(pubkey, None, None));

        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let titles = widget.titles();
        assert_eq!(titles.len(), 2);
        assert_eq!(titles[0], "Home");
        // handle() returns None for empty profile, should fallback to shortened npub
        assert!(titles[1].contains(':'));
        assert_eq!(titles[1].len(), 11); // "xxxxx:xxxxx" format
    }

    #[test]
    fn test_titles_multiple_user_tabs() {
        let mut timeline = Timeline::default();
        let pubkey1 = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let pubkey2 = PublicKey::from_slice(&[2u8; 32]).expect("Valid pubkey");

        // Add two user timeline tabs
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey: pubkey1 },
        });
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey: pubkey2 },
        });

        let mut profiles = HashMap::new();
        profiles.insert(
            pubkey1,
            create_test_profile(pubkey1, Some("Alice"), Some("alice")),
        );
        profiles.insert(
            pubkey2,
            create_test_profile(pubkey2, Some("Bob"), Some("bob")),
        );

        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let titles = widget.titles();
        assert_eq!(titles.len(), 3);
        assert_eq!(titles[0], "Home");
        // handle() returns @name when name is present
        assert_eq!(titles[1], "@alice");
        assert_eq!(titles[2], "@bob");
    }

    #[test]
    fn test_render_does_not_panic() {
        let timeline = Timeline::default();
        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);

        // Render should not panic
        widget.render(area, &mut buffer);
    }

    #[test]
    fn test_render_with_multiple_tabs() {
        let mut timeline = Timeline::default();
        let pubkey = create_test_pubkey();

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        let mut profiles = HashMap::new();
        profiles.insert(
            pubkey,
            create_test_profile(pubkey, Some("Alice"), Some("alice")),
        );

        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);

        // Render with multiple tabs should not panic
        widget.render(area, &mut buffer);

        // Check that tab titles appear in buffer
        let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Home"));
        // handle() returns @alice
        assert!(content.contains("alice"));
    }

    #[test]
    fn test_render_with_active_tab() {
        let mut timeline = Timeline::default();
        let pubkey = create_test_pubkey();

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Active tab should be the newly added tab (index 1)
        assert_eq!(timeline.active_tab_index(), 1);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);

        // Render should not panic
        widget.render(area, &mut buffer);
    }

    #[test]
    fn test_render_small_area() {
        let timeline = Timeline::default();
        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let area = Rect::new(0, 0, 10, 1);
        let mut buffer = Buffer::empty(area);

        // Render with small area should not panic
        widget.render(area, &mut buffer);
    }

    #[test]
    fn test_render_large_area() {
        let timeline = Timeline::default();
        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let area = Rect::new(0, 0, 200, 1);
        let mut buffer = Buffer::empty(area);

        // Render with large area should not panic
        widget.render(area, &mut buffer);
    }

    #[test]
    fn test_render_zero_height() {
        let timeline = Timeline::default();
        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let area = Rect::new(0, 0, 80, 0);
        let mut buffer = Buffer::empty(area);

        // Render with zero height should not panic
        widget.render(area, &mut buffer);
    }

    #[test]
    fn test_render_switching_tabs() {
        let mut timeline = Timeline::default();
        let pubkey = create_test_pubkey();

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Switch to Home tab
        let _ = timeline.update(Message::TabSelected { index: 0 });
        assert_eq!(timeline.active_tab_index(), 0);

        let profiles = HashMap::new();
        let ctx = ViewContext {
            profiles: &profiles,
        };
        let widget = TabBarWidget::new(&timeline, ctx);
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);

        // Render with Home tab active should not panic
        widget.render(area, &mut buffer);
    }
}
