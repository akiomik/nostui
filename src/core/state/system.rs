/// System-related state
#[derive(Debug)]
pub struct SystemState {
    status_message: Option<String>,
    is_loading: bool,
}

impl SystemState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn start_loading(&mut self) {
        self.is_loading = true;
    }

    pub fn stop_loading(&mut self) {
        self.is_loading = false;
    }

    pub fn status_message(&self) -> Option<&String> {
        self.status_message.as_ref()
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            status_message: None,
            is_loading: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemState;

    #[test]
    fn test_is_loading() {
        let mut state = SystemState::new();
        assert!(state.is_loading());

        state.stop_loading();
        assert!(!state.is_loading());

        state.start_loading();
        assert!(state.is_loading());
    }

    #[test]
    fn test_status_message() {
        let mut state = SystemState::new();
        assert_eq!(state.status_message(), None);

        state.set_status_message("Hello, world!");
        assert_eq!(state.status_message(), Some(&"Hello, world!".to_owned()));

        state.clear_status_message();
        assert_eq!(state.status_message(), None);
    }
}
