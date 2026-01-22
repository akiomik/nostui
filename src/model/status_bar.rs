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
    pub fn message(&self) -> &Option<String> {
        &self.message
    }

    fn set_message(&mut self, label: String, message: String) {
        let normalized_message = message.replace("\n", " ");
        self.message = Some(format!("[{label}] {normalized_message}"));
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
    fn test_message_getter() {
        let status_bar = StatusBar {
            message: Some("test message".to_string()),
        };
        assert_eq!(status_bar.message(), &Some("test message".to_string()));
    }

    #[test]
    fn test_message_getter_none() {
        let status_bar = StatusBar::default();
        assert_eq!(status_bar.message(), &None);
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
