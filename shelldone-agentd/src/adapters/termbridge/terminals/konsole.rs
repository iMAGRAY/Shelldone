use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;

pub struct KonsoleAdapter;

impl KonsoleAdapter {
    pub fn new() -> Self {
        Self
    }

    fn is_linux() -> bool {
        cfg!(target_os = "linux")
    }
}

#[async_trait]
impl TerminalControlPort for KonsoleAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("konsole")
    }

    async fn detect(&self) -> CapabilityObservation {
        let supported = Self::is_linux();
        let mut notes = Vec::new();
        if !supported {
            notes.push("Konsole доступна только на Linux/KDE".to_string());
        }
        notes.push("Для удаленного управления требуется D-Bus".to_string());
        let capabilities = TerminalCapabilities::builder()
            .spawn(supported)
            .split(supported)
            .focus(supported)
            .send_text(false)
            .clipboard_write(supported)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .build();
        CapabilityObservation::new("Konsole", capabilities, true, notes)
    }

    async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "spawn",
            "Konsole DBus integration not implemented",
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
            "Konsole DBus send-text not yet implemented",
        ))
    }
}
