use crate::domain::termbridge::TermBridgeState;
use async_trait::async_trait;

#[async_trait]
pub trait TermBridgeStateRepository: Send + Sync {
    async fn load(&self) -> anyhow::Result<Option<TermBridgeState>>;
    async fn save(&self, state: TermBridgeState) -> anyhow::Result<()>;
}
