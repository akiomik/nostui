use std::sync::LazyLock;

use nostr_sdk::prelude::*;
use regex::Regex;

/// The delimiters are word boundaries rather than characters the match consumes, so a mention
/// still counts when it opens or closes the note, and when only one space separates it from the
/// next one.
///
/// `(?-u:\b)` makes those boundaries ASCII-only. A bech32 URI is ASCII, so what has to be ruled
/// out is a URI running into surrounding ASCII; Japanese writes no space before a mention, and a
/// Unicode boundary would treat `こんにちはnostr:npub1…さん` as one word and find nothing in it.
static REFERENCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u:\b)nostr:(?:npub|note)1[a-z0-9]{58}(?-u:\b)")
        .expect("hardcoded NIP-27 reference regex must be valid")
});

#[derive(Debug, PartialEq, Eq)]
pub struct Reference {
    // TODO: Add search index
    nip21: Nip21,
    value: String,
}

impl Reference {
    pub fn new(nip21: Nip21, value: String) -> Self {
        Self { nip21, value }
    }

    pub fn find(text: &str) -> Vec<Self> {
        // TODO: Add nevent and nprofile support
        REFERENCE_PATTERN
            .find_iter(text)
            .filter_map(|matched| {
                let uri = matched.as_str();

                Nip21::parse(uri)
                    .ok()
                    .map(|nip21| Reference::new(nip21, uri.to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::*;

    #[rstest]
    #[case("", vec![])]
    #[case("Hello, world!", vec![])]
    #[case("Hello, npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug!", vec![])]
    #[case("Hello, note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv!", vec![])]
    #[case("Hello, foobarnostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug!", vec![])]
    #[case("Hello, foobarnostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv!", vec![])]
    #[case("Hello, nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmugfoobar!", vec![])]
    #[case("Hello, nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyvfoobar!", vec![])]
    #[case(
        "Hello, nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug!",
        vec![
            Reference::new(
                Nip21::Pubkey(PublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            )
        ])
    ]
    #[case(
        "Hello, nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv!",
        vec![
            Reference::new(
                Nip21::EventId(EventId::from_nostr_uri("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv").unwrap()),
                String::from("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv")
            )
        ])
    ]
    #[case(
        r#"
            Hello, nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug and nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug!
            nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv
        "#,
        vec![
            Reference::new(
                Nip21::Pubkey(PublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
            Reference::new(
                Nip21::Pubkey(PublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
            Reference::new(
                Nip21::EventId(EventId::from_nostr_uri("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv").unwrap()),
                String::from("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv")
            )
        ])
    ]
    #[case(
        "nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug",
        vec![
            Reference::new(
                Nip21::Pubkey(PublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
        ])
    ]
    #[case(
        "Hello, nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv",
        vec![
            Reference::new(
                Nip21::EventId(EventId::from_nostr_uri("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv").unwrap()),
                String::from("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv")
            ),
        ])
    ]
    #[case(
        "Hello, nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv!",
        vec![
            Reference::new(
                Nip21::Pubkey(PublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
            Reference::new(
                Nip21::EventId(EventId::from_nostr_uri("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv").unwrap()),
                String::from("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv")
            ),
        ])
    ]
    #[case(
        "こんにちはnostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug",
        vec![
            Reference::new(
                Nip21::Pubkey(PublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
        ])
    ]
    #[case(
        "nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmugさん、ありがとう",
        vec![
            Reference::new(
                Nip21::Pubkey(PublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
        ])
    ]
    #[allow(clippy::unwrap_used)]
    fn find_extracts_recognised_references_in_order_and_rejects_the_rest(
        #[case] content: &str,
        #[case] expected: Vec<Reference>,
    ) {
        assert_eq!(Reference::find(content), expected);
    }
}
