use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets::*};

use crate::action::Action;
use crate::components::Component;
use crate::nostr::Metadata;
use crate::tui::Frame;
use crate::widgets::PublicKey;

pub struct StatusBar {
    pubkey: XOnlyPublicKey,
    metadata: Option<Metadata>,
    message: Option<String>,
    is_loading: bool,
}

impl StatusBar {
    pub fn new(
        pubkey: XOnlyPublicKey,
        metadata: Option<Metadata>,
        message: Option<String>,
        is_loading: bool,
    ) -> Self {
        Self {
            pubkey,
            metadata,
            message,
            is_loading,
        }
    }

    pub fn set_metadata(&mut self, metadata: Option<Metadata>) {
        self.metadata = metadata;
    }

    pub fn name(&self) -> String {
        self.metadata
            .clone()
            .and_then(|metadata| match (metadata.name, metadata.display_name) {
                (Some(name), _) if !name.is_empty() => Some(format!("@{name}")),
                (_, Some(display_name)) if !display_name.is_empty() => Some(display_name),
                (_, _) => None,
            })
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
                        let maybe_metadata = Metadata::from_json(ev.content);
                        if let Ok(metadata) = maybe_metadata {
                            self.set_metadata(Some(metadata));
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
