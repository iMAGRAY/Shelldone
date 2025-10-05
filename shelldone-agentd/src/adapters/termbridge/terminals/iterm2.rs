use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;

pub struct ITerm2Adapter;

impl ITerm2Adapter {
    pub fn new() -> Self {
        Self
    }

    fn is_macos() -> bool {
        cfg!(target_os = "macos")
    }
}

#[async_trait]
impl TerminalControlPort for ITerm2Adapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("iterm2")
    }

    async fn detect(&self) -> CapabilityObservation {
        let supported = Self::is_macos();
        let capabilities = TerminalCapabilities::builder()
            .spawn(supported)
            .split(supported)
            .focus(supported)
            .send_text(false)
            .clipboard_write(supported)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .max_clipboard_kb(Some(128))
            .build();
        let mut notes = vec![];
        if !supported {
            notes.push("iTerm2 API доступен только на macOS".to_string());
        } else {
            notes.push("API выключен по умолчанию; требует явного consent".to_string());
        }
        CapabilityObservation::new("iTerm2", capabilities, true, notes)
    }

    async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "spawn",
            "iTerm2 automation requires AppleScript integration",
        ))
    }

    async fn send_text(
        &self,
        _binding: &TerminalBinding,
        _payload: &str,
        _as_bracketed: bool,
    ) -> Result<(), TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "send_text",
            "implement via iTerm2 Python API",
        ))
    }
}
