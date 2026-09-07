#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    MessageChanged { label: String, message: String },
    ErrorMessageChanged { label: String, message: String },
    MessageCleared,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusBar {
    message: Option<String>,
}

impl StatusBar {
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    fn set_message(&mut self, label: String, message: String) {
        self.message = Some(Self::render(&label, &message));
    }

    fn render(label: &str, message: &str) -> String {
        let normalized_message = message.replace("\n", " ");
        format!("[{label}] {normalized_message}")
    }

    /// Whether the bar is still showing exactly this label and message.
    ///
    /// Asked rather than reconstructed on purpose: a caller that rebuilt the rendered
    /// string itself would keep comparing against the old shape the day this rendering
    /// changes, and would simply stop matching instead of failing.
    pub fn shows(&self, label: &str, message: &str) -> bool {
        self.message.as_deref() == Some(Self::render(label, message).as_str())
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::MessageChanged { label, message } => self.set_message(label, message),
            Message::ErrorMessageChanged { label, message } => {
                self.set_message(format!("ERR: {label}"), message)
            }
            Message::MessageCleared => {
                self.message = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_agrees_with_what_was_set() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::MessageChanged {
            label: "Sending".to_string(),
            message: "line one\nline two".to_string(),
        });

        // Asked with the original message, newlines and all. This pairing is the whole
        // point of `shows`: a caller that rebuilt the rendered string itself would have
        // to normalise identically, and would quietly stop matching if this did not.
        assert!(status_bar.shows("Sending", "line one\nline two"));
        assert!(!status_bar.shows("Sending", "something else"));
        assert!(!status_bar.shows("Posted", "line one\nline two"));
    }

    #[test]
    fn shows_does_not_match_an_error_with_the_same_text() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::ErrorMessageChanged {
            label: "Sending".to_string(),
            message: "hi".to_string(),
        });

        // Errors render as `ERR: label`, so this is a different line and must not read
        // as the pending one still standing.
        assert!(!status_bar.shows("Sending", "hi"));
    }

    #[test]
    fn test_message_getter() {
        let status_bar = StatusBar {
            message: Some("test message".to_string()),
        };
        assert_eq!(status_bar.message(), Some("test message"));
    }

    #[test]
    fn test_message_getter_none() {
        let status_bar = StatusBar::default();
        assert_eq!(status_bar.message(), None);
    }

    #[test]
    fn test_update_message_changed() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::MessageChanged {
            label: "Info".to_string(),
            message: "Connection established".to_string(),
        });
        assert_eq!(
            status_bar,
            StatusBar {
                message: Some("[Info] Connection established".to_string()),
            }
        );
    }

    #[test]
    fn test_update_error_message_changed() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::ErrorMessageChanged {
            label: "Network".to_string(),
            message: "Connection failed".to_string(),
        });
        assert_eq!(
            status_bar,
            StatusBar {
                message: Some("[ERR: Network] Connection failed".to_string()),
            }
        );
    }

    #[test]
    fn test_update_message_cleared() {
        let mut status_bar = StatusBar {
            message: Some("[Info] Test message".to_string()),
        };
        status_bar.update(Message::MessageCleared);
        assert_eq!(status_bar, StatusBar { message: None });
    }

    #[test]
    fn test_newline_normalization() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::MessageChanged {
            label: "MultiLine".to_string(),
            message: "Line 1\nLine 2\nLine 3".to_string(),
        });
        assert_eq!(
            status_bar,
            StatusBar {
                message: Some("[MultiLine] Line 1 Line 2 Line 3".to_string()),
            }
        );
    }

    #[test]
    fn test_newline_normalization_in_error_message() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::ErrorMessageChanged {
            label: "Error".to_string(),
            message: "Error occurred\nat line 42".to_string(),
        });
        assert_eq!(
            status_bar,
            StatusBar {
                message: Some("[ERR: Error] Error occurred at line 42".to_string()),
            }
        );
    }

    #[test]
    fn test_message_overwrite() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::MessageChanged {
            label: "First".to_string(),
            message: "First message".to_string(),
        });
        status_bar.update(Message::MessageChanged {
            label: "Second".to_string(),
            message: "Second message".to_string(),
        });
        assert_eq!(
            status_bar,
            StatusBar {
                message: Some("[Second] Second message".to_string()),
            }
        );
    }

    #[test]
    fn test_empty_label_and_message() {
        let mut status_bar = StatusBar::default();
        status_bar.update(Message::MessageChanged {
            label: "".to_string(),
            message: "".to_string(),
        });
        assert_eq!(
            status_bar,
            StatusBar {
                message: Some("[] ".to_string()),
            }
        );
    }
}
