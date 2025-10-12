use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, DuplicateOptions, SpawnRequest, TermBridgeError, TerminalControlPort,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::task;
use tokio::time::timeout;
use which::which;

use super::util::encode_env;

const KONSOLE_DBUS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
struct KonsoleDbusOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone)]
enum KonsoleDbusError {
    MissingCli,
    Timeout { command: String },
    Io { command: String, message: String },
}

#[async_trait]
trait KonsoleDbusRunner: Send + Sync {
    async fn run(&self, args: &[String]) -> Result<KonsoleDbusOutput, KonsoleDbusError>;
}

#[derive(Clone, Default)]
struct SystemKonsoleDbusRunner;

impl SystemKonsoleDbusRunner {
    fn resolve_cli_path() -> Option<PathBuf> {
        which("qdbus6").ok().or_else(|| which("qdbus").ok())
    }

    fn command_label(args: &[String]) -> String {
        let mut label = String::from("qdbus");
        if !args.is_empty() {
            label.push(' ');
            label.push_str(&args.join(" "));
        }
        label
    }

    fn build_command(&self, args: &[String]) -> Result<Command, KonsoleDbusError> {
        let path = Self::resolve_cli_path().ok_or(KonsoleDbusError::MissingCli)?;
        let mut cmd = Command::new(path);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        Ok(cmd)
    }
}

