use crate::domain::agents::{AgentBinding, AgentBindingId};
use async_trait::async_trait;

#[async_trait]
pub trait AgentBindingRepository: Send + Sync {
    async fn save(&self, binding: AgentBinding) -> anyhow::Result<()>;
    async fn get(&self, id: &AgentBindingId) -> anyhow::Result<Option<AgentBinding>>;
    async fn list(&self) -> anyhow::Result<Vec<AgentBinding>>;
    async fn delete(&self, id: &AgentBindingId) -> anyhow::Result<()>;
}
