use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;
use which::which;

pub struct AlacrittyAdapter;

impl AlacrittyAdapter {
    pub fn new() -> Self {
        Self
    }

    fn has_binary() -> bool {
        which("alacritty").is_ok()
    }
}

#[async_trait]
impl TerminalControlPort for AlacrittyAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("alacritty")
    }

    async fn detect(&self) -> CapabilityObservation {
        let binary = Self::has_binary();
        let mut notes = Vec::new();
        if !binary {
            notes.push("alacritty binary not found".to_string());
        }
        notes.push("IPC ограничен: нет split/focus".to_string());
        let capabilities = TerminalCapabilities::builder()
            .spawn(binary)
            .split(false)
            .focus(false)
            .send_text(false)
            .clipboard_write(false)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .build();
        CapabilityObservation::new("Alacritty", capabilities, false, notes)
    }

    async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "spawn",
            "alacritty msg integration pending",
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
            "alacritty IPC doesn't support send-text",
        ))
    }
}
