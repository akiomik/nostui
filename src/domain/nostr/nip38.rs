use nostr_sdk::prelude::*;
use nowhear::Track;

#[derive(Debug, PartialEq)]
pub struct MusicStatus {
    track: Track,
}

impl MusicStatus {
    pub fn new(track: Track) -> Option<Self> {
        if track.title.is_empty() || track.artist.is_empty() || track.duration.is_none() {
            return None;
        }

        Some(Self { track })
    }

    pub fn content(&self) -> String {
        format!("{} - {}", self.track.title, self.track.artist.join(", "))
    }

    pub fn reference(&self) -> String {
        percent_encoding::utf8_percent_encode(&self.content(), percent_encoding::NON_ALPHANUMERIC)
            .to_string()
    }

    pub fn expiration(&self) -> Option<Timestamp> {
        self.track
            .duration
            .map(|duration| Timestamp::now() + duration)
    }
}

impl From<MusicStatus> for LiveStatus {
    fn from(value: MusicStatus) -> Self {
        LiveStatus {
            status_type: StatusType::Music,
            expiration: value.expiration(),
            reference: Some(value.reference()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_valid_track() -> Track {
        Track {
            title: "Test Song".to_string(),
            artist: vec!["Test Artist".to_string()],
            duration: Some(Duration::from_secs(180)),
            album: None,
            album_artist: None,
            track_number: None,
            art_url: None,
        }
    }

    #[test]
    fn test_new_with_valid_track() {
        let track = create_valid_track();
        let status = MusicStatus::new(track);

        assert!(status.is_some());
    }

    #[test]
    fn test_new_with_empty_title() {
        let track = Track {
            title: "".to_string(),
            artist: vec!["Test Artist".to_string()],
            duration: Some(Duration::from_secs(180)),
            album: None,
            album_artist: None,
            track_number: None,
            art_url: None,
        };
        let status = MusicStatus::new(track);

        assert_eq!(status, None);
    }

    #[test]
    fn test_new_with_empty_artist() {
        let track = Track {
            title: "Test Song".to_string(),
            artist: vec![],
            duration: Some(Duration::from_secs(180)),
            album: None,
            album_artist: None,
            track_number: None,
            art_url: None,
        };
        let status = MusicStatus::new(track);

        assert_eq!(status, None);
    }

    #[test]
    fn test_new_with_none_duration() {
        let track = Track {
            title: "Test Song".to_string(),
            artist: vec!["Test Artist".to_string()],
            duration: None,
            album: None,
            album_artist: None,
            track_number: None,
            art_url: None,
        };
        let status = MusicStatus::new(track);

        assert_eq!(status, None);
    }

    #[test]
    fn test_content_single_artist() {
        let track = create_valid_track();
        let status = MusicStatus::new(track).expect("Failed to create MusicStatus");

        assert_eq!(status.content(), "Test Song - Test Artist");
    }

    #[test]
    fn test_content_multiple_artists() {
        let track = Track {
            title: "Collaboration".to_string(),
            artist: vec![
                "Artist One".to_string(),
                "Artist Two".to_string(),
                "Artist Three".to_string(),
            ],
            duration: Some(Duration::from_secs(200)),
            album: None,
            album_artist: None,
            track_number: None,
            art_url: None,
        };
        let status = MusicStatus::new(track).expect("Failed to create MusicStatus");

        assert_eq!(
            status.content(),
            "Collaboration - Artist One, Artist Two, Artist Three"
        );
    }

    #[test]
    fn test_reference_encodes_special_characters() {
        let track = Track {
            title: "Song & Title".to_string(),
            artist: vec!["Artist/Name".to_string()],
            duration: Some(Duration::from_secs(180)),
            album: None,
            album_artist: None,
            track_number: None,
            art_url: None,
        };
        let status = MusicStatus::new(track).expect("Failed to create MusicStatus");
        let reference = status.reference();

        // Should encode special characters
        assert!(reference.contains("%26")); // '&' encoded
        assert!(reference.contains("%2F")); // '/' encoded
        assert!(!reference.contains("&"));
        assert!(!reference.contains("/"));
    }

    #[test]
    fn test_reference_encodes_spaces() {
        let track = create_valid_track();
        let status = MusicStatus::new(track).expect("Failed to create MusicStatus");
        let reference = status.reference();

        // Spaces should be encoded
        assert!(reference.contains("%20"));
        assert!(!reference.contains(" "));
    }

    #[test]
    fn test_expiration_returns_some() {
        let track = create_valid_track();
        let status = MusicStatus::new(track).expect("Failed to create MusicStatus");
        let expiration = status.expiration();

        assert!(expiration.is_some());
    }

    #[test]
    fn test_expiration_is_in_future() {
        let track = create_valid_track();
        let status = MusicStatus::new(track).expect("Failed to create MusicStatus");
        let expiration = status.expiration().expect("Expiration should be Some");

        // Expiration should be in the future
        assert!(expiration > Timestamp::now());
    }

    #[test]
    fn test_from_music_status_to_live_status() {
        let track = create_valid_track();
        let music_status = MusicStatus::new(track).expect("Failed to create MusicStatus");
        let live_status: LiveStatus = music_status.into();

        assert_eq!(live_status.status_type, StatusType::Music);
        assert!(live_status.expiration.is_some());
        assert!(live_status.reference.is_some());
    }

    #[test]
    fn test_from_music_status_preserves_reference() {
        let track = Track {
            title: "Test & Song".to_string(),
            artist: vec!["Test Artist".to_string()],
            duration: Some(Duration::from_secs(180)),
            album: None,
            album_artist: None,
            track_number: None,
            art_url: None,
        };
        let music_status = MusicStatus::new(track).expect("Failed to create MusicStatus");
        let expected_reference = music_status.reference();
        let live_status: LiveStatus = music_status.into();

        assert_eq!(live_status.reference, Some(expected_reference));
    }
}
