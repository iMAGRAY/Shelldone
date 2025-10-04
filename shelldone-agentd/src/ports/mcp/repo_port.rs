use crate::domain::mcp::McpSession;
use crate::domain::mcp::SessionId;
use async_trait::async_trait;

#[async_trait]
pub trait McpSessionRepository: Send + Sync {
    async fn insert(&self, session: McpSession);
    async fn update(&self, session: McpSession);
    async fn get(&self, id: &SessionId) -> Option<McpSession>;
    async fn remove(&self, id: &SessionId);
}
