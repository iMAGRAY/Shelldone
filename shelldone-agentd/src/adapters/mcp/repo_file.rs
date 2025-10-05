use crate::domain::mcp::aggregate::{McpSession, McpSessionSnapshot};
use crate::domain::mcp::SessionId;
use crate::ports::mcp::repo_port::McpSessionRepository;
use anyhow::Context;
use async_trait::async_trait;
use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::error;

const TMP_SUFFIX: &str = ".tmp";

pub struct FileMcpSessionRepository {
    path: PathBuf,
    inner: RwLock<HashMap<SessionId, McpSession>>,
}

impl FileMcpSessionRepository {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        let sessions = if path.exists() {
            let data = std::fs::read(&path)
                .with_context(|| format!("reading MCP session store {}", path.display()))?;
            if data.is_empty() {
                HashMap::new()
            } else {
                let snapshots: Vec<McpSessionSnapshot> = serde_json::from_slice(&data)
                    .with_context(|| format!("parsing MCP session store {}", path.display()))?;
                snapshots
                    .into_iter()
                    .map(|snapshot| {
                        let id = snapshot.id.clone();
                        match McpSession::from_snapshot(snapshot) {
                            Ok(session) => Ok((id, session)),
                            Err(err) => Err(anyhow::anyhow!(err)),
                        }
                    })
                    .collect::<Result<HashMap<_, _>, _>>()?
            }
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            inner: RwLock::new(sessions),
        })
    }

    async fn persist(&self) -> anyhow::Result<()> {
        let sessions = self.inner.read().await;
        let snapshots: Vec<McpSessionSnapshot> = sessions
            .values()
            .cloned()
            .map(|session| session.to_snapshot())
            .collect();
        let json = serde_json::to_vec_pretty(&snapshots)?;
        let tmp_path = self.path.with_extension(format!(
            "{}{}",
            self.path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("json"),
            TMP_SUFFIX
        ));

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(&tmp_path)
            .await
            .with_context(|| format!("creating temp MCP session store {}", tmp_path.display()))?;
        file.write_all(&json)
            .await
            .with_context(|| format!("writing temp MCP session store {}", tmp_path.display()))?;
        file.flush().await?;
        drop(file);
        fs::rename(&tmp_path, &self.path)
            .await
            .with_context(|| format!("renaming MCP session store to {}", self.path.display()))?;
        Ok(())
    }

    async fn persist_or_log(&self) {
        if let Err(err) = self.persist().await {
            error!(%err, "failed to persist MCP sessions");
        }
    }
}

#[async_trait]
impl McpSessionRepository for FileMcpSessionRepository {
    async fn insert(&self, session: McpSession) {
        self.inner.write().await.insert(session.id(), session);
        self.persist_or_log().await;
    }

    async fn update(&self, session: McpSession) {
        self.inner.write().await.insert(session.id(), session);
        self.persist_or_log().await;
    }

    async fn get(&self, id: &SessionId) -> Option<McpSession> {
        self.inner.read().await.get(id).cloned()
    }

    async fn remove(&self, id: &SessionId) {
        self.inner.write().await.remove(id);
        self.persist_or_log().await;
    }

    async fn list(&self) -> Vec<McpSession> {
        self.inner.read().await.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::mcp::{CapabilityName, PersonaProfile, SessionStatus};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn persists_and_loads_sessions() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("sessions.json");
        let repo = FileMcpSessionRepository::new(path.clone()).unwrap();

        let mut session = McpSession::new(PersonaProfile::Core);
        session
            .complete_handshake("1.0".into(), vec![CapabilityName::new("fs").unwrap()])
            .unwrap();
        let session_id = session.id();
        repo.insert(session.clone()).await;

        let loaded_repo = FileMcpSessionRepository::new(path).unwrap();
        let restored = loaded_repo.get(&session_id).await.unwrap();
        assert_eq!(restored.persona().name(), session.persona().name());
        assert_eq!(restored.protocol_version(), session.protocol_version());
        assert_eq!(restored.status(), &SessionStatus::Active);
    }

    #[tokio::test]
    async fn corrupted_store_returns_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("sessions.json");
        std::fs::write(&path, b"not-json").unwrap();
        let repo = FileMcpSessionRepository::new(path);
        assert!(repo.is_err(), "expected error on corrupted session store");
    }

    #[tokio::test]
    async fn concurrent_updates_do_not_drop_sessions() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("sessions.json");
        let repo = Arc::new(FileMcpSessionRepository::new(path).unwrap());

        let mut session = McpSession::new(PersonaProfile::Core);
        session
            .complete_handshake("1.0".into(), vec![CapabilityName::new("fs").unwrap()])
            .unwrap();
        let session_id = session.id();

        let repo_insert = repo.clone();
        let insert_session = session.clone();
        let insert_handle = tokio::spawn(async move {
            repo_insert.insert(insert_session).await;
        });

        let repo_update = repo.clone();
        let update_session = session.clone();
        let update_handle = tokio::spawn(async move {
            repo_update.update(update_session).await;
        });

        insert_handle.await.unwrap();
        update_handle.await.unwrap();

        let stored = repo.get(&session_id).await.unwrap();
        assert_eq!(stored.persona().name(), session.persona().name());
        assert_eq!(stored.status(), &SessionStatus::Active);
    }
}
