use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use std::{env, io};
use tokio::process::Command;
use tokio::time::timeout;
use which::which;

const WEZTERM_TIMEOUT: Duration = Duration::from_secs(3);
const WEZTERM_CLI_OVERRIDE_ENV: &str = "SHELLDONE_TERMBRIDGE_WEZTERM_CLI";

#[derive(Debug)]
enum WezTermCliError {
    MissingCli,
    Timeout { command: String },
    Io { command: String, source: io::Error },
}

#[derive(Debug, Clone)]
struct WezTermCliOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl WezTermCliOutput {
    #[cfg(test)]
    fn success() -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    #[cfg(test)]
    fn failure(exit_code: Option<i32>, stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            success: false,
            exit_code,
            stdout: Vec::new(),
            stderr: stderr.into(),
        }
    }
}

#[async_trait]
trait WezTermCliRunner: Send + Sync {
    async fn run(&self, args: &[String]) -> Result<WezTermCliOutput, WezTermCliError>;
}

#[derive(Default)]
struct SystemWezTermCliRunner;

impl SystemWezTermCliRunner {
    fn build_command(&self, args: &[String]) -> Result<Command, WezTermCliError> {
        let cli_path = WezTermAdapter::resolve_cli_path().ok_or(WezTermCliError::MissingCli)?;
        let mut cmd = Command::new(cli_path);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        Ok(cmd)
    }

    fn command_label(args: &[String]) -> String {
        let mut label = String::from("wezterm");
        if !args.is_empty() {
            label.push(' ');
            label.push_str(&args.join(" "));
        }
        label
    }
}

#[async_trait]
impl WezTermCliRunner for SystemWezTermCliRunner {
    async fn run(&self, args: &[String]) -> Result<WezTermCliOutput, WezTermCliError> {
        let command_label = Self::command_label(args);
        let mut cmd = self.build_command(args)?;
        let output = timeout(WEZTERM_TIMEOUT, cmd.output())
            .await
            .map_err(|_| WezTermCliError::Timeout {
                command: command_label.clone(),
            })?
            .map_err(|err| WezTermCliError::Io {
                command: command_label.clone(),
                source: err,
            })?;
        Ok(WezTermCliOutput {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

pub struct WezTermAdapter {
    runner: Arc<dyn WezTermCliRunner>,
}

impl WezTermAdapter {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(SystemWezTermCliRunner),
        }
    }

    #[cfg(test)]
    fn with_runner(runner: Arc<dyn WezTermCliRunner>) -> Self {
        Self { runner }
    }

    fn resolve_cli_path() -> Option<PathBuf> {
        if let Ok(raw) = env::var(WEZTERM_CLI_OVERRIDE_ENV) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                let candidate = PathBuf::from(trimmed);
                if candidate.exists() {
                    return Some(candidate);
                }
                return None;
            }
        }
        which("wezterm").ok()
    }

    fn map_cli_error(&self, action: &str, err: WezTermCliError) -> TermBridgeError {
        match err {
            WezTermCliError::MissingCli => TermBridgeError::not_supported(
                self.terminal_id(),
                action,
                format!(
                    "wezterm CLI not found (configure {} or install wezterm)",
                    WEZTERM_CLI_OVERRIDE_ENV
                ),
            ),
            WezTermCliError::Timeout { command } => {
                TermBridgeError::internal(self.terminal_id(), format!("{command} timed out"))
            }
            WezTermCliError::Io { command, source } => {
                TermBridgeError::internal(self.terminal_id(), format!("{command}: {source}"))
            }
        }
    }

    async fn execute_cli(
        &self,
        args: Vec<String>,
        action: &'static str,
    ) -> Result<WezTermCliOutput, TermBridgeError> {
        self.runner
            .run(&args)
            .await
            .map_err(|err| self.map_cli_error(action, err))
    }

    fn extract_pane_id(binding: &TerminalBinding) -> Option<String> {
        binding.labels.get("pane_id").cloned().or_else(|| {
            binding.ipc_endpoint.as_ref().and_then(|endpoint| {
                endpoint
                    .strip_prefix("wezterm://pane/")
                    .map(|s| s.to_string())
            })
        })
    }

    fn stderr_message(action: &str, output: &WezTermCliOutput) -> String {
        let mut base = format!("wezterm {action} failed");
        if let Some(code) = output.exit_code {
            base.push_str(&format!(" (exit code {code})"));
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            base.push_str(": ");
            base.push_str(stderr.trim());
        }
        base
    }
}

#[derive(Debug, Deserialize, Default)]
struct WezTermSpawnOutput {
    #[serde(default)]
    pane_id: Option<u64>,
    #[serde(default)]
    window_id: Option<u64>,
    #[serde(default)]
    tab_id: Option<u64>,
}

