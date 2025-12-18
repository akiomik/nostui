use crate::core::raw_msg::RawMsg;
use crate::integration::elm_integration::ElmRuntime;

/// Executes Elm update cycle with error handling, and applies pending coalesced resize.
pub struct UpdateExecutor;

impl UpdateExecutor {
    pub fn process_update_cycle(runtime: &mut ElmRuntime, pending_resize: &mut Option<(u16, u16)>) {
        if let Some((w, h)) = pending_resize.take() {
            runtime.send_raw_msg(RawMsg::Resize(w, h));
        }
        if let Err(e) = runtime.run_update_cycle() {
            log::error!("ElmRuntime error: {}", e);
            runtime.send_raw_msg(RawMsg::Error(format!("ElmRuntime error: {}", e)));
        }
    }
}
