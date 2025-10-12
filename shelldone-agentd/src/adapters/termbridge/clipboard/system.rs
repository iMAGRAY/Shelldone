use crate::domain::termbridge::{
    ClipboardBackendDescriptor, ClipboardChannel, ClipboardContent, ClipboardMime,
};
use crate::ports::termbridge::{ClipboardBackend, ClipboardError};
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::ExitStatus;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use which::which;

#[derive(Clone, Debug)]
pub(crate) struct CommandTemplate {
    program: String,
    args: Vec<String>,
}

impl CommandTemplate {
    fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

#[derive(Clone)]
pub struct CommandClipboardBackend {
    id: &'static str,
    write_templates: HashMap<ClipboardChannel, CommandTemplate>,
    read_templates: HashMap<ClipboardChannel, CommandTemplate>,
    notes: Vec<String>,
    executor: Arc<dyn CommandExecutor>,
}

impl CommandClipboardBackend {
    pub(crate) fn new(
        id: &'static str,
        write_templates: HashMap<ClipboardChannel, CommandTemplate>,
        read_templates: HashMap<ClipboardChannel, CommandTemplate>,
        notes: Vec<String>,
        executor: Arc<dyn CommandExecutor>,
    ) -> Self {
        Self {
            id,
            write_templates,
            read_templates,
            notes,
            executor,
        }
    }

    fn resolve_template(
        templates: &HashMap<ClipboardChannel, CommandTemplate>,
        channel: ClipboardChannel,
    ) -> Option<&CommandTemplate> {
        templates
            .get(&channel)
            .or_else(|| templates.get(&ClipboardChannel::Clipboard))
    }
}

#[async_trait]
impl ClipboardBackend for CommandClipboardBackend {
    fn id(&self) -> &str {
        self.id
    }

    fn descriptor(&self) -> ClipboardBackendDescriptor {
        let mut channels: Vec<ClipboardChannel> = self
            .write_templates
            .keys()
            .chain(self.read_templates.keys())
            .copied()
            .collect();
        channels.sort_by_key(|channel| channel.as_str());
        channels.dedup();
        let can_read = !self.read_templates.is_empty();
        let can_write = !self.write_templates.is_empty();
        ClipboardBackendDescriptor::new(self.id, channels, can_read, can_write, self.notes.clone())
    }

    fn supports_channel(&self, channel: ClipboardChannel) -> bool {
        Self::resolve_template(&self.write_templates, channel)
            .or_else(|| Self::resolve_template(&self.read_templates, channel))
            .is_some()
    }

    async fn write(
        &self,
        content: &ClipboardContent,
        channel: ClipboardChannel,
    ) -> Result<(), ClipboardError> {
        let template = Self::resolve_template(&self.write_templates, channel)
            .ok_or_else(|| ClipboardError::ChannelNotSupported(channel.to_string()))?;
        let output = self
            .executor
            .run(&template.program, &template.args, Some(content.bytes()))
            .await
            .map_err(|err| ClipboardError::backend_failure(self.id, err))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(ClipboardError::backend_failure(
                self.id,
                format!("exit code {}", exit_code(&output.status)),
            ))
        }
    }

    async fn read(&self, channel: ClipboardChannel) -> Result<ClipboardContent, ClipboardError> {
        let template = Self::resolve_template(&self.read_templates, channel).ok_or_else(|| {
            ClipboardError::OperationNotSupported {
                backend: self.id.to_string(),
            }
        })?;
        let output = self
            .executor
            .run(&template.program, &template.args, None)
            .await
            .map_err(|err| ClipboardError::backend_failure(self.id, err))?;
        if !output.status.success() {
            return Err(ClipboardError::backend_failure(
                self.id,
                format!("exit code {}", exit_code(&output.status)),
            ));
        }
        let bytes = strip_trailing_newline(output.stdout);
        let content = ClipboardContent::new(bytes, ClipboardMime::text_plain_utf8())
            .map_err(|err| ClipboardError::backend_failure(self.id, err))?;
        Ok(content)
    }
}

fn exit_code(status: &ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}

fn strip_trailing_newline(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.ends_with(b"\r\n") {
        bytes.truncate(bytes.len() - 2);
    } else if bytes.ends_with(b"\n") {
        bytes.truncate(bytes.len() - 1);
    }
    bytes
}

