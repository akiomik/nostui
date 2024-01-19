use nostr_sdk::prelude::*;

pub struct ReplyTagsBuilder {}

impl ReplyTagsBuilder {
    pub fn build(reply_to: Event) -> Vec<Tag> {
        let (mut etags, mut ptags, rest_tags): (Vec<Tag>, Vec<Tag>, Vec<Tag>) = reply_to
            .tags
            .iter()
            .fold((vec![], vec![], vec![]), |mut acc, tag| {
                match tag {
                    Tag::Event {
                        event_id,
                        relay_url,
                        marker,
                    } => {
                        if let Some(Marker::Reply) = marker {
                            acc.0.push(Tag::Event {
                                event_id: *event_id,
                                relay_url: relay_url.clone(),
                                marker: None,
                            })
                        } else {
                            acc.0.push(tag.clone())
                        }
                    }
                    Tag::PublicKey { .. } => acc.1.push(tag.clone()),
                    _ => acc.2.push(tag.clone()),
                }

                acc
            });

        let marker = if etags.is_empty() {
            Some(Marker::Root)
        } else {
            Some(Marker::Reply)
        };

        etags.push(Tag::Event {
            event_id: reply_to.id,
            relay_url: None,
            marker,
        });

        if !ptags
            .iter()
            .any(|tag| matches!(tag, Tag::PublicKey { public_key, .. } if *public_key == reply_to.pubkey))
        {
            ptags.push(Tag::PublicKey {
                public_key: reply_to.pubkey,
                relay_url: None,
                alias: None,
                uppercase: false,
            });
        }

        [etags, ptags, rest_tags].concat()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::*;

    #[fixture]
    fn root_event() -> Event {
        Event::from_json(
            r#"{
              "kind": 1,
              "id": "03aafbdec84e4cbbbe3cd1811d45f16a0b55214b0b72097851c3618f73638cf0",
              "pubkey": "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
              "tags": [],
              "content": "リプライしたい",
              "sig": "a183ace35e8c4704d8fb9c388858edbc71f2d795d1c18efaf8604512b4330d5f553adf3b25b97a66450bddfa7666a0db12e66d5e0032c26a6d6c7ee77d0b0535",
              "created_at": 1705129933
            }"#,
        ).unwrap()
    }

    #[fixture]
    fn reply_event() -> Event {
        Event::from_json(
            r#"{
              "pubkey": "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
              "content": "rabbitからリプライ",
              "id": "d444f485b5d401ee64564e4cc2bca7d9a50ad5ec628191470c009490ed1d43c3",
              "created_at": 1705133557,
              "sig": "06653b51cd5e081e1005ebb19c52cb666c4ccb96e42d1db5352757c75aeacb2570b3415696b8edbab977cfb131ff43f81f9f63cabf8eebc82bd1d585c90950f4",
              "kind": 1,
              "tags": [
                [
                  "e",
                  "03aafbdec84e4cbbbe3cd1811d45f16a0b55214b0b72097851c3618f73638cf0",
                  "",
                  "root"
                ],
                [
                  "p",
                  "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25"
                ]
              ]
            }"#,
        ).unwrap()
    }

    #[fixture]
    fn tag_event() -> Event {
        Event::from_json(
            r#"{
              "pubkey": "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
              "content": "\\#nostr タグを入れたリプライ",
              "id": "5d6468d901f4b933b3b71c1ad9761226121de929ba3351a28973a3ba1cab05f2",
              "created_at": 1705133866,
              "sig": "94f919c666d52d843bf98830a31a9aa8fb331aeaf84f9124f6aafed40548a92bcedf094da85d94ae481140f16d9c11d97421c13c1e3365b517edbe046e05b2b7",
              "kind": 1,
              "tags": [
                [
                  "e",
                  "03aafbdec84e4cbbbe3cd1811d45f16a0b55214b0b72097851c3618f73638cf0",
                  "",
                  "root"
                ],
                [
                  "e",
                  "d444f485b5d401ee64564e4cc2bca7d9a50ad5ec628191470c009490ed1d43c3",
                  "",
                  "reply"
                ],
                [
                  "p",
                  "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25"
                ],
                [
                  "t",
                  "nostr"
                ]
              ]
            }"#
        )
        .unwrap()
    }

    #[rstest]
    fn test_reply_tags_builder_build_root(root_event: Event) {
        let expected = vec![
            Tag::Event {
                event_id: EventId::from_hex(
                    "03aafbdec84e4cbbbe3cd1811d45f16a0b55214b0b72097851c3618f73638cf0",
                )
                .unwrap(),
                relay_url: None,
                marker: Some(Marker::Root),
            },
            Tag::PublicKey {
                public_key: XOnlyPublicKey::from_str(
                    "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
                )
                .unwrap(),
                relay_url: None,
                alias: None,
                uppercase: false,
            },
        ];
        assert_eq!(ReplyTagsBuilder::build(root_event), expected);
    }

    #[rstest]
    fn test_reply_tags_builder_build_reply(reply_event: Event) {
        let expected = vec![
            Tag::Event {
                event_id: EventId::from_hex(
                    "03aafbdec84e4cbbbe3cd1811d45f16a0b55214b0b72097851c3618f73638cf0",
                )
                .unwrap(),
                relay_url: None,
                marker: Some(Marker::Root),
            },
            Tag::Event {
                event_id: EventId::from_hex(
                    "d444f485b5d401ee64564e4cc2bca7d9a50ad5ec628191470c009490ed1d43c3",
                )
                .unwrap(),
                relay_url: None,
                marker: Some(Marker::Reply),
            },
            Tag::PublicKey {
                public_key: XOnlyPublicKey::from_str(
                    "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
                )
                .unwrap(),
                relay_url: None,
                alias: None,
                uppercase: false,
            },
        ];
        assert_eq!(ReplyTagsBuilder::build(reply_event), expected);
    }

    #[rstest]
    fn test_reply_tags_builder_build_tag(tag_event: Event) {
        let expected = vec![
            Tag::Event {
                event_id: EventId::from_hex(
                    "03aafbdec84e4cbbbe3cd1811d45f16a0b55214b0b72097851c3618f73638cf0",
                )
                .unwrap(),
                relay_url: None,
                marker: Some(Marker::Root),
            },
            Tag::Event {
                event_id: EventId::from_hex(
                    "d444f485b5d401ee64564e4cc2bca7d9a50ad5ec628191470c009490ed1d43c3",
                )
                .unwrap(),
                relay_url: None,
                marker: None,
            },
            Tag::Event {
                event_id: EventId::from_hex(
                    "5d6468d901f4b933b3b71c1ad9761226121de929ba3351a28973a3ba1cab05f2",
                )
                .unwrap(),
                relay_url: None,
                marker: Some(Marker::Reply),
            },
            Tag::PublicKey {
                public_key: XOnlyPublicKey::from_str(
                    "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
                )
                .unwrap(),
                relay_url: None,
                alias: None,
                uppercase: false,
            },
            Tag::Hashtag(String::from("nostr")),
        ];
        assert_eq!(ReplyTagsBuilder::build(tag_event), expected);
    }
}
