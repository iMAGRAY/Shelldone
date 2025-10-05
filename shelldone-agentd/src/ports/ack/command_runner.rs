use crate::app::ack::model::ExecArgs;
use async_trait::async_trait;

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, args: &ExecArgs) -> anyhow::Result<std::process::Output>;
}
