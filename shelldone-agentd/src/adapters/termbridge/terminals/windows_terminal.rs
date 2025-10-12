use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;
use which::which;

pub struct WindowsTerminalAdapter;

impl WindowsTerminalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn has_binary() -> bool {
        which("wt").is_ok() || which("wt.exe").is_ok()
    }
}

#[async_trait]
impl TerminalControlPort for WindowsTerminalAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("windows-terminal")
    }

    async fn detect(&self) -> CapabilityObservation {
        let binary = if cfg!(target_os = "windows") {
            Self::has_binary()
        } else {
            false
        };
        let mut notes = Vec::new();
        if !binary {
            notes.push("wt.exe not available".to_string());
        }
        let capabilities = TerminalCapabilities::builder()
            .spawn(binary)
            .split(binary)
            .focus(binary)
            .send_text(false)
            .clipboard_write(true)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .max_clipboard_kb(Some(64))
            .build();
        CapabilityObservation::new("Windows Terminal", capabilities, false, notes)
    }

    async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "spawn",
            "Windows Terminal orchestration not implemented",
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
            "Windows Terminal CLI lacks send-text APIs",
        ))
    }
}
