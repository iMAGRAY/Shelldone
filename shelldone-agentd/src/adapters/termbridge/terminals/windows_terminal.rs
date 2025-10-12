use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;
#[cfg(target_os = "windows")]
use std::collections::HashMap;

#[cfg(target_os = "windows")]
use super::util::{decode_env, encode_env};

#[cfg(target_os = "windows")]
use tokio::{process::Command, task};

#[cfg(target_os = "windows")]
use which::which;

pub struct WindowsTerminalAdapter;

impl WindowsTerminalAdapter {
    pub fn new() -> Self {
        Self
    }

    #[cfg(target_os = "windows")]
    fn has_binary() -> bool {
        which("wt.exe").is_ok() || which("wt").is_ok()
    }

    #[cfg(not(target_os = "windows"))]
    fn has_binary() -> bool {
        false
    }

    #[cfg(target_os = "windows")]
    fn binary_name() -> &'static str {
        if which("wt.exe").is_ok() {
            "wt.exe"
        } else {
            "wt"
        }
    }
}

#[async_trait]
impl TerminalControlPort for WindowsTerminalAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("windows-terminal")
    }

    async fn detect(&self) -> CapabilityObservation {
        let binary = Self::has_binary();
        let mut notes = Vec::new();
        if !binary {
            notes.push("wt.exe not available".to_string());
        }
        let capabilities = TerminalCapabilities::builder()
            .spawn(binary)
            .split(binary)
            .focus(binary)
            .duplicate(binary)
            .close(binary)
            .send_text(false)
            .clipboard_write(true)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .max_clipboard_kb(Some(64))
            .build();
        CapabilityObservation::new("Windows Terminal", capabilities, false, notes)
    }

    async fn spawn(&self, request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        #[cfg(target_os = "windows")]
        {
            if !Self::has_binary() {
                return Err(TermBridgeError::not_supported(
                    self.terminal_id(),
                    "spawn",
                    "Windows Terminal CLI not available",
                ));
            }

            let mut cmd = Command::new(Self::binary_name());
            cmd.kill_on_drop(false);
            cmd.arg("new-tab");

            if let Some(cwd) = &request.cwd {
                cmd.arg("-d");
                cmd.arg(cwd);
            }

            for (key, value) in &request.env {
                cmd.env(key, value);
            }

            if let Some(command) = &request.command {
                cmd.arg("cmd");
                cmd.arg("/c");
                cmd.arg(command);
            }

            let mut child = cmd.spawn().map_err(|err| {
                TermBridgeError::internal(self.terminal_id(), format!("wt spawn failed: {err}"))
            })?;
            child.kill_on_drop(false);
            let pid = child.id().ok_or_else(|| {
                TermBridgeError::internal(self.terminal_id(), "wt spawn missing process id")
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

            let token = format!("wt-{pid}");
            return Ok(TerminalBinding::new(
                request.terminal.clone(),
                token,
                labels,
                None,
            ));
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = request;
            Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "spawn",
                "Windows Terminal automation available only on Windows",
            ))
        }
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

    async fn duplicate(
        &self,
        binding: &TerminalBinding,
        options: &crate::ports::termbridge::DuplicateOptions,
    ) -> Result<TerminalBinding, TermBridgeError> {
        #[cfg(target_os = "windows")]
        {
            if !Self::has_binary() {
                return Err(TermBridgeError::not_supported(
                    self.terminal_id(),
                    "duplicate",
                    "Windows Terminal CLI not available",
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

            return self.spawn(&request).await;
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (binding, options);
            Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "Windows Terminal automation available only on Windows",
            ))
        }
    }

    async fn close(&self, binding: &TerminalBinding) -> Result<(), TermBridgeError> {
        #[cfg(target_os = "windows")]
        {
            if !Self::has_binary() {
                return Err(TermBridgeError::not_supported(
                    self.terminal_id(),
                    "close",
                    "Windows Terminal CLI not available",
                ));
            }

            let pid = binding
                .labels
                .get("pid")
                .ok_or_else(|| {
                    TermBridgeError::not_supported(
                        self.terminal_id(),
                        "close",
                        "binding missing pid",
                    )
                })?
                .clone();

            let status = Command::new("taskkill")
                .arg("/PID")
                .arg(&pid)
                .arg("/T")
                .arg("/F")
                .status()
                .await
                .map_err(|err| {
                    TermBridgeError::internal(
                        self.terminal_id(),
                        format!("taskkill {pid} failed: {err}"),
                    )
                })?;
            if !status.success() {
                return Err(TermBridgeError::internal(
                    self.terminal_id(),
                    format!("taskkill {pid} exited with {:?}", status.code()),
                ));
            }
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = binding;
            Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "close",
                "Windows Terminal automation available only on Windows",
            ))
        }
    }
}
