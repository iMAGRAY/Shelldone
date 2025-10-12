use crate::domain::termbridge::{TerminalBinding, TerminalCapabilities, TerminalId};
use crate::ports::termbridge::{
    CapabilityObservation, DuplicateOptions, DuplicateStrategy, SpawnRequest, TermBridgeError,
    TerminalControlPort,
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

use super::util::{decode_env, encode_env};

const KITTY_REMOTE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone)]
struct KittyCommandOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::termbridge::TerminalId;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct MockRunner {
        calls: Mutex<Vec<Vec<String>>>,
        responses: Mutex<Vec<Result<KittyCommandOutput, KittyCommandError>>>,
    }

    impl MockRunner {
        fn new(responses: Vec<Result<KittyCommandOutput, KittyCommandError>>) -> Self {
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
    impl KittyCommandRunner for MockRunner {
        async fn run(&self, args: &[String]) -> Result<KittyCommandOutput, KittyCommandError> {
            self.calls.lock().unwrap().push(args.to_vec());
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn binding_with(labels: HashMap<String, String>) -> TerminalBinding {
        TerminalBinding::new(TerminalId::new("kitty"), "token", labels, None)
    }

    fn force_binary_available() {
        std::env::set_var("SHELLDONE_TEST_FORCE_KITTY", "1");
    }

    #[tokio::test]
    async fn duplicate_uses_remote_launch() {
        force_binary_available();
        let runner = Arc::new(MockRunner::new(vec![Ok(
            KittyCommandOutput::success_with_stdout("4242\n"),
        )]));
        let adapter = KittyAdapter::with_runner(runner.clone());

        let mut labels = HashMap::new();
        labels.insert("kitty_listen_on".into(), "unix:/tmp/kitty.sock".into());
        labels.insert("kitty_window_id".into(), "777".into());
        labels.insert("command".into(), "htop".into());
        labels.insert("cwd".into(), "/workspace".into());
        let binding = binding_with(labels);

        let mut options = DuplicateOptions {
            strategy: DuplicateStrategy::HorizontalSplit,
            command: Some("ls".into()),
            cwd: Some("/tmp".into()),
            ..Default::default()
        };
        options.env.insert("FOO".into(), "BAR".into());

        let duplicated = adapter
            .duplicate(&binding, &options)
            .await
            .expect("duplicate");

        assert_eq!(
            duplicated.labels.get("kitty_window_id"),
            Some(&"4242".to_string())
        );
        assert_eq!(
            duplicated.labels.get("kitty_listen_on"),
            Some(&"unix:/tmp/kitty.sock".to_string())
        );
        assert_eq!(duplicated.labels.get("command"), Some(&"ls".to_string()));
        assert_eq!(duplicated.labels.get("cwd"), Some(&"/tmp".to_string()));
        assert_eq!(duplicated.labels.get("env:FOO"), Some(&"BAR".to_string()));
        assert_eq!(duplicated.token, "kitty-window-4242");

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        let expected: Vec<String> = vec![
            "@",
            "launch",
            "--to",
            "unix:/tmp/kitty.sock",
            "--match",
            "window_id:777",
            "--location",
            "hsplit",
            "--cwd",
            "/tmp",
            "--env",
            "FOO=BAR",
            "--",
            "sh",
            "-lc",
            "ls",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(calls[0], expected);
    }

    #[tokio::test]
    async fn duplicate_without_remote_returns_not_supported() {
        force_binary_available();
        let runner = Arc::new(MockRunner::new(vec![]));
        let adapter = KittyAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("kitty_window_id".into(), "42".into());
        let binding = binding_with(labels);

        let result = adapter
            .duplicate(&binding, &DuplicateOptions::default())
            .await;

        assert!(matches!(
            result,
            Err(TermBridgeError::NotSupported { action, .. }) if action == "duplicate"
        ));
    }

    #[tokio::test]
    async fn duplicate_uses_pid_selector_when_no_window() {
        force_binary_available();
        let runner = Arc::new(MockRunner::new(vec![Ok(
            KittyCommandOutput::success_with_stdout("4243"),
        )]));
        let adapter = KittyAdapter::with_runner(runner.clone());

        let mut labels = HashMap::new();
        labels.insert("kitty_listen_on".into(), "unix:/tmp/kitty.sock".into());
        labels.insert("pid".into(), "1234".into());
        let binding = binding_with(labels);

        let duplicated = adapter
            .duplicate(&binding, &DuplicateOptions::default())
            .await
            .expect("duplicate");

        assert_eq!(
            duplicated.labels.get("kitty_window_id"),
            Some(&"4243".to_string())
        );

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].contains(&"pid:1234".to_string()));
    }

    #[tokio::test]
    async fn close_invokes_remote_close_window() {
        force_binary_available();
        let runner = Arc::new(MockRunner::new(vec![Ok(
            KittyCommandOutput::success_with_stdout(Vec::<u8>::new()),
        )]));
        let adapter = KittyAdapter::with_runner(runner.clone());

        let mut labels = HashMap::new();
        labels.insert("kitty_listen_on".into(), "unix:/tmp/kitty.sock".into());
        labels.insert("kitty_window_id".into(), "555".into());
        let binding = binding_with(labels);

        adapter.close(&binding).await.expect("close");

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        let expected: Vec<String> = vec![
            "@",
            "close-window",
            "--to",
            "unix:/tmp/kitty.sock",
            "--match",
            "window_id:555",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(calls[0], expected);
    }

    #[tokio::test]
    async fn close_failure_returns_internal_error() {
        force_binary_available();
        let runner = Arc::new(MockRunner::new(vec![Ok(KittyCommandOutput::failure_with(
            "failed",
            Some(1),
        ))]));
        let adapter = KittyAdapter::with_runner(runner);

        let mut labels = HashMap::new();
        labels.insert("kitty_listen_on".into(), "unix:/tmp/kitty.sock".into());
        labels.insert("kitty_window_id".into(), "999".into());
        let binding = binding_with(labels);

        let result = adapter.close(&binding).await;
        assert!(matches!(result, Err(TermBridgeError::Internal(_))));
    }
}

impl KittyCommandOutput {
    #[cfg(test)]
    fn success_with_stdout(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }

    #[cfg(test)]
    fn failure_with(stderr: impl Into<Vec<u8>>, exit_code: Option<i32>) -> Self {
        Self {
            success: false,
            exit_code,
            stdout: Vec::new(),
            stderr: stderr.into(),
        }
    }
}

#[derive(Debug, Clone)]
enum KittyCommandError {
    MissingBinary,
    Timeout { command: String },
    Io { command: String, message: String },
}

#[async_trait]
trait KittyCommandRunner: Send + Sync {
    async fn run(&self, args: &[String]) -> Result<KittyCommandOutput, KittyCommandError>;
}

#[derive(Default)]
struct SystemKittyCommandRunner;

impl SystemKittyCommandRunner {
    fn resolve_cli_path() -> Option<PathBuf> {
        which("kitty").ok()
    }

    fn command_label(args: &[String]) -> String {
        let mut label = String::from("kitty");
        if !args.is_empty() {
            label.push(' ');
            label.push_str(&args.join(" "));
        }
        label
    }

    fn build_command(&self, args: &[String]) -> Result<Command, KittyCommandError> {
        let path = Self::resolve_cli_path().ok_or(KittyCommandError::MissingBinary)?;
        let mut cmd = Command::new(path);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        Ok(cmd)
    }
}

#[async_trait]
impl KittyCommandRunner for SystemKittyCommandRunner {
    async fn run(&self, args: &[String]) -> Result<KittyCommandOutput, KittyCommandError> {
        let command_label = Self::command_label(args);
        let mut cmd = self.build_command(args)?;
        let output = timeout(KITTY_REMOTE_TIMEOUT, cmd.output())
            .await
            .map_err(|_| KittyCommandError::Timeout {
                command: command_label.clone(),
            })?;
        let output = output.map_err(|err| KittyCommandError::Io {
            command: command_label.clone(),
            message: err.to_string(),
        })?;
        Ok(KittyCommandOutput {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

pub struct KittyAdapter {
    runner: Arc<dyn KittyCommandRunner>,
}

impl KittyAdapter {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(SystemKittyCommandRunner),
        }
    }

    #[cfg(test)]
    fn with_runner(runner: Arc<dyn KittyCommandRunner>) -> Self {
        Self { runner }
    }

    fn has_binary() -> bool {
        if cfg!(test) && env::var("SHELLDONE_TEST_FORCE_KITTY").ok().as_deref() == Some("1") {
            return true;
        }
        SystemKittyCommandRunner::resolve_cli_path().is_some()
    }

    fn listen_on_env() -> Option<String> {
        env::var("KITTY_LISTEN_ON").ok()
    }

    fn binding_listen_on(binding: &TerminalBinding) -> Option<String> {
        binding.labels.get("kitty_listen_on").cloned()
    }

    fn map_error(&self, action: &str, err: KittyCommandError) -> TermBridgeError {
        match err {
            KittyCommandError::MissingBinary => TermBridgeError::not_supported(
                self.terminal_id(),
                action,
                "kitty remote control not available (binary missing)",
            ),
            KittyCommandError::Timeout { command } => {
                TermBridgeError::internal(self.terminal_id(), format!("{command} timed out"))
            }
            KittyCommandError::Io { command, message } => {
                TermBridgeError::internal(self.terminal_id(), format!("{command}: {message}"))
            }
        }
    }

    async fn run_command(
        &self,
        args: Vec<String>,
        action: &str,
    ) -> Result<KittyCommandOutput, TermBridgeError> {
        self.runner
            .run(&args)
            .await
            .map_err(|err| self.map_error(action, err))
    }

    fn listen_on_or_binding(binding: &TerminalBinding) -> Option<String> {
        Self::binding_listen_on(binding).or_else(Self::listen_on_env)
    }

    fn match_selector(binding: &TerminalBinding) -> Option<String> {
        binding
            .labels
            .get("kitty_window_id")
            .map(|id| format!("window_id:{id}"))
            .or_else(|| binding.labels.get("pid").map(|pid| format!("pid:{pid}")))
    }

    async fn spawn_via_remote(
        &self,
        request: &SpawnRequest,
        listen_on: String,
    ) -> Result<TerminalBinding, TermBridgeError> {
        let mut args = vec![
            "@".to_string(),
            "launch".to_string(),
            "--to".to_string(),
            listen_on.clone(),
            "--type".to_string(),
            "os-window".to_string(),
        ];

        if let Some(cwd) = &request.cwd {
            args.push("--cwd".to_string());
            args.push(cwd.clone());
        }

        for (key, value) in &request.env {
            args.push("--env".to_string());
            args.push(format!("{key}={value}"));
        }

        if let Some(command) = &request.command {
            args.push("--".to_string());
            args.push("sh".to_string());
            args.push("-lc".to_string());
            args.push(command.clone());
        }

        let output = self.run_command(args, "spawn").await?;
        if !output.success {
            return Err(TermBridgeError::internal(
                self.terminal_id(),
                format!(
                    "kitty @ launch failed (code {:?}): {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let window_id = Self::parse_window_id(&output.stdout).ok_or_else(|| {
            TermBridgeError::internal(
                self.terminal_id(),
                "kitty @ launch did not return window id",
            )
        })?;

        let mut labels: HashMap<String, String> = HashMap::new();
        labels.insert("kitty_window_id".to_string(), window_id.clone());
        labels.insert("kitty_listen_on".to_string(), listen_on.clone());
        if let Some(command) = &request.command {
            labels.insert("command".to_string(), command.clone());
        }
        if let Some(cwd) = &request.cwd {
            labels.insert("cwd".to_string(), cwd.clone());
        }
        encode_env(&mut labels, &request.env);

        let token = format!("kitty-window-{window_id}");
        Ok(TerminalBinding::new(
            request.terminal.clone(),
            token,
            labels,
            None,
        ))
    }

    async fn spawn_via_process(
        &self,
        request: &SpawnRequest,
    ) -> Result<TerminalBinding, TermBridgeError> {
        let mut cmd = Command::new("kitty");
        if let Some(cwd) = &request.cwd {
            cmd.current_dir(cwd);
        }
        cmd.kill_on_drop(false);

        for (key, value) in &request.env {
            cmd.env(key, value);
        }
        if let Some(command) = &request.command {
            cmd.arg("--");
            cmd.arg("sh");
            cmd.arg("-lc");
            cmd.arg(command);
        }
        let mut child = cmd.spawn().map_err(|err| {
            TermBridgeError::internal(self.terminal_id(), format!("kitty spawn failed: {err}"))
        })?;
        let pid = child.id().ok_or_else(|| {
            TermBridgeError::internal(self.terminal_id(), "kitty spawn missing process id")
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

        if let Some(listen) = Self::listen_on_env() {
            labels.insert("kitty_listen_on".to_string(), listen);
        }

        let token = format!("kitty-{pid}");
        Ok(TerminalBinding::new(
            request.terminal.clone(),
            token,
            labels,
            None,
        ))
    }

    fn parse_window_id(bytes: &[u8]) -> Option<String> {
        let value = String::from_utf8_lossy(bytes).trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }
}

#[async_trait]
impl TerminalControlPort for KittyAdapter {
    fn terminal_id(&self) -> TerminalId {
        TerminalId::new("kitty")
    }

    async fn detect(&self) -> CapabilityObservation {
        let binary = Self::has_binary();
        let listen_on = Self::listen_on_env();
        let remote = binary && listen_on.is_some();
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
            .duplicate(remote)
            .close(remote)
            .send_text(remote)
            .clipboard_write(true)
            .clipboard_read(false)
            .cwd_sync(true)
            .bracketed_paste(true)
            .max_clipboard_kb(Some(75))
            .build();
        CapabilityObservation::new("kitty", capabilities, listen_on.is_none(), notes)
    }

    async fn spawn(&self, request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
        if !Self::has_binary() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "spawn",
                "kitty binary not available",
            ));
        }

        if let Some(listen_on) = Self::listen_on_env() {
            return self.spawn_via_remote(request, listen_on).await;
        }

        self.spawn_via_process(request).await
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

    async fn duplicate(
        &self,
        binding: &TerminalBinding,
        options: &DuplicateOptions,
    ) -> Result<TerminalBinding, TermBridgeError> {
        if !Self::has_binary() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "kitty binary not available",
            ));
        }

        let listen_on = Self::listen_on_or_binding(binding).ok_or_else(|| {
            TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "kitty remote control not configured",
            )
        })?;

        let selector = Self::match_selector(binding).ok_or_else(|| {
            TermBridgeError::not_supported(
                self.terminal_id(),
                "duplicate",
                "kitty binding missing remote handle",
            )
        })?;

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

        let mut args = vec![
            "@".to_string(),
            "launch".to_string(),
            "--to".to_string(),
            listen_on.clone(),
            "--match".to_string(),
            selector,
        ];

        match options.strategy {
            DuplicateStrategy::HorizontalSplit => {
                args.push("--location".to_string());
                args.push("hsplit".to_string());
            }
            DuplicateStrategy::VerticalSplit => {
                args.push("--location".to_string());
                args.push("vsplit".to_string());
            }
            DuplicateStrategy::NewTab => {
                args.push("--type".to_string());
                args.push("tab".to_string());
            }
            DuplicateStrategy::NewWindow => {
                args.push("--type".to_string());
                args.push("os-window".to_string());
            }
        }

        if let Some(cwd) = &cwd {
            args.push("--cwd".to_string());
            args.push(cwd.clone());
        }

        for (key, value) in &env_map {
            args.push("--env".to_string());
            args.push(format!("{key}={value}"));
        }

        if let Some(command) = &command {
            args.push("--".to_string());
            args.push("sh".to_string());
            args.push("-lc".to_string());
            args.push(command.clone());
        }

        let output = self.run_command(args, "duplicate").await?;
        if !output.success {
            return Err(TermBridgeError::internal(
                self.terminal_id(),
                format!(
                    "kitty @ launch duplicate failed (code {:?}): {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let window_id = Self::parse_window_id(&output.stdout).ok_or_else(|| {
            TermBridgeError::internal(
                self.terminal_id(),
                "kitty duplicate did not return window id",
            )
        })?;

        let mut labels: HashMap<String, String> = HashMap::new();
        labels.insert("kitty_window_id".to_string(), window_id.clone());
        labels.insert("kitty_listen_on".to_string(), listen_on);
        if let Some(command) = &command {
            labels.insert("command".to_string(), command.clone());
        }
        if let Some(cwd) = &cwd {
            labels.insert("cwd".to_string(), cwd.clone());
        }
        encode_env(&mut labels, &env_map);

        let token = format!("kitty-window-{window_id}");
        Ok(TerminalBinding::new(
            binding.terminal.clone(),
            token,
            labels,
            None,
        ))
    }

    async fn close(&self, binding: &TerminalBinding) -> Result<(), TermBridgeError> {
        if !Self::has_binary() {
            return Err(TermBridgeError::not_supported(
                self.terminal_id(),
                "close",
                "kitty binary not available",
            ));
        }

        let listen_on = Self::listen_on_or_binding(binding).ok_or_else(|| {
            TermBridgeError::not_supported(
                self.terminal_id(),
                "close",
                "kitty remote control not configured",
            )
        })?;

        let selector = Self::match_selector(binding).ok_or_else(|| {
            TermBridgeError::not_supported(
                self.terminal_id(),
                "close",
                "kitty binding missing remote handle",
            )
        })?;

        let args = vec![
            "@".to_string(),
            "close-window".to_string(),
            "--to".to_string(),
            listen_on,
            "--match".to_string(),
            selector,
        ];

        let output = self.run_command(args, "close").await?;
        if output.success {
            Ok(())
        } else {
            Err(TermBridgeError::internal(
                self.terminal_id(),
                format!(
                    "kitty @ close-window failed (code {:?}): {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ))
        }
    }
}
