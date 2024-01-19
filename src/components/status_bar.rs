use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets::*};

use crate::action::Action;
use crate::components::Component;
use crate::nostr::Profile;
use crate::tui::Frame;
use crate::widgets::PublicKey;

pub struct StatusBar {
    pubkey: XOnlyPublicKey,
    profile: Option<Profile>,
    message: Option<String>,
    is_loading: bool,
}

impl StatusBar {
    pub fn new(
        pubkey: XOnlyPublicKey,
        profile: Option<Profile>,
        message: Option<String>,
        is_loading: bool,
    ) -> Self {
        Self {
            pubkey,
            profile,
            message,
            is_loading,
        }
    }

    pub fn set_profile(&mut self, profile: Option<Profile>) {
        self.profile = profile;
    }

    pub fn name(&self) -> String {
        self.profile
            .clone()
            .map(|profile| profile.name())
            .unwrap_or(PublicKey::new(self.pubkey).shortened())
    }
}

impl Component for StatusBar {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::ReceiveEvent(ev) => {
                self.is_loading = false;

                match ev.kind {
                    Kind::Metadata if ev.pubkey == self.pubkey => {
                        if let Ok(metadata) = Metadata::from_json(ev.content.clone()) {
                            let profile = Profile::new(ev.pubkey, ev.created_at, metadata);
                            if let Some(existing_profile) = &self.profile {
                                if existing_profile.created_at > profile.created_at {
                                    // TODO
                                }
                            }

                            self.set_profile(Some(profile));
                        }
                    }
                    _ => {}
                };
            }
            Action::SystemMessage(message) => self.message = Some(message),
            _ => {}
        };

        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let layout = Layout::new(
            Direction::Vertical,
            [
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(1),
            ],
        )
        .split(area);
        f.render_widget(Clear, layout[1]);
        f.render_widget(Clear, layout[2]);

        let name = Span::styled(self.name(), Style::default().fg(Color::Gray).italic());
        let status_line = Paragraph::new(name).style(Style::default().bg(Color::Black));
        f.render_widget(status_line, layout[1]);

        let message_line = if self.is_loading {
            Paragraph::new("Loading...")
        } else {
            Paragraph::new(self.message.clone().unwrap_or_default())
        };
        f.render_widget(message_line, layout[2]);

        Ok(())
    }
}
