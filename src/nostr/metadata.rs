use serde::{Deserialize, Serialize};

use nostr_sdk::prelude::{Metadata as NostrMetadata, *};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub about: Option<String>,
    pub nip05: Option<String>,
}

impl Metadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name<T>(self, name: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            name: Some(name.into()),
            ..self
        }
    }

    pub fn display_name<T>(self, display_name: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            display_name: Some(display_name.into()),
            ..self
        }
    }

    pub fn about<T>(self, about: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            about: Some(about.into()),
            ..self
        }
    }

    pub fn nip05<T>(self, nip05: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            nip05: Some(nip05.into()),
            ..self
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Json(serde_json::Error),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<Metadata> for NostrMetadata {
    fn from(value: Metadata) -> Self {
        let mut metadata = NostrMetadata::new();

        if let Some(ref name) = value.name {
            metadata = metadata.name(name);
        }
        if let Some(ref display_name) = value.display_name {
            metadata = metadata.display_name(display_name);
        }
        if let Some(ref about) = value.about {
            metadata = metadata.about(about);
        }
        if let Some(ref nip05) = value.nip05 {
            metadata = metadata.nip05(nip05);
        }

        metadata
    }
}

impl JsonUtil for Metadata {
    type Err = Error;
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_from_json() {
        let json = r#"
            {
              "nip05": "_@0m1.io",
              "lud16": "akiomik@getalby.com",
              "picture": "https://image.nostr.build/7e99a1f30f59aa981057a5a910a62d43e600cd5bbc47f00130ca284f1914cc27.jpg",
              "website": "https://github.com/akiomik",
              "about": "鎌倉→軽井沢\nBird lover.\n~\nNostrends: https://nostrends.vercel.app\nNosli: https://nosli.vercel.app\nNosey: https://nosey.vercel.app\nnosvelte: https://github.com/akiomik/nosvelte\n~\nenglish: npub12gtrhfv04634qsyfm6l3m7a06l04qta6yefkuwezwcw6z4qe5nvqddy5qj",
              "name": "omi",
              "display_name": "kamakura",
              "displayName": "foobar",
              "banner": "https://github.com/akiomik/akiomik.github.io/raw/main/raindrop/raindrop.gif",
              "created_at": 1689299849,
              "nip05valid": true,
              "identities": [
                {
                  "type": "github",
                  "claim": "akiomik",
                  "proof": "https://github.com/akiomik"
                }
              ]
            }
        "#;
        let actual = Metadata::from_json(json).unwrap();
        let expected = Metadata::new().name("omi").display_name("kamakura").about("鎌倉→軽井沢\nBird lover.\n~\nNostrends: https://nostrends.vercel.app\nNosli: https://nosli.vercel.app\nNosey: https://nosey.vercel.app\nnosvelte: https://github.com/akiomik/nosvelte\n~\nenglish: npub12gtrhfv04634qsyfm6l3m7a06l04qta6yefkuwezwcw6z4qe5nvqddy5qj").nip05("_@0m1.io");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_for_nostr_metadata() {
        let actual = Metadata::new().name("omi").display_name("kamakura").about("鎌倉→軽井沢\nBird lover.\n~\nNostrends: https://nostrends.vercel.app\nNosli: https://nosli.vercel.app\nNosey: https://nosey.vercel.app\nnosvelte: https://github.com/akiomik/nosvelte\n~\nenglish: npub12gtrhfv04634qsyfm6l3m7a06l04qta6yefkuwezwcw6z4qe5nvqddy5qj").nip05("_@0m1.io");
        let expected = NostrMetadata::new().name("omi").display_name("kamakura").about("鎌倉→軽井沢\nBird lover.\n~\nNostrends: https://nostrends.vercel.app\nNosli: https://nosli.vercel.app\nNosey: https://nosey.vercel.app\nnosvelte: https://github.com/akiomik/nosvelte\n~\nenglish: npub12gtrhfv04634qsyfm6l3m7a06l04qta6yefkuwezwcw6z4qe5nvqddy5qj").nip05("_@0m1.io");
        assert_eq!(NostrMetadata::from(actual), expected);
    }
}
