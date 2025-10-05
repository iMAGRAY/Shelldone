use crate::domain::termbridge::{TermBridgeState, TerminalBinding, TerminalBindingId};
use crate::ports::termbridge::{TermBridgeStateRepository, TerminalBindingRepository};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Default)]
pub struct InMemoryTermBridgeStateRepository {
    state: RwLock<Option<TermBridgeState>>,
}

#[async_trait]
impl TermBridgeStateRepository for InMemoryTermBridgeStateRepository {
    async fn load(&self) -> anyhow::Result<Option<TermBridgeState>> {
        Ok(self.state.read().unwrap().clone())
    }

    async fn save(&self, state: TermBridgeState) -> anyhow::Result<()> {
        *self.state.write().unwrap() = Some(state);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryTermBridgeBindingRepository {
    bindings: RwLock<HashMap<TerminalBindingId, TerminalBinding>>,
}

#[async_trait]
impl TerminalBindingRepository for InMemoryTermBridgeBindingRepository {
    async fn save(&self, binding: TerminalBinding) -> anyhow::Result<TerminalBinding> {
        self.bindings
            .write()
            .unwrap()
            .insert(binding.id.clone(), binding.clone());
        Ok(binding)
    }

    async fn get(&self, id: &TerminalBindingId) -> anyhow::Result<Option<TerminalBinding>> {
        Ok(self.bindings.read().unwrap().get(id).cloned())
    }

    async fn delete(&self, id: &TerminalBindingId) -> anyhow::Result<()> {
        self.bindings.write().unwrap().remove(id);
        Ok(())
    }

    async fn list(&self) -> anyhow::Result<Vec<TerminalBinding>> {
        Ok(self.bindings.read().unwrap().values().cloned().collect())
    }
}
