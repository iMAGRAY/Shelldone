use crate::domain::termbridge::{TerminalBinding, TerminalBindingId};
use async_trait::async_trait;

#[async_trait]
pub trait TerminalBindingRepository: Send + Sync {
    async fn save(&self, binding: TerminalBinding) -> anyhow::Result<TerminalBinding>;
    async fn get(&self, id: &TerminalBindingId) -> anyhow::Result<Option<TerminalBinding>>;
    #[allow(dead_code)]
    async fn delete(&self, id: &TerminalBindingId) -> anyhow::Result<()>;
    async fn list(&self) -> anyhow::Result<Vec<TerminalBinding>>;
}
