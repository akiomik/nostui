use nostr_sdk::prelude::*;
use regex::Regex;

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
        let pattern = Regex::new(r"[^\w](nostr:(npub|note)1[a-z0-9]{58})[^\w]").unwrap();
        pattern
            .captures_iter(text)
            .filter_map(|capture| {
                let (_, [uri, _]) = capture.extract();

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
                Nip21::Pubkey(XOnlyPublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
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
                Nip21::Pubkey(XOnlyPublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
            Reference::new(
                Nip21::Pubkey(XOnlyPublicKey::from_nostr_uri("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug").unwrap()),
                String::from("nostr:npub1f5uuywemqwlejj2d7he6zjw8jz9wr0r5z6q8lhttxj333ph24cjsymjmug")
            ),
            Reference::new(
                Nip21::EventId(EventId::from_nostr_uri("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv").unwrap()),
                String::from("nostr:note1jnnkqfzn70k6z94nwljdnaw5s5pd8jlf0eyjfmc2pvsytvsa7unsex9dyv")
            )
        ])
    ]
    fn test_parse(#[case] content: &str, #[case] expected: Vec<Reference>) {
        assert_eq!(Reference::find(content), expected);
    }
}