#[async_trait]
pub trait CommandExecutor: Send + Sync {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput, String>;
}

#[derive(Debug)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    #[allow(dead_code)]
    pub stderr: Vec<u8>,
}

pub struct SystemCommandExecutor;

impl Default for SystemCommandExecutor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl CommandExecutor for SystemCommandExecutor {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput, String> {
        let mut command = Command::new(program);
        command.args(args);
        if stdin.is_some() {
            command.stdin(std::process::Stdio::piped());
        }
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|err| format!("spawn {program}: {err}"))?;

        if let Some(input) = stdin {
            if let Some(mut handle) = child.stdin.take() {
                handle
                    .write_all(input)
                    .await
                    .map_err(|err| format!("write stdin for {program}: {err}"))?;
            }
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|err| format!("wait {program}: {err}"))?;

        Ok(CommandOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

/// Build default clipboard backends for current runtime.
pub fn default_clipboard_backends(
    executor: Arc<dyn CommandExecutor>,
) -> Vec<Arc<CommandClipboardBackend>> {
    let mut backends = Vec::new();

    if cfg!(target_os = "macos") && has_command("pbcopy") {
        let mut write = HashMap::new();
        write.insert(
            ClipboardChannel::Clipboard,
            CommandTemplate::new("pbcopy", Vec::new()),
        );
        let mut read = HashMap::new();
        if has_command("pbpaste") {
            read.insert(
                ClipboardChannel::Clipboard,
                CommandTemplate::new("pbpaste", Vec::new()),
            );
        }
        backends.push(Arc::new(CommandClipboardBackend::new(
            "pbcopy",
            write,
            read,
            vec!["macOS clipboard".to_string()],
            executor.clone(),
        )));
    }

    if cfg!(target_os = "linux") {
        if is_wayland() && has_command("wl-copy") {
            let mut write = HashMap::new();
            write.insert(
                ClipboardChannel::Clipboard,
                CommandTemplate::new("wl-copy", vec!["--type".into(), "text/plain".into()]),
            );
            write.insert(
                ClipboardChannel::Primary,
                CommandTemplate::new(
                    "wl-copy",
                    vec!["--primary".into(), "--type".into(), "text/plain".into()],
                ),
            );
            let mut read = HashMap::new();
            if has_command("wl-paste") {
                read.insert(
                    ClipboardChannel::Clipboard,
                    CommandTemplate::new("wl-paste", vec!["--type".into(), "text/plain".into()]),
                );
                read.insert(
                    ClipboardChannel::Primary,
                    CommandTemplate::new(
                        "wl-paste",
                        vec!["--primary".into(), "--type".into(), "text/plain".into()],
                    ),
                );
            }
            backends.push(Arc::new(CommandClipboardBackend::new(
                "wl-copy",
                write,
                read,
                vec!["Wayland wl-copy/wl-paste".to_string()],
                executor.clone(),
            )));
        }

        if has_command("xclip") {
            let mut write = HashMap::new();
            write.insert(
                ClipboardChannel::Clipboard,
                CommandTemplate::new(
                    "xclip",
                    vec!["-selection".into(), "clipboard".into(), "-in".into()],
                ),
            );
            write.insert(
                ClipboardChannel::Primary,
                CommandTemplate::new(
                    "xclip",
                    vec!["-selection".into(), "primary".into(), "-in".into()],
                ),
            );
            let mut read = HashMap::new();
            read.insert(
                ClipboardChannel::Clipboard,
                CommandTemplate::new(
                    "xclip",
                    vec!["-selection".into(), "clipboard".into(), "-out".into()],
                ),
            );
            read.insert(
                ClipboardChannel::Primary,
                CommandTemplate::new(
                    "xclip",
                    vec!["-selection".into(), "primary".into(), "-out".into()],
                ),
            );
            backends.push(Arc::new(CommandClipboardBackend::new(
                "xclip",
                write,
                read,
                vec!["X11 xclip bridge".to_string()],
                executor.clone(),
            )));
        }
    }

    if (cfg!(target_os = "windows") || is_wsl()) && has_command("clip.exe") {
        let mut write = HashMap::new();
        write.insert(
            ClipboardChannel::Clipboard,
            CommandTemplate::new("clip.exe", Vec::new()),
        );
        let mut read = HashMap::new();
        if has_command("powershell.exe") {
            read.insert(
                ClipboardChannel::Clipboard,
                CommandTemplate::new(
                    "powershell.exe",
                    vec![
                        "-NoLogo".into(),
                        "-NoProfile".into(),
                        "-Command".into(),
                        "Get-Clipboard".into(),
                    ],
                ),
            );
        }
        backends.push(Arc::new(CommandClipboardBackend::new(
            "clip.exe",
            write,
            read,
            vec!["Windows clipboard bridge".to_string()],
            executor.clone(),
        )));
    }

    backends
}

fn has_command(cmd: &str) -> bool {
    which(cmd).is_ok()
}

fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

fn is_wsl() -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WSL_DISTRO_NAME").is_ok() {
            return true;
        }
        if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
            return release.to_ascii_lowercase().contains("microsoft");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    type CommandCall = (String, Vec<String>, Vec<u8>);

