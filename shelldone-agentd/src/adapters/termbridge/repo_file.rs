use crate::domain::termbridge::TermBridgeState;
use crate::ports::termbridge::TermBridgeStateRepository;
use anyhow::Context;
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Persist TermBridge capability maps on disk so discovery survives restarts.
#[derive(Debug, Clone)]
pub struct FileTermBridgeStateRepository {
    path: PathBuf,
}

impl FileTermBridgeStateRepository {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    async fn ensure_parent_dir(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("creating termbridge state dir at {}", parent.display())
            })?;
        }
        Ok(())
    }
}

#[async_trait]
impl TermBridgeStateRepository for FileTermBridgeStateRepository {
    async fn load(&self) -> anyhow::Result<Option<TermBridgeState>> {
        match fs::read(&self.path).await {
            Ok(bytes) => {
                if bytes.is_empty() {
                    return Ok(None);
                }
                let value: Value = serde_json::from_slice(&bytes).with_context(|| {
                    format!("parsing termbridge state file {}", self.path.display())
                })?;
                let state: TermBridgeState = serde_json::from_value(value).with_context(|| {
                    format!(
                        "deserializing termbridge state from {}",
                        self.path.display()
                    )
                })?;
                Ok(Some(state))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(anyhow::Error::from(err).context(format!(
                "reading termbridge state file {}",
                self.path.display()
            ))),
        }
    }

    async fn save(&self, state: TermBridgeState) -> anyhow::Result<()> {
        self.ensure_parent_dir().await?;
        let json = serde_json::to_vec_pretty(&state)
            .context("serializing termbridge capability map to json")?;
        let tmp_path = self
            .path
            .parent()
            .map(|dir| dir.join("state.json.tmp"))
            .unwrap_or_else(|| self.path.with_extension("tmp"));
        fs::write(&tmp_path, json)
            .await
            .with_context(|| format!("writing termbridge state tmp file {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &self.path).await.with_context(|| {
            format!(
                "atomically replacing termbridge state file {}",
                self.path.display()
            )
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::termbridge::{
        CapabilityRecord, TermBridgeState, TerminalCapabilities, TerminalId,
    };
    use tempfile::tempdir;

    #[tokio::test]
    async fn load_returns_none_when_file_missing() {
        let dir = tempdir().unwrap();
        let repo = FileTermBridgeStateRepository::new(dir.path().join("capabilities.json"));
        assert!(repo.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let repo = FileTermBridgeStateRepository::new(dir.path().join("capabilities.json"));
        let mut state = TermBridgeState::new();
        let record = CapabilityRecord::new(
            TerminalId::new("testterm"),
            "TestTerm",
            false,
            TerminalCapabilities::builder().send_text(true).build(),
            vec!["note".to_string()],
        );
        state.update_capabilities(vec![record.clone()]);
        repo.save(state.clone()).await.unwrap();

        let loaded = repo.load().await.unwrap().unwrap();
        assert_eq!(loaded.capabilities(), vec![record]);
        assert!(loaded.discovered_at().is_some());
    }
}
