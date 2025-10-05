use crate::domain::termbridge::{
    TerminalBinding, TerminalBindingId, TerminalCapabilities, TerminalId,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapabilityObservation {
    pub display_name: String,
    pub capabilities: TerminalCapabilities,
    pub requires_opt_in: bool,
    pub notes: Vec<String>,
}

impl CapabilityObservation {
    pub fn new(
        display_name: impl Into<String>,
        capabilities: TerminalCapabilities,
        requires_opt_in: bool,
        notes: Vec<String>,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            capabilities,
            requires_opt_in,
            notes,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub terminal: TerminalId,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TermBridgeCommandRequest {
    pub binding_id: Option<TerminalBindingId>,
    pub terminal: Option<TerminalId>,
    pub payload: Option<String>,
    pub bracketed_paste: Option<bool>,
}

#[async_trait]
pub trait TerminalControlPort: Send + Sync {
    fn terminal_id(&self) -> TerminalId;

    async fn detect(&self) -> CapabilityObservation;

    async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "spawn",
            "not implemented",
        ))
    }

    async fn focus(&self, _binding: &TerminalBinding) -> Result<(), TermBridgeError> {
        Err(TermBridgeError::not_supported(
            self.terminal_id(),
            "focus",
            "not implemented",
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
            "not implemented",
        ))
    }
}

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum TermBridgeError {
    #[error("terminal {terminal} does not support {action}: {reason}")]
    NotSupported {
        terminal: TerminalId,
        action: String,
        reason: String,
    },
    #[error("terminal {terminal} binding not found")]
    BindingNotFound { terminal: TerminalId },
    #[error("internal error: {0}")]
    Internal(String),
}

impl TermBridgeError {
    pub fn not_supported(
        terminal: TerminalId,
        action: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::NotSupported {
            terminal,
            action: action.into(),
            reason: reason.into(),
        }
    }

    pub fn internal(terminal: TerminalId, message: impl Into<String>) -> Self {
        Self::Internal(format!("{}: {}", terminal, message.into()))
    }
}
