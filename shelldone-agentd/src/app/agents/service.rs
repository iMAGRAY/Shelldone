use crate::domain::agents::{
    AgentBinding, AgentBindingId, AgentEventEnvelope, AgentProvider, BindingStatus, CapabilityName,
    CapabilitySet, SdkChannel, SdkVersion,
};
use crate::ports::agents::AgentBindingRepository;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentServiceError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AgentServiceResult<T> = Result<T, AgentServiceError>;

pub struct AgentBindingService<R: AgentBindingRepository> {
    repository: Arc<R>,
}

impl<R: AgentBindingRepository> AgentBindingService<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    pub async fn register_binding(
        &self,
        provider: AgentProvider,
        sdk_version: SdkVersion,
        channel: SdkChannel,
        capability_names: Vec<CapabilityName>,
    ) -> AgentServiceResult<(AgentBinding, AgentEventEnvelope)> {
        let capabilities = CapabilitySet::new(capability_names)
            .map_err(|e| AgentServiceError::Invalid(e.to_string()))?;
        let (binding, event) = AgentBinding::register(provider, sdk_version, channel, capabilities)
            .map_err(AgentServiceError::Invalid)?;
        self.repository
            .save(binding.clone())
            .await
            .map_err(|e| AgentServiceError::Internal(e.to_string()))?;
        Ok((binding, event))
    }

    pub async fn activate_binding(
        &self,
        id: &AgentBindingId,
    ) -> AgentServiceResult<AgentEventEnvelope> {
        let mut binding = self
            .load_binding(id)
            .await
            .ok_or_else(|| AgentServiceError::NotFound(id.to_string()))?;
        let event = binding.activate().map_err(AgentServiceError::Invalid)?;
        self.persist(binding).await?;
        Ok(event)
    }

    pub async fn deactivate_binding(
        &self,
        id: &AgentBindingId,
    ) -> AgentServiceResult<AgentEventEnvelope> {
        let mut binding = self
            .load_binding(id)
            .await
            .ok_or_else(|| AgentServiceError::NotFound(id.to_string()))?;
        let event = binding.deactivate().map_err(AgentServiceError::Invalid)?;
        self.persist(binding).await?;
        Ok(event)
    }

    pub async fn record_heartbeat(
        &self,
        id: &AgentBindingId,
    ) -> AgentServiceResult<AgentEventEnvelope> {
        let mut binding = self
            .load_binding(id)
            .await
            .ok_or_else(|| AgentServiceError::NotFound(id.to_string()))?;
        let event = binding
            .record_heartbeat()
            .map_err(AgentServiceError::Invalid)?;
        self.persist(binding).await?;
        Ok(event)
    }

    pub async fn set_capabilities(
        &self,
        id: &AgentBindingId,
        capability_names: Vec<CapabilityName>,
    ) -> AgentServiceResult<AgentEventEnvelope> {
        let mut binding = self
            .load_binding(id)
            .await
            .ok_or_else(|| AgentServiceError::NotFound(id.to_string()))?;
        let capabilities = CapabilitySet::new(capability_names)
            .map_err(|e| AgentServiceError::Invalid(e.to_string()))?;
        let event = binding
            .update_capabilities(capabilities)
            .map_err(AgentServiceError::Invalid)?;
        self.persist(binding).await?;
        Ok(event)
    }

    pub async fn list_bindings(&self) -> AgentServiceResult<Vec<AgentBinding>> {
        self.repository
            .list()
            .await
            .map_err(|e| AgentServiceError::Internal(e.to_string()))
    }

    pub async fn list_active(&self) -> AgentServiceResult<Vec<AgentBinding>> {
        let bindings = self.list_bindings().await?;
        Ok(bindings
            .into_iter()
            .filter(|binding| matches!(binding.status(), BindingStatus::Active))
            .collect())
    }

    pub async fn remove_binding(&self, id: &AgentBindingId) -> AgentServiceResult<()> {
        self.repository
            .delete(id)
            .await
            .map_err(|e| AgentServiceError::Internal(e.to_string()))
    }

    async fn load_binding(&self, id: &AgentBindingId) -> Option<AgentBinding> {
        self.repository
            .get(id)
            .await
            .ok()
            .and_then(|binding| binding)
    }

    async fn persist(&self, binding: AgentBinding) -> AgentServiceResult<()> {
        self.repository
            .save(binding)
            .await
            .map_err(|e| AgentServiceError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::agents::InMemoryAgentBindingRepository;
    use crate::domain::agents::events::AgentDomainEvent;

    fn capability(name: &str) -> CapabilityName {
        CapabilityName::new(name).unwrap()
    }

    #[tokio::test]
    async fn register_and_activate_binding() {
        let repo = Arc::new(InMemoryAgentBindingRepository::new());
        let service = AgentBindingService::new(repo.clone());
        let (binding, _event) = service
            .register_binding(
                AgentProvider::Microsoft,
                SdkVersion::new("1.4.0").unwrap(),
                SdkChannel::Preview,
                vec![capability("fs.read"), capability("shell.exec")],
            )
            .await
            .unwrap();
        let id = binding.id();
        assert!(matches!(binding.status(), BindingStatus::Registered));
        let _activate = service.activate_binding(&id).await.unwrap();
        let active = service.list_active().await.unwrap();
        assert_eq!(active.len(), 1);
        let heartbeat = service.record_heartbeat(&id).await.unwrap();
        assert!(matches!(
            heartbeat.event,
            AgentDomainEvent::HeartbeatObserved { .. }
        ));
    }

    #[tokio::test]
    async fn deactivate_and_update_capabilities() {
        let repo = Arc::new(InMemoryAgentBindingRepository::new());
        let service = AgentBindingService::new(repo.clone());
        let (binding, _) = service
            .register_binding(
                AgentProvider::OpenAi,
                SdkVersion::new("3.0.0").unwrap(),
                SdkChannel::Stable,
                vec![capability("agent.exec")],
            )
            .await
            .unwrap();
        let id = binding.id();
        service.activate_binding(&id).await.unwrap();
        service
            .set_capabilities(&id, vec![capability("agent.exec"), capability("fs.read")])
            .await
            .unwrap();
        service.deactivate_binding(&id).await.unwrap();
        assert!(service.record_heartbeat(&id).await.is_err());
    }
}
