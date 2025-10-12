use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::process::Command;
use tokio::task;

use super::util::{decode_env, encode_env};

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
        notes.push("Automation использует CLI tilix --session".to_string());
        let capabilities = TerminalCapabilities::builder()
            .spawn(supported)
            .split(false)
            .focus(false)
            .duplicate(supported)
            .close(supported)
            .send_text(false)
            .clipboard_write(supported)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .build();
        CapabilityObservation::new("Tilix", capabilities, false, notes)
    }

    async fn spawn(&self, request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        if !Self::is_linux() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "spawn",
                "Tilix automation доступна только на Linux",
            ));
        }

        let mut cmd = Command::new("tilix");
        if let Some(cwd) = &request.cwd {
            cmd.current_dir(cwd);
        }
        cmd.kill_on_drop(false);

        for (key, value) in &request.env {
            cmd.env(key, value);
        }
        if let Some(command) = &request.command {
            cmd.arg("-e");
            cmd.arg("bash");
            cmd.arg("-lc");
            cmd.arg(command);
        }

        let mut child = cmd.spawn().map_err(|err| {
            TermBridgeError::internal(self.terminal_id(), format!("tilix spawn failed: {err}"))
        })?;
        let pid = child.id().ok_or_else(|| {
            TermBridgeError::internal(self.terminal_id(), "tilix spawn missing process id")
        })?;
        task::spawn(async move {
            let _ = child.wait().await;
        });

        let mut labels: HashMap<String, String> = HashMap::new();
        labels.insert("pid".to_string(), pid.to_string());
        if let Some(command) = &request.command {
            labels.insert("command".to_string(), command.clone());
        }
        if let Some(cwd) = &request.cwd {
            labels.insert("cwd".to_string(), cwd.clone());
        }
        encode_env(&mut labels, &request.env);

        let token = format!("tilix-{pid}");
        Ok(TerminalBinding::new(
            request.terminal.clone(),
            token,
            labels,
            None,
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
            "Tilix IPC не поддерживает send-text",
        ))
    }

    async fn duplicate(
        &self,
        binding: &TerminalBinding,
        options: &crate::ports::termbridge::DuplicateOptions,
    ) -> Result<TerminalBinding, TermBridgeError> {
        if !Self::is_linux() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "Tilix automation доступна только на Linux",
            ));
        }

        let mut env_map = decode_env(&binding.labels);
        for (key, value) in &options.env {
            env_map.insert(key.clone(), value.clone());
        }

        let command = options
            .command
            .clone()
            .or_else(|| binding.labels.get("command").cloned());
        let cwd = options
            .cwd
            .clone()
            .or_else(|| binding.labels.get("cwd").cloned());

        let request = SpawnRequest {
            terminal: binding.terminal.clone(),
            command,
            cwd,
            env: env_map,
        };

        self.spawn(&request).await
    }

    async fn close(&self, binding: &TerminalBinding) -> Result<(), TermBridgeError> {
        if !Self::is_linux() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "close",
                "Tilix automation доступна только на Linux",
            ));
        }

        let pid = binding
            .labels
            .get("pid")
            .ok_or_else(|| {
                TermBridgeError::not_supported(self.terminal_id(), "close", "binding missing pid")
            })?
            .parse::<u32>()
            .map_err(|_| TermBridgeError::internal(self.terminal_id(), "invalid pid value"))?;
        let status = Command::new("kill")
            .arg(format!("{pid}"))
            .status()
            .await
            .map_err(|err| {
                TermBridgeError::internal(self.terminal_id(), format!("kill {pid} failed: {err}"))
            })?;
        if !status.success() {
            return Err(TermBridgeError::internal(
                self.terminal_id(),
                format!("kill {pid} exited with {:?}", status.code()),
            ));
        }
        Ok(())
    }
}