#[async_trait]
impl TerminalControlPort for WezTermAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("wezterm")
    }

    async fn detect(&self) -> CapabilityObservation {
        let cli_path = Self::resolve_cli_path();
        let mut notes = Vec::new();
        if let Ok(raw) = env::var(WEZTERM_CLI_OVERRIDE_ENV) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push(
                    "SHELLDONE_TERMBRIDGE_WEZTERM_CLI is empty; override ignored".to_string(),
                );
            } else if let Some(path) = &cli_path {
                notes.push(format!("using wezterm CLI override: {}", path.display()));
            } else {
                notes.push(format!("wezterm CLI override path not found: {}", trimmed));
            }
        }
        if cli_path.is_none() {
            notes.push("wezterm CLI not found".to_string());
        }
        let binary_available = cli_path.is_some();
        let capabilities = TerminalCapabilities::builder()
            .spawn(binary_available)
            .split(binary_available)
            .focus(binary_available)
            .send_text(binary_available)
            .clipboard_write(true)
            .clipboard_read(true)
            .cwd_sync(true)
            .bracketed_paste(true)
            .max_clipboard_kb(Some(256))
            .build();
        CapabilityObservation::new("WezTerm", capabilities, false, notes)
    }

    async fn spawn(&self, request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        let mut args = vec![
            "cli".to_string(),
            "spawn".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        if let Some(cwd) = &request.cwd {
            args.push("--cwd".to_string());
            args.push(cwd.clone());
        }
        if let Some(command) = &request.command {
            args.push("--".to_string());
            args.push(command.clone());
        }

        let output = self.execute_cli(args, "spawn").await?;
        if !output.success {
            return Err(TermBridgeError::internal(
                self.terminal_id(),
                Self::stderr_message("spawn", &output),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let parsed: Option<WezTermSpawnOutput> = serde_json::from_str(&stdout).ok();

        let mut labels = HashMap::new();
        if let Some(parsed) = parsed {
            if let Some(pane) = parsed.pane_id {
                labels.insert("pane_id".to_string(), pane.to_string());
            }
            if let Some(window) = parsed.window_id {
                labels.insert("window_id".to_string(), window.to_string());
            }
            if let Some(tab) = parsed.tab_id {
                labels.insert("tab_id".to_string(), tab.to_string());
            }
        }

        if let Some(command) = &request.command {
            labels.insert("command".to_string(), command.clone());
        }

        let pane_id = labels.get("pane_id").cloned();
        let ipc_endpoint = pane_id
            .as_ref()
            .map(|pane| format!("wezterm://pane/{pane}"));
        let token = pane_id.unwrap_or_else(|| stdout.clone());

        let binding = TerminalBinding::new(request.terminal.clone(), token, labels, ipc_endpoint);
        Ok(binding)
    }

    async fn send_text(
        &self,
        binding: &TerminalBinding,
        payload: &str,
        as_bracketed: bool,
    ) -> Result<(), TermBridgeError> {
        let pane_id = Self::extract_pane_id(binding).ok_or_else(|| {
            TermBridgeError::internal(self.terminal_id(), "binding missing pane_id for send_text")
        })?;

        let mut args = vec![
            "cli".to_string(),
            "send-text".to_string(),
            "--pane-id".to_string(),
            pane_id,
        ];
        if !as_bracketed {
            args.push("--no-paste".to_string());
        }
        args.push("--text".to_string());
        args.push(payload.to_string());

        let output = self.execute_cli(args, "send_text").await?;
        if output.success {
            Ok(())
        } else {
            Err(TermBridgeError::internal(
                self.terminal_id(),
                Self::stderr_message("send-text", &output),
            ))
        }
    }

    async fn focus(&self, binding: &TerminalBinding) -> Result<(), TermBridgeError> {
        let pane_id = Self::extract_pane_id(binding).ok_or_else(|| {
            TermBridgeError::internal(self.terminal_id(), "binding missing pane_id for focus")
        })?;
        let args = vec![
            "cli".to_string(),
            "activate-pane".to_string(),
            "--pane-id".to_string(),
            pane_id,
        ];
        let output = self.execute_cli(args, "focus").await?;
        if output.success {
            Ok(())
        } else {
            Err(TermBridgeError::internal(
                self.terminal_id(),
                Self::stderr_message("activate-pane", &output),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    struct RecordingRunner {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
        responses: Mutex<VecDeque<Result<WezTermCliOutput, WezTermCliError>>>,
    }

    impl RecordingRunner {
        fn new(responses: Vec<Result<WezTermCliOutput, WezTermCliError>>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                responses: Mutex::new(responses.into_iter().collect()),
            }
        }

        fn with_calls(
            responses: Vec<Result<WezTermCliOutput, WezTermCliError>>,
        ) -> (Arc<Self>, Arc<Mutex<Vec<Vec<String>>>>) {
            let runner = Arc::new(Self::new(responses));
            (runner.clone(), runner.calls.clone())
        }
    }

    #[async_trait]
    impl WezTermCliRunner for RecordingRunner {
        async fn run(&self, args: &[String]) -> Result<WezTermCliOutput, WezTermCliError> {
            self.calls.lock().unwrap().push(args.to_vec());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(WezTermCliOutput::success()))
        }
    }

    #[test]
    fn resolve_cli_prefers_override_when_exists() {
        let tempdir = tempdir().unwrap();
        let cli_path = tempdir.path().join("fake-wezterm"); // qa:allow-realness
        fs::write(&cli_path, "#!/bin/sh\nexit 0\n").unwrap();
        let original = env::var(WEZTERM_CLI_OVERRIDE_ENV).ok();
        env::set_var(
            WEZTERM_CLI_OVERRIDE_ENV,
            cli_path.to_string_lossy().to_string(),
        );

        let resolved = WezTermAdapter::resolve_cli_path();
        assert_eq!(resolved.as_deref(), Some(cli_path.as_path()));

        if let Some(value) = original {
            env::set_var(WEZTERM_CLI_OVERRIDE_ENV, value);
        } else {
            env::remove_var(WEZTERM_CLI_OVERRIDE_ENV);
        }
    }

    #[test]
    fn resolve_cli_falls_back_to_none_when_override_missing() {
        let original = env::var(WEZTERM_CLI_OVERRIDE_ENV).ok();
        env::set_var(WEZTERM_CLI_OVERRIDE_ENV, "/no/such/wezterm");

        assert!(WezTermAdapter::resolve_cli_path().is_none());

        if let Some(value) = original {
            env::set_var(WEZTERM_CLI_OVERRIDE_ENV, value);
        } else {
            env::remove_var(WEZTERM_CLI_OVERRIDE_ENV);
        }
    }

    #[tokio::test]
    async fn focus_invokes_activate_pane() {
        let (runner, calls) = RecordingRunner::with_calls(vec![Ok(WezTermCliOutput::success())]);
        let adapter = WezTermAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("pane_id".to_string(), "pane-7".to_string());
        let binding = TerminalBinding::new(TerminalId::new("wezterm"), "pane-7", labels, None);

        adapter.focus(&binding).await.unwrap();

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0],
            vec![
                "cli".to_string(),
                "activate-pane".to_string(),
                "--pane-id".to_string(),
                "pane-7".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn focus_propagates_failure() {
        let (runner, _) =
            RecordingRunner::with_calls(vec![Ok(WezTermCliOutput::failure(Some(1), "boom"))]);
        let adapter = WezTermAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("pane_id".to_string(), "pane-9".to_string());
        let binding = TerminalBinding::new(TerminalId::new("wezterm"), "pane-9", labels, None);

        let err = adapter
            .focus(&binding)
            .await
            .expect_err("expected focus error");
        assert!(matches!(err, TermBridgeError::Internal(_)));
    }

    #[tokio::test]
    async fn send_text_includes_no_paste_flag_when_disabled() {
        let (runner, calls) = RecordingRunner::with_calls(vec![Ok(WezTermCliOutput::success())]);
        let adapter = WezTermAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("pane_id".to_string(), "pane-3".to_string());
        let binding = TerminalBinding::new(TerminalId::new("wezterm"), "pane-3", labels, None);

        adapter.send_text(&binding, "echo hi", false).await.unwrap();

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert!(recorded[0].contains(&"--no-paste".to_string()));
    }

    #[tokio::test]
    async fn send_text_without_override_delegates_to_runner() {
        let (runner, calls) = RecordingRunner::with_calls(vec![Ok(WezTermCliOutput::success())]);
        let adapter = WezTermAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("pane_id".to_string(), "pane-11".to_string());
        let binding = TerminalBinding::new(TerminalId::new("wezterm"), "pane-11", labels, None);

        adapter.send_text(&binding, "ls", true).await.unwrap();

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert!(!recorded[0].contains(&"--no-paste".to_string()));
    }

    #[tokio::test]
    async fn send_text_propagates_missing_cli_as_not_supported() {
        let runner = Arc::new(RecordingRunner::new(vec![Err(WezTermCliError::MissingCli)]));
        let adapter = WezTermAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("pane_id".to_string(), "pane-15".to_string());
        let binding = TerminalBinding::new(TerminalId::new("wezterm"), "pane-15", labels, None);

        let err = adapter
            .send_text(&binding, "ls", true)
            .await
            .expect_err("expected missing CLI error");
        assert!(matches!(err, TermBridgeError::NotSupported { .. }));
    }
}
