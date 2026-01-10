use nostr_sdk::prelude::*;
use ratatui::prelude::*;

use crate::domain::text::shorten_npub;

pub struct PublicKey {
    key: nostr_sdk::PublicKey,
}

impl PublicKey {
    pub fn new(key: nostr_sdk::PublicKey) -> Self {
        Self { key }
    }

    pub fn shortened(&self) -> String {
        let Ok(npub) = self.key.to_bech32();
        shorten_npub(npub)
    }
}

impl From<PublicKey> for Text<'_> {
    fn from(value: PublicKey) -> Self {
        Text::from(value.shortened())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use color_eyre::eyre::Result;
    use pretty_assertions::assert_eq;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;

    #[test]
    fn test_new() -> Result<()> {
        let key = nostr_sdk::PublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )?;
        let publickey = PublicKey::new(key);

        assert_eq!(publickey.key, key);

        Ok(())
    }

    #[test]
    fn test_shortened() -> Result<()> {
        let key = nostr_sdk::PublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )?;
        let publickey = PublicKey::new(key);
        assert_eq!(publickey.shortened(), "4d39c:aae25");

        Ok(())
    }

    #[test]
    fn test_from_public_key_to_text() -> Result<()> {
        let key = nostr_sdk::PublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )?;
        let publickey = PublicKey::new(key);

        let text: Text = publickey.into();

        assert_eq!(text.to_string(), "4d39c:aae25");

        Ok(())
    }

    #[test]
    fn test_render_public_key_widget() -> Result<()> {
        let key = nostr_sdk::PublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )?;
        let publickey = PublicKey::new(key);

        let backend = TestBackend::new(20, 1);
        let mut terminal = Terminal::new(backend)?;

        terminal.draw(|frame| {
            let text: Text = publickey.into();
            let paragraph = ratatui::widgets::Paragraph::new(text);
            frame.render_widget(paragraph, frame.area());
        })?;

        let buffer = terminal.backend().buffer();
        let rendered_text = buffer
            .content()
            .iter()
            .take(11) // "4d39c:aae25" is 11 characters
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert_eq!(rendered_text, "4d39c:aae25");

        Ok(())
    }

    #[test]
    fn test_shortened_format_consistency() -> Result<()> {
        // Test multiple keys to ensure consistent formatting
        let keys = vec![
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
            "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ];

        for key_str in keys {
            let key = nostr_sdk::PublicKey::from_str(key_str)?;
            let publickey = PublicKey::new(key);
            let shortened = publickey.shortened();

            // All shortened keys should have the format: 5 chars + ':' + 5 chars = 11 total
            assert_eq!(
                shortened.len(),
                11,
                "Shortened key should be 11 characters: {shortened}"
            );
            assert!(
                shortened.contains(':'),
                "Shortened key should contain ':': {shortened}"
            );

            let parts: Vec<&str> = shortened.split(':').collect();
            assert_eq!(parts.len(), 2, "Should have exactly 2 parts");
            assert_eq!(parts[0].len(), 5, "First part should be 5 chars");
            assert_eq!(parts[1].len(), 5, "Second part should be 5 chars");
        }

        Ok(())
    }
}
