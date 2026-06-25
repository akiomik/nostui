//! Pure feed filter construction.
//!
//! This module isolates the logic that decides *which* Nostr events a feed
//! should request (kinds, author set, time window, limit) from the I/O that
//! actually performs the subscription. The functions here are pure and free of
//! any `Client` or framework dependency, which makes the filter rules
//! unit-testable.
//!
//! These builders are intentionally agnostic of UI concepts such as tabs: the
//! caller maps a `FeedKind` to the appropriate builder. This keeps the domain
//! layer free of dependencies on outer layers.

use nostr_sdk::prelude::*;

/// Maximum number of events fetched per feed page.
pub const DEFAULT_FEED_LIMIT: usize = 50;

/// Event kinds shown in the home feed.
pub const HOME_FEED_KINDS: [Kind; 4] = [
    Kind::TextNote,
    Kind::Repost,
    Kind::Reaction,
    Kind::ZapReceipt,
];

/// Event kinds shown in a single user's feed.
pub const USER_FEED_KINDS: [Kind; 2] = [Kind::TextNote, Kind::Repost];

/// Ensure the user's own pubkey is part of the author set so that their posts
/// always appear in the home feed, even when they do not follow themselves.
pub fn with_own_pubkey(mut authors: Vec<PublicKey>, own_pubkey: PublicKey) -> Vec<PublicKey> {
    if !authors.contains(&own_pubkey) {
        authors.push(own_pubkey);
    }
    authors
}

/// Build the three home-feed subscription filters.
///
/// Returns `[backward, forward, profile]`:
/// - `backward`: historical events up to `now` (bounded by the page limit)
/// - `forward`: real-time events from `now` onward
/// - `profile`: metadata for the same author set
pub fn home_feed_filters(authors: Vec<PublicKey>, now: Timestamp) -> [Filter; 3] {
    let backward = Filter::new()
        .authors(authors.clone())
        .kinds(HOME_FEED_KINDS)
        .until(now)
        .limit(DEFAULT_FEED_LIMIT);
    let forward = Filter::new()
        .authors(authors.clone())
        .kinds(HOME_FEED_KINDS)
        .since(now);
    let profile = Filter::new().authors(authors).kinds([Kind::Metadata]);

    [backward, forward, profile]
}

/// Build the backward + forward subscription filters for a user feed.
///
/// Returns `[backward, forward]`:
/// - `backward`: historical events up to `now` (bounded by the page limit)
/// - `forward`: real-time events from `now` onward
pub fn user_feed_filters(pubkey: PublicKey, now: Timestamp) -> [Filter; 2] {
    let backward = Filter::new()
        .authors(vec![pubkey])
        .kinds(USER_FEED_KINDS)
        .until(now)
        .limit(DEFAULT_FEED_LIMIT);
    let forward = Filter::new()
        .authors(vec![pubkey])
        .kinds(USER_FEED_KINDS)
        .since(now);

    [backward, forward]
}

/// Build the "load more" filter for paginating older home-feed events
/// before `since`, using the given author set.
pub fn home_load_more_filter(authors: Vec<PublicKey>, since: Timestamp) -> Filter {
    Filter::new()
        .authors(authors)
        .kinds(HOME_FEED_KINDS)
        .until(since)
        .limit(DEFAULT_FEED_LIMIT)
}

/// Build the "load more" filter for paginating older events before `since` on a
/// single user's feed.
pub fn user_load_more_filter(pubkey: PublicKey, since: Timestamp) -> Filter {
    Filter::new()
        .authors(vec![pubkey])
        .kinds(USER_FEED_KINDS)
        .until(since)
        .limit(DEFAULT_FEED_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pubkey(byte: u8) -> PublicKey {
        PublicKey::from_slice(&[byte; 32]).expect("valid public key")
    }

    #[test]
    fn test_with_own_pubkey_appends_when_absent() {
        let own = pubkey(1);
        let authors = vec![pubkey(2), pubkey(3)];

        assert_eq!(
            with_own_pubkey(authors, own),
            vec![pubkey(2), pubkey(3), own]
        );
    }

    #[test]
    fn test_with_own_pubkey_keeps_unchanged_when_present() {
        let own = pubkey(1);
        let authors = vec![pubkey(2), own, pubkey(3)];

        assert_eq!(
            with_own_pubkey(authors, own),
            vec![pubkey(2), own, pubkey(3)]
        );
    }

    #[test]
    fn test_with_own_pubkey_on_empty_authors() {
        let own = pubkey(1);

        assert_eq!(with_own_pubkey(Vec::new(), own), vec![own]);
    }

    #[test]
    fn test_home_feed_filters() {
        let authors = vec![pubkey(1), pubkey(2)];
        let now = Timestamp::from(1000);

        let [backward, forward, profile] = home_feed_filters(authors.clone(), now);

        assert_eq!(
            backward,
            Filter::new()
                .authors(authors.clone())
                .kinds(HOME_FEED_KINDS)
                .until(now)
                .limit(DEFAULT_FEED_LIMIT)
        );
        assert_eq!(
            forward,
            Filter::new()
                .authors(authors.clone())
                .kinds(HOME_FEED_KINDS)
                .since(now)
        );
        assert_eq!(
            profile,
            Filter::new().authors(authors).kinds([Kind::Metadata])
        );
    }

    #[test]
    fn test_user_feed_filters() {
        let author = pubkey(7);
        let now = Timestamp::from(2000);

        let [backward, forward] = user_feed_filters(author, now);

        assert_eq!(
            backward,
            Filter::new()
                .authors(vec![author])
                .kinds(USER_FEED_KINDS)
                .until(now)
                .limit(DEFAULT_FEED_LIMIT)
        );
        assert_eq!(
            forward,
            Filter::new()
                .authors(vec![author])
                .kinds(USER_FEED_KINDS)
                .since(now)
        );
    }

    #[test]
    fn test_home_load_more_filter() {
        let authors = vec![pubkey(1), pubkey(2)];
        let since = Timestamp::from(500);

        assert_eq!(
            home_load_more_filter(authors.clone(), since),
            Filter::new()
                .authors(authors)
                .kinds(HOME_FEED_KINDS)
                .until(since)
                .limit(DEFAULT_FEED_LIMIT)
        );
    }

    #[test]
    fn test_user_load_more_filter() {
        let author = pubkey(9);
        let since = Timestamp::from(500);

        assert_eq!(
            user_load_more_filter(author, since),
            Filter::new()
                .authors(vec![author])
                .kinds(USER_FEED_KINDS)
                .until(since)
                .limit(DEFAULT_FEED_LIMIT)
        );
    }
}
