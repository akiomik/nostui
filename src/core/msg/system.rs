use serde::{Deserialize, Serialize};

/// Messages specific to SystemState
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SystemMsg {
    // System control
    Quit,
    Suspend,
    Resume,
    Resize(u16, u16),

    // Status management
    UpdateStatusMessage(String),
    ClearStatusMessage,
    SetLoading(bool),
    ShowError(String),

    // Performance tracking
    UpdateAppFps(f64),
    UpdateRenderFps(f64),
}

impl SystemMsg {
    /// Determine if this is a frequent message during debugging
    pub fn is_frequent(&self) -> bool {
        matches!(
            self,
            SystemMsg::UpdateAppFps(_) | SystemMsg::UpdateRenderFps(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use color_eyre::Result;

    #[test]
    fn test_system_msg_frequent_detection() {
        assert!(SystemMsg::UpdateAppFps(60.0).is_frequent());
        assert!(SystemMsg::UpdateRenderFps(60.0).is_frequent());
        assert!(!SystemMsg::Quit.is_frequent());
        assert!(!SystemMsg::ShowError("test".to_string()).is_frequent());
    }

    #[test]
    fn test_system_msg_equality() {
        assert_eq!(SystemMsg::Quit, SystemMsg::Quit);
        assert_eq!(SystemMsg::Suspend, SystemMsg::Suspend);
        assert_ne!(SystemMsg::Quit, SystemMsg::Suspend);

        let error1 = SystemMsg::ShowError("test".to_string());
        let error2 = SystemMsg::ShowError("test".to_string());
        assert_eq!(error1, error2);
    }

    #[test]
    fn test_system_msg_serialization() -> Result<()> {
        let msg = SystemMsg::UpdateStatusMessage("test status".to_string());
        let serialized = serde_json::to_string(&msg)?;
        let deserialized: SystemMsg = serde_json::from_str(&serialized)?;
        assert_eq!(msg, deserialized);

        Ok(())
    }
}
