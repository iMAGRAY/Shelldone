use crate::domain::agents::{AgentBinding, AgentBindingId};
use crate::ports::agents::AgentBindingRepository;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct InMemoryAgentBindingRepository {
    bindings: RwLock<HashMap<AgentBindingId, AgentBinding>>,
}

impl InMemoryAgentBindingRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AgentBindingRepository for InMemoryAgentBindingRepository {
    async fn save(&self, binding: AgentBinding) -> Result<()> {
        self.bindings.write().await.insert(binding.id(), binding);
        Ok(())
    }

    async fn get(&self, id: &AgentBindingId) -> Result<Option<AgentBinding>> {
        Ok(self.bindings.read().await.get(id).cloned())
    }

    async fn list(&self) -> Result<Vec<AgentBinding>> {
        Ok(self.bindings.read().await.values().cloned().collect())
    }

    async fn delete(&self, id: &AgentBindingId) -> Result<()> {
        self.bindings.write().await.remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agents::{
        AgentBinding, AgentProvider, CapabilityName, CapabilitySet, SdkChannel, SdkVersion,
    };

    fn mk_binding() -> AgentBinding {
        let (binding, _) = AgentBinding::register(
            AgentProvider::OpenAi,
            SdkVersion::new("1.0.0").unwrap(),
            SdkChannel::Stable,
            CapabilitySet::new(vec![CapabilityName::new("fs.read").unwrap()]).unwrap(),
        )
        .unwrap();
        binding
    }

    #[tokio::test]
    async fn repository_roundtrip() {
        let repo = InMemoryAgentBindingRepository::new();
        let binding = mk_binding();
        let id = binding.id();
        repo.save(binding.clone()).await.unwrap();
        let loaded = repo.get(&id).await.unwrap().unwrap();
        assert_eq!(loaded.provider(), binding.provider());
        let list = repo.list().await.unwrap();
        assert_eq!(list.len(), 1);
        repo.delete(&id).await.unwrap();
        assert!(repo.get(&id).await.unwrap().is_none());
    }
}