#[async_trait]
impl KonsoleDbusRunner for SystemKonsoleDbusRunner {
    async fn run(&self, args: &[String]) -> Result<KonsoleDbusOutput, KonsoleDbusError> {
        let command_label = Self::command_label(args);
        let mut cmd = self.build_command(args)?;
        let output = timeout(KONSOLE_DBUS_TIMEOUT, cmd.output())
            .await
            .map_err(|_| KonsoleDbusError::Timeout {
                command: command_label.clone(),
            })?;
        let output = output.map_err(|err| KonsoleDbusError::Io {
            command: command_label.clone(),
            message: err.to_string(),
        })?;
        Ok(KonsoleDbusOutput {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

pub struct KonsoleAdapter {
    runner: Arc<dyn KonsoleDbusRunner>,
}

impl KonsoleAdapter {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(SystemKonsoleDbusRunner),
        }
    }

    #[cfg(test)]
    fn with_runner(runner: Arc<dyn KonsoleDbusRunner>) -> Self {
        Self { runner }
    }

    fn is_linux() -> bool {
        if cfg!(test) && env::var("SHELLDONE_TEST_FORCE_KONSOLE").ok().as_deref() == Some("1") {
            return true;
        }
        cfg!(target_os = "linux")
    }

    fn remote_supported() -> bool {
        if !Self::is_linux() {
            return false;
        }
        if cfg!(test)
            && env::var("SHELLDONE_TEST_FORCE_KONSOLE_DBUS")
                .ok()
                .as_deref()
                == Some("1")
        {
            return true;
        }
        SystemKonsoleDbusRunner::resolve_cli_path().is_some()
    }

    fn map_error(&self, action: &str, err: KonsoleDbusError) -> TermBridgeError {
        match err {
            KonsoleDbusError::MissingCli => TermBridgeError::not_supported(
                self.terminal_id(),
                action,
                "Konsole DBus CLI (qdbus) not available",
            ),
            KonsoleDbusError::Timeout { command } => {
                TermBridgeError::internal(self.terminal_id(), format!("{command} timed out"))
            }
            KonsoleDbusError::Io { command, message } => {
                TermBridgeError::internal(self.terminal_id(), format!("{command}: {message}"))
            }
        }
    }

    async fn run_dbus(
        &self,
        args: Vec<String>,
        action: &str,
    ) -> Result<KonsoleDbusOutput, TermBridgeError> {
        self.runner
            .run(&args)
            .await
            .map_err(|err| self.map_error(action, err))
    }

    async fn session_id_for_pid(&self, pid: u32) -> Result<Option<String>, TermBridgeError> {
        let output = self
            .run_dbus(
                vec![
                    "org.kde.konsole".into(),
                    "/Sessions".into(),
                    "org.kde.konsole.SessionManager.sessionIdForPID".into(),
                    pid.to_string(),
                ],
                "sessionIdForPID",
            )
            .await?;
        if !output.success {
            return Err(TermBridgeError::internal(
                self.terminal_id(),
                format!(
                    "sessionIdForPID failed (code {:?}): {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(id))
        }
    }

    async fn clone_session(&self, session_id: &str) -> Result<String, TermBridgeError> {
        let output = self
            .run_dbus(
                vec![
                    "org.kde.konsole".into(),
                    "/Sessions".into(),
                    "org.kde.konsole.SessionManager.cloneSession".into(),
                    session_id.into(),
                ],
                "cloneSession",
            )
            .await?;
        if !output.success {
            return Err(TermBridgeError::internal(
                self.terminal_id(),
                format!(
                    "cloneSession failed (code {:?}): {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if id.is_empty() {
            Err(TermBridgeError::internal(
                self.terminal_id(),
                "cloneSession returned empty response",
            ))
        } else {
            Ok(id)
        }
    }

    async fn session_pid(&self, session_id: &str) -> Result<Option<u32>, TermBridgeError> {
        let output = self
            .run_dbus(
                vec![
                    "org.kde.konsole".into(),
                    format!("/Sessions/{session_id}"),
                    "org.kde.konsole.Session.pid".into(),
                ],
                "sessionPid",
            )
            .await?;
        if !output.success {
            return Err(TermBridgeError::internal(
                self.terminal_id(),
                format!(
                    "sessionPid failed (code {:?}): {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if raw.is_empty() {
            Ok(None)
        } else {
            raw.parse::<u32>()
                .map(Some)
                .map_err(|_| TermBridgeError::internal(self.terminal_id(), "invalid session pid"))
        }
    }

    async fn close_session(&self, session_id: &str) -> Result<(), TermBridgeError> {
        let output = self
            .run_dbus(
                vec![
                    "org.kde.konsole".into(),
                    format!("/Sessions/{session_id}"),
                    "org.kde.konsole.Session.close".into(),
                ],
                "closeSession",
            )
            .await?;
        if output.success {
            Ok(())
        } else {
            Err(TermBridgeError::internal(
                self.terminal_id(),
                format!(
                    "closeSession failed (code {:?}): {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ))
        }
    }

    async fn attach_session_metadata(
        &self,
        labels: &mut HashMap<String, String>,
        pid: u32,
    ) -> Result<(), TermBridgeError> {
        if !Self::remote_supported() {
            return Ok(());
        }
        if let Some(session_id) = self.session_id_for_pid(pid).await? {
            labels.insert("konsole_session_id".to_string(), session_id);
        }
        Ok(())
    }
}

#[async_trait]
impl TerminalControlPort for KonsoleAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("konsole")
    }

    async fn detect(&self) -> CapabilityObservation {
        let supported = Self::is_linux();
        let remote = Self::remote_supported();
        let mut notes = Vec::new();
        if !supported {
            notes.push("Konsole доступна только на Linux/KDE".to_string());
        }
        if !remote {
            notes.push(
                "Для операций duplicate/close требуется qdbus (konsole remote control)".to_string(),
            );
        } else {
            notes.push("DBus управление активировано (qdbus)".to_string());
        }
        let capabilities = TerminalCapabilities::builder()
            .spawn(supported)
            .split(supported)
            .focus(supported)
            .duplicate(remote)
            .close(remote)
            .send_text(false)
            .clipboard_write(supported)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .build();
        CapabilityObservation::new("Konsole", capabilities, !remote, notes)
    }

    async fn spawn(&self, request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        if !Self::is_linux() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "spawn",
                "Konsole automation доступна только на Linux",
            ));
        }

        let mut cmd = Command::new("konsole");
        if let Some(cwd) = &request.cwd {
            cmd.current_dir(cwd);
        }
        cmd.kill_on_drop(false);

        for (key, value) in &request.env {
            cmd.env(key, value);
        }
        if let Some(command) = &request.command {
            cmd.arg("--hold");
            cmd.arg("-e");
            cmd.arg("sh");
            cmd.arg("-lc");
            cmd.arg(command);
        }

        let mut child = cmd.spawn().map_err(|err| {
            TermBridgeError::internal(self.terminal_id(), format!("konsole spawn failed: {err}"))
        })?;
        let pid = child.id().ok_or_else(|| {
            TermBridgeError::internal(self.terminal_id(), "konsole spawn missing process id")
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
        self.attach_session_metadata(&mut labels, pid).await?;

        let token = if let Some(session_id) = labels.get("konsole_session_id") {
            format!("konsole-session-{session_id}")
        } else {
            format!("konsole-{pid}")
        };

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
            "Konsole DBus send-text не реализован",
        ))
    }

    async fn duplicate(
        &self,
        binding: &TerminalBinding,
        options: &DuplicateOptions,
    ) -> Result<TerminalBinding, TermBridgeError> {
        if !Self::remote_supported() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "Konsole DBus automation недоступно",
            ));
        }

        if options.command.is_some() || options.cwd.is_some() || !options.env.is_empty() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "Konsole duplicate не поддерживает изменение command/cwd/env",
            ));
        }

        let session_id = binding.labels.get("konsole_session_id").ok_or_else(|| {
            TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "binding missing konsole_session_id",
            )
        })?;

        let new_session = self.clone_session(session_id).await?;
        let new_pid = self
            .session_pid(&new_session)
            .await?
            .map(|pid| pid.to_string());

        let mut labels = binding.labels.clone();
        labels.insert("konsole_session_id".to_string(), new_session.clone());
        if let Some(pid) = new_pid {
            labels.insert("pid".to_string(), pid);
        }

        let token = format!("konsole-session-{new_session}");

        Ok(TerminalBinding::new(
            binding.terminal.clone(),
            token,
            labels,
            None,
        ))
    }

    async fn close(&self, binding: &TerminalBinding) -> Result<(), TermBridgeError> {
        if !Self::remote_supported() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "close",
                "Konsole DBus automation недоступно",
            ));
        }