    #[derive(Default)]
    struct MockExecutor {
        calls: Mutex<Vec<CommandCall>>,
        success: bool,
        stdout: Vec<u8>,
    }

    #[async_trait]
    impl CommandExecutor for MockExecutor {
        async fn run(
            &self,
            program: &str,
            args: &[String],
            stdin: Option<&[u8]>,
        ) -> Result<CommandOutput, String> {
            self.calls.lock().unwrap().push((
                program.to_string(),
                args.to_vec(),
                stdin.unwrap_or(&[]).to_vec(),
            ));
            if self.success {
                Ok(CommandOutput {
                    status: <ExitStatus as ExitStatusShim>::from_raw(0),
                    stdout: self.stdout.clone(),
                    stderr: Vec::new(),
                })
            } else {
                Ok(CommandOutput {
                    status: <ExitStatus as ExitStatusShim>::from_raw(1),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
        }
    }

    #[cfg(unix)]
    trait ExitStatusShim {
        fn from_raw(code: i32) -> ExitStatus;
    }

    #[cfg(unix)]
    impl ExitStatusShim for ExitStatus {
        fn from_raw(code: i32) -> ExitStatus {
            use std::os::unix::process::ExitStatusExt;
            ExitStatusExt::from_raw(code << 8)
        }
    }

    #[cfg(windows)]
    impl ExitStatusShim for ExitStatus {
        fn from_raw(code: i32) -> ExitStatus {
            use std::os::windows::process::ExitStatusExt;
            ExitStatusExt::from_raw(code as u32)
        }
    }

    #[tokio::test]
    async fn command_backend_invokes_executor_with_bytes() {
        let executor = Arc::new(MockExecutor {
            success: true,
            stdout: Vec::new(),
            ..Default::default()
        });
        let mut write = HashMap::new();
        write.insert(
            ClipboardChannel::Clipboard,
            CommandTemplate::new("tool", vec!["--flag".into()]),
        );
        let backend = CommandClipboardBackend::new(
            "tool",
            write,
            HashMap::new(),
            Vec::new(),
            executor.clone(),
        );
        let content = ClipboardContent::from_text("payload").unwrap();
        backend
            .write(&content, ClipboardChannel::Clipboard)
            .await
            .unwrap();
        let calls = executor.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "tool");
        assert_eq!(calls[0].1, vec!["--flag".to_string()]);
        assert_eq!(calls[0].2, b"payload");
    }

    #[tokio::test]
    async fn command_backend_reads_output() {
        let executor = Arc::new(MockExecutor {
            success: true,
            stdout: b"hello\n".to_vec(),
            ..Default::default()
        });
        let backend = CommandClipboardBackend::new(
            "tool",
            HashMap::new(),
            {
                let mut map = HashMap::new();
                map.insert(
                    ClipboardChannel::Clipboard,
                    CommandTemplate::new("tool-read", Vec::new()),
                );
                map
            },
            Vec::new(),
            executor.clone(),
        );
        let content = backend.read(ClipboardChannel::Clipboard).await.unwrap();
        assert_eq!(content.bytes(), b"hello");
        let calls = executor.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "tool-read");
    }
}
