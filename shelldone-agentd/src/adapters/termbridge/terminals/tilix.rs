use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;

pub struct TilixAdapter;

impl TilixAdapter {
    pub fn new() -> Self {
        Self
    }

    fn is_linux() -> bool {
        cfg!(target_os = "linux")
    }
}

#[async_trait]
impl TerminalControlPort for TilixAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("tilix")
    }

    async fn detect(&self) -> CapabilityObservation {
        let supported = Self::is_linux();
        let mut notes = Vec::new();
        if !supported {
            notes.push("Tilix доступен только на Linux".to_string());
        }
        notes.push("IPC через --session JSON (ограниченный функционал)".to_string());
        let capabilities = TerminalCapabilities::builder()
            .spawn(supported)
            .split(false)
            .focus(false)
            .send_text(false)
            .clipboard_write(supported)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .build();
        CapabilityObservation::new("Tilix", capabilities, false, notes)
    }

    async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "spawn",
            "Tilix session automation pending",
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
            "Tilix IPC does not support send-text",
        ))
    }
}