        let session_id = binding.labels.get("konsole_session_id").ok_or_else(|| {
            TermBridgeError::not_supported(
                self.terminal_id(),
                "close",
                "binding missing konsole_session_id",
            )
        })?;

        self.close_session(session_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct MockRunner {
        calls: Mutex<Vec<Vec<String>>>,
        responses: Mutex<Vec<Result<KonsoleDbusOutput, KonsoleDbusError>>>,
    }

    impl MockRunner {
        fn new(responses: Vec<Result<KonsoleDbusOutput, KonsoleDbusError>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            }
        }

        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl KonsoleDbusRunner for MockRunner {
        async fn run(&self, args: &[String]) -> Result<KonsoleDbusOutput, KonsoleDbusError> {
            self.calls.lock().unwrap().push(args.to_vec());
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn binding_with(labels: HashMap<String, String>) -> TerminalBinding {
        TerminalBinding::new(TerminalId::new("konsole"), "token", labels, None)
    }

    fn force_platform() {
        env::set_var("SHELLDONE_TEST_FORCE_KONSOLE", "1");
        env::set_var("SHELLDONE_TEST_FORCE_KONSOLE_DBUS", "1");
    }

    #[tokio::test]
    async fn duplicate_clones_session_and_updates_labels() {
        force_platform();
        let runner = Arc::new(MockRunner::new(vec![
            Ok(KonsoleDbusOutput {
                success: true,
                exit_code: Some(0),
                stdout: b"21\n".to_vec(),
                stderr: Vec::new(),
            }),
            Ok(KonsoleDbusOutput {
                success: true,
                exit_code: Some(0),
                stdout: b"4321".to_vec(),
                stderr: Vec::new(),
            }),
        ]));
        let adapter = KonsoleAdapter::with_runner(runner.clone());

        let mut labels = HashMap::new();
        labels.insert("konsole_session_id".into(), "11".into());
        labels.insert("pid".into(), "1234".into());
        let binding = binding_with(labels);

        let duplicated = adapter
            .duplicate(&binding, &DuplicateOptions::default())
            .await
            .expect("duplicate");

        assert_eq!(
            duplicated.labels.get("konsole_session_id"),
            Some(&"21".to_string())
        );
        assert_eq!(duplicated.labels.get("pid"), Some(&"4321".to_string()));
        assert_eq!(duplicated.token, "konsole-session-21");

        let calls = runner.calls();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].contains(&"org.kde.konsole.SessionManager.cloneSession".to_string()));
        assert!(calls[1].contains(&"org.kde.konsole.Session.pid".to_string()));
    }

    #[tokio::test]
    async fn duplicate_with_overrides_not_supported() {
        force_platform();
        let runner = Arc::new(MockRunner::new(vec![]));
        let adapter = KonsoleAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("konsole_session_id".into(), "11".into());
        let binding = binding_with(labels);

        let options = DuplicateOptions {
            command: Some("ls".into()),
            ..Default::default()
        };

        let result = adapter.duplicate(&binding, &options).await;
        assert!(matches!(
            result,
            Err(TermBridgeError::NotSupported { action, .. }) if action == "duplicate"
        ));
    }

    #[tokio::test]
    async fn close_invokes_dbus() {
        force_platform();
        let runner = Arc::new(MockRunner::new(vec![Ok(KonsoleDbusOutput {
            success: true,
            exit_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })]));
        let adapter = KonsoleAdapter::with_runner(runner.clone());

        let mut labels = HashMap::new();
        labels.insert("konsole_session_id".into(), "45".into());
        let binding = binding_with(labels);

        adapter.close(&binding).await.expect("close");

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].contains(&"org.kde.konsole.Session.close".to_string()));
    }

    #[tokio::test]
    async fn close_without_session_returns_not_supported() {
        force_platform();
        let adapter = KonsoleAdapter::with_runner(Arc::new(MockRunner::new(vec![])));
        let binding = binding_with(HashMap::new());
        let result = adapter.close(&binding).await;
        assert!(matches!(
            result,
            Err(TermBridgeError::NotSupported { action, .. }) if action == "close"
        ));
    }
}
