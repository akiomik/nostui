use nostr_sdk::prelude::*;
use ratatui::prelude::*;

use crate::text::shorten_hex;

pub struct PublicKey {
    key: XOnlyPublicKey,
}

impl PublicKey {
    pub fn new(key: XOnlyPublicKey) -> Self {
        Self { key }
    }

    pub fn shortened(&self) -> String {
        shorten_hex(&self.key.to_string())
    }
}

impl<'a> From<PublicKey> for Text<'a> {
    fn from(value: PublicKey) -> Self {
        Text::from(value.shortened())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::*;

    #[rstest]
    fn test_shortened() {
        let key = XOnlyPublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )
        .unwrap();
        let publickey = PublicKey::new(key);
        assert_eq!(publickey.shortened(), "4d39c:aae25");
    }
}
