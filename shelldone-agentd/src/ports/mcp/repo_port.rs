use crate::domain::mcp::{McpSession, SessionId};
use async_trait::async_trait;

#[allow(dead_code)]
#[async_trait]
pub trait McpSessionRepository: Send + Sync {
    async fn insert(&self, session: McpSession);
    async fn update(&self, session: McpSession);
    async fn get(&self, id: &SessionId) -> Option<McpSession>;
    async fn remove(&self, id: &SessionId);
    async fn list(&self) -> Vec<McpSession>;
}
