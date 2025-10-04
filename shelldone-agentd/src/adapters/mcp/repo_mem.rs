use crate::domain::mcp::{McpSession, SessionId};
use crate::ports::mcp::repo_port::McpSessionRepository;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct InMemoryMcpSessionRepository {
    inner: RwLock<HashMap<SessionId, McpSession>>,
}

impl InMemoryMcpSessionRepository {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl McpSessionRepository for InMemoryMcpSessionRepository {
    async fn insert(&self, session: McpSession) {
        self.inner
            .write()
            .await
            .insert(session.id(), session);
    }

    async fn update(&self, session: McpSession) {
        self.inner
            .write()
            .await
            .insert(session.id(), session);
    }

    async fn get(&self, id: &SessionId) -> Option<McpSession> {
        self.inner.read().await.get(id).cloned()
    }

    async fn remove(&self, id: &SessionId) {
        self.inner.write().await.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::mcp::{PersonaProfile, SessionStatus};

    #[tokio::test]
    async fn repo_round_trip() {
        let repo = InMemoryMcpSessionRepository::new();
        let mut session = McpSession::new(PersonaProfile::Core);
        let session_id = session.id();
        repo.insert(session.clone()).await;
        let stored = repo.get(&session_id).await.unwrap();
        assert_eq!(stored.status(), &SessionStatus::Negotiating);

        session
            .complete_handshake("1.0".into(), Vec::new())
            .unwrap();
        repo.update(session.clone()).await;
        let updated = repo.get(&session_id).await.unwrap();
        assert_eq!(updated.status(), &SessionStatus::Active);

        repo.remove(&session_id).await;
        assert!(repo.get(&session_id).await.is_none());
    }
}
