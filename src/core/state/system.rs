/// System-related state
#[derive(Debug)]
pub struct SystemState {
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
}

impl Default for SystemState {
    fn default() -> Self {
        Self { is_loading: true }
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
}
