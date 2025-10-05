use crate::app::ack::model::ExecArgs;
use crate::ports::ack::command_runner::CommandRunner;
use anyhow::Context;
use async_trait::async_trait;
use tokio::process::Command;

pub struct ShellCommandRunner;

impl ShellCommandRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CommandRunner for ShellCommandRunner {
    async fn run(&self, args: &ExecArgs) -> anyhow::Result<std::process::Output> {
        #[cfg(windows)]
        let shell = args.shell.as_deref().unwrap_or("cmd.exe");
        #[cfg(not(windows))]
        let shell = args.shell.as_deref().unwrap_or("sh");

        #[cfg(windows)]
        let mut command = {
            let mut cmd = Command::new(shell);
            cmd.arg("/C").arg(&args.cmd);
            cmd
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut cmd = Command::new(shell);
            cmd.arg("-c").arg(&args.cmd);
            cmd
        };

        if let Some(cwd) = &args.cwd {
            command.current_dir(cwd);
        }
        if !args.env.is_empty() {
            command.envs(args.env.clone());
        }

        command
            .output()
            .await
            .with_context(|| format!("failed to spawn command '{}'", args.cmd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runner_executes_command() {
        let runner = ShellCommandRunner::new();
        let args = ExecArgs::try_new("echo ok".to_string(), None, None, None).unwrap();
        let output = runner.run(&args).await.unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("ok"));
    }
}
