use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;
use std::env;
use which::which;

pub struct KittyAdapter;

impl KittyAdapter {
    pub fn new() -> Self {
        Self
    }

    fn has_binary() -> bool {
        which("kitty").is_ok()
    }

    fn listen_on() -> Option<String> {
        env::var("KITTY_LISTEN_ON").ok()
    }
}

#[async_trait]
impl TerminalControlPort for KittyAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("kitty")
    }

    async fn detect(&self) -> CapabilityObservation {
        let binary = Self::has_binary();
        let listen_on = Self::listen_on();
        let mut notes = Vec::new();
        if !binary {
            notes.push("kitty binary not found in PATH".to_string());
        }
        if listen_on.is_none() {
            notes.push("KITTY_LISTEN_ON not set; remote control disabled".to_string());
        }
        let capabilities = TerminalCapabilities::builder()
            .spawn(binary)
            .split(binary)
            .focus(binary)
            .send_text(binary && listen_on.is_some())
            .clipboard_write(true)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .max_clipboard_kb(Some(75))
            .build();
        CapabilityObservation::new("kitty", capabilities, listen_on.is_none(), notes)
    }

    async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "spawn",
            "kitty spawn orchestration not implemented yet",
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
            "kitty remote control not enabled",
        ))
    }
}
