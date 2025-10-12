use crate::domain::termbridge::{
    CapabilityRecord, CurrentWorkingDirectory, TermBridgeState, TerminalBinding, TerminalBindingId,
};
use crate::ports::termbridge::{
    SpawnRequest, TermBridgeCommandRequest, TermBridgeError, TermBridgeStateRepository,
    TerminalBindingRepository, TerminalControlPort,
};
use crate::telemetry::PrismMetrics;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum TermBridgeServiceError {
    #[error("internal error: {0}")]
    Internal(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("not supported: terminal={terminal}, action={action}, reason={reason}")]
    NotSupported {
        terminal: String,
        action: String,
        reason: String,
    },
}

impl TermBridgeServiceError {
    fn internal(err: impl Into<String>) -> Self {
        Self::Internal(err.into())
    }

    fn not_found(err: impl Into<String>) -> Self {
        Self::NotFound(err.into())
    }

    fn not_supported(
        terminal: impl Into<String>,
        action: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::NotSupported {
            terminal: terminal.into(),
            action: action.into(),
            reason: reason.into(),
        }
    }
}

pub struct TermBridgeService<R, B>
where
    R: TermBridgeStateRepository + 'static,
    B: TerminalBindingRepository + 'static,
{
    state_repo: Arc<R>,
    binding_repo: Arc<B>,
    adapters: Vec<Arc<dyn TerminalControlPort>>,
    metrics: Option<Arc<PrismMetrics>>,
    cache: Arc<RwLock<Option<TermBridgeState>>>,
}

impl<R, B> TermBridgeService<R, B>
where
    R: TermBridgeStateRepository + 'static,
    B: TerminalBindingRepository + 'static,
{
    pub fn new(
        state_repo: Arc<R>,
        binding_repo: Arc<B>,
        adapters: Vec<Arc<dyn TerminalControlPort>>,
        metrics: Option<Arc<PrismMetrics>>,
    ) -> Self {
        Self {
            state_repo,
            binding_repo,
            adapters,
            metrics,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn discover(&self) -> Result<TermBridgeState, TermBridgeServiceError> {
        let started = Instant::now();
        let mut observations = Vec::with_capacity(self.adapters.len());
        for adapter in &self.adapters {
            let terminal = adapter.terminal_id();
            let observation = adapter.detect().await;
            observations.push((terminal, observation));
        }

        let records = observations
            .into_iter()
            .map(|(terminal, observation)| {
                CapabilityRecord::new(
                    terminal,
                    observation.display_name,
                    observation.requires_opt_in,
                    observation.capabilities,
                    observation.notes,
                )
            })
            .collect::<Vec<_>>();

        let mut state = TermBridgeState::new();
        let changed = state.update_capabilities(records);

        if changed {
            self.state_repo
                .save(state.clone())
                .await
                .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?;
            {
                let mut cache = self.cache.write().await;
                *cache = Some(state.clone());
            }
        }

        if let Some(metrics) = &self.metrics {
            let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
            metrics.record_termbridge_action("discover", "all", latency_ms, "success");
        }

        Ok(state)
    }

    pub async fn snapshot(&self) -> Result<TermBridgeState, TermBridgeServiceError> {
        if let Some(state) = self.cache.read().await.clone() {
            if !state.capabilities().is_empty() {
                return Ok(state);
            }
        }

        if let Some(state) = self
            .state_repo
            .load()
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?
        {
            {
                let mut cache = self.cache.write().await;
                *cache = Some(state.clone());
            }
            Ok(state)
        } else {
            self.discover().await
        }
    }

    pub async fn list_bindings(&self) -> Result<Vec<TerminalBinding>, TermBridgeServiceError> {
        self.binding_repo
            .list()
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))
    }

    pub async fn get_binding(
        &self,
        id: &TerminalBindingId,
    ) -> Result<Option<TerminalBinding>, TermBridgeServiceError> {
        self.binding_repo
            .get(id)
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))
    }

    pub async fn spawn(
        &self,
        request: SpawnRequest,
    ) -> Result<TerminalBinding, TermBridgeServiceError> {
        let started = Instant::now();
        let adapter = self
            .find_adapter(request.terminal.as_str())
            .ok_or_else(|| {
                TermBridgeServiceError::internal(format!(
                    "adapter for terminal {} not found",
                    request.terminal
                ))
            })?;

        let spawn_result = adapter.spawn(&request).await;
        let binding = match spawn_result {
            Ok(binding) => binding,
            Err(TermBridgeError::NotSupported {
                terminal,
                action,
                reason,
            }) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error("spawn", terminal.as_str(), &reason);
                }
                return Err(TermBridgeServiceError::not_supported(
                    terminal.as_str(),
                    action,
                    reason,
                ));
            }
            Err(TermBridgeError::BindingNotFound { terminal }) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error(
                        "spawn",
                        terminal.as_str(),
                        "binding not found",
                    );
                }
                return Err(TermBridgeServiceError::not_found(format!(
                    "binding for terminal {}",
                    terminal.as_str()
                )));
            }
            Err(TermBridgeError::Internal(err)) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error("spawn", request.terminal.as_str(), &err);
                }
                return Err(TermBridgeServiceError::internal(err));
            }
        };

        self.binding_repo
            .save(binding.clone())
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?;

        if let Some(metrics) = &self.metrics {
            let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
            metrics.record_termbridge_action(
                "spawn",
                request.terminal.as_str(),
                latency_ms,
                "accepted",
            );
        }

        Ok(binding)
    }

    pub async fn send_text(
        &self,
        request: TermBridgeCommandRequest,
    ) -> Result<(), TermBridgeServiceError> {
        let started = Instant::now();
        let binding_id = request.binding_id.ok_or_else(|| {
            TermBridgeServiceError::internal("binding_id is required for send_text")
        })?;

        let binding = self
            .binding_repo
            .get(&binding_id)
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?
            .ok_or_else(|| TermBridgeServiceError::not_found(format!("binding {}", binding_id)))?;

        let payload = request
            .payload
            .ok_or_else(|| TermBridgeServiceError::internal("payload is required"))?;
        let bracketed = request.bracketed_paste.unwrap_or(true);

        let adapter = self
            .find_adapter(binding.terminal.as_str())
            .ok_or_else(|| {
                TermBridgeServiceError::internal(format!(
                    "adapter for terminal {} not registered",
                    binding.terminal
                ))
            })?;

        match adapter.send_text(&binding, &payload, bracketed).await {
            Ok(()) => {
                if let Some(metrics) = &self.metrics {
                    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
                    metrics.record_termbridge_action(
                        "send_text",
                        binding.terminal.as_str(),
                        latency_ms,
                        "accepted",
                    );
                }
                Ok(())
            }
            Err(TermBridgeError::NotSupported {
                terminal,
                action,
                reason,
            }) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error("send_text", terminal.as_str(), &reason);
                }
                Err(TermBridgeServiceError::not_supported(
                    terminal.as_str(),
                    action,
                    reason,
                ))
            }
            Err(TermBridgeError::BindingNotFound { terminal }) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error(
                        "send_text",
                        terminal.as_str(),
                        "binding not found",
                    );
                }
                Err(TermBridgeServiceError::not_found(format!(
                    "binding for terminal {}",
                    terminal.as_str()
                )))
            }
            Err(TermBridgeError::Internal(err)) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error("send_text", binding.terminal.as_str(), &err);
                }
                Err(TermBridgeServiceError::internal(err))
            }
        }
    }

    pub async fn update_cwd(
        &self,
        id: &TerminalBindingId,
        cwd: CurrentWorkingDirectory,
    ) -> Result<TerminalBinding, TermBridgeServiceError> {
        let started = Instant::now();
        let mut binding = match self
            .binding_repo
            .get(id)
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?
        {
            Some(binding) => binding,
            None => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error(
                        "update_cwd",
                        "unknown",
                        &format!("binding {} not found", id),
                    );
                }
                return Err(TermBridgeServiceError::not_found(format!("binding {}", id)));
            }
        };

        let terminal = binding.terminal.to_string();
        binding.set_cwd(&cwd);

        let result = self.binding_repo.save(binding).await;

        match result {
            Ok(binding) => {
                if let Some(metrics) = &self.metrics {
                    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
                    metrics.record_termbridge_action(
                        "update_cwd",
                        terminal.as_str(),
                        latency_ms,
                        "accepted",
                    );
                }
                Ok(binding)
            }
            Err(err) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error(
                        "update_cwd",
                        terminal.as_str(),
                        &err.to_string(),
                    );
                }
                Err(TermBridgeServiceError::internal(err.to_string()))
            }
        }
    }

    pub async fn focus(
        &self,
        binding_id: &TerminalBindingId,
    ) -> Result<(), TermBridgeServiceError> {
        let started = Instant::now();
        let binding = self
            .binding_repo
            .get(binding_id)
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?
            .ok_or_else(|| TermBridgeServiceError::not_found(format!("binding {}", binding_id)))?;

        let adapter = self
            .find_adapter(binding.terminal.as_str())
            .ok_or_else(|| {
                TermBridgeServiceError::internal(format!(
                    "adapter for terminal {} not registered",
                    binding.terminal
                ))
            })?;

        match adapter.focus(&binding).await {
            Ok(()) => {
                if let Some(metrics) = &self.metrics {
                    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
                    metrics.record_termbridge_action(
                        "focus",
                        binding.terminal.as_str(),
                        latency_ms,
                        "accepted",
                    );
                }
                Ok(())
            }
            Err(TermBridgeError::NotSupported {
                terminal,
                action,
                reason,
            }) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error("focus", terminal.as_str(), &reason);
                }
                Err(TermBridgeServiceError::not_supported(
                    terminal.as_str(),
                    action,
                    reason,
                ))
            }
            Err(TermBridgeError::BindingNotFound { terminal }) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error(
                        "focus",
                        terminal.as_str(),
                        "binding not found",
                    );
                }
                Err(TermBridgeServiceError::not_found(format!(
                    "binding for terminal {}",
                    terminal.as_str()
                )))
            }
            Err(TermBridgeError::Internal(err)) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error("focus", binding.terminal.as_str(), &err);
                }
                Err(TermBridgeServiceError::internal(err))
            }
        }
    }

    fn find_adapter(&self, terminal: impl AsRef<str>) -> Option<Arc<dyn TerminalControlPort>> {
        let terminal = terminal.as_ref();
        self.adapters
            .iter()
            .find(|adapter| adapter.terminal_id().as_str() == terminal)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::termbridge::{
        InMemoryTermBridgeBindingRepository, InMemoryTermBridgeStateRepository,
    };
    use crate::domain::termbridge::{CurrentWorkingDirectory, TerminalCapabilities, TerminalId};
    use crate::ports::termbridge::{CapabilityObservation, TermBridgeError};
    use async_trait::async_trait;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::{Arc, Mutex};

    struct StubAdapter {
        terminal: &'static str,
        spawn_binding: TerminalBinding,
        send_log: Arc<Mutex<Vec<String>>>,
        send_error: Option<(TerminalId, String, String)>,
        focus_error: Option<(TerminalId, String, String)>,
    }

    impl StubAdapter {
        fn new(terminal: &'static str, spawn_binding: TerminalBinding) -> Self {
            Self {
                terminal,
                spawn_binding,
                send_log: Arc::new(Mutex::new(Vec::new())),
                send_error: None,
                focus_error: None,
            }
        }

        fn with_send_not_supported(
            mut self,
            terminal: TerminalId,
            action: impl Into<String>,
            reason: impl Into<String>,
        ) -> Self {
            self.send_error = Some((terminal, action.into(), reason.into()));
            self
        }

        fn with_focus_not_supported(
            mut self,
            terminal: TerminalId,
            action: impl Into<String>,
            reason: impl Into<String>,
        ) -> Self {
            self.focus_error = Some((terminal, action.into(), reason.into()));
            self
        }
    }

    #[async_trait]
    impl TerminalControlPort for StubAdapter {
        fn terminal_id(&self) -> TerminalId {
            TerminalId::new(self.terminal)
        }

        async fn detect(&self) -> CapabilityObservation {
            CapabilityObservation::new(
                self.terminal,
                TerminalCapabilities::builder()
                    .spawn(true)
                    .send_text(true)
                    .build(),
                false,
                Vec::new(),
            )
        }

        async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
            Ok(self.spawn_binding.clone())
        }

        async fn send_text(
            &self,
            _binding: &TerminalBinding,
            payload: &str,
            _as_bracketed: bool,
        ) -> Result<(), TermBridgeError> {
            if let Some((terminal, action, reason)) = &self.send_error {
                return Err(TermBridgeError::not_supported(
                    terminal.clone(),
                    action.clone(),
                    reason.clone(),
                ));
            }
            self.send_log.lock().unwrap().push(payload.to_string());
            Ok(())
        }

        async fn focus(&self, _binding: &TerminalBinding) -> Result<(), TermBridgeError> {
            if let Some((terminal, action, reason)) = &self.focus_error {
                return Err(TermBridgeError::not_supported(
                    terminal.clone(),
                    action.clone(),
                    reason.clone(),
                ));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn spawn_persists_binding() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "99".into());
        let spawn_binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "99",
            labels,
            Some("wezterm://pane/99".into()),
        );
        let adapter = Arc::new(StubAdapter::new("wezterm", spawn_binding.clone()));
        let service = TermBridgeService::new(state_repo, binding_repo.clone(), vec![adapter], None);

        let request = SpawnRequest {
            terminal: TerminalId::new("wezterm"),
            command: None,
            cwd: None,
            env: BTreeMap::new(),
        };

        let binding = service.spawn(request).await.unwrap();
        assert_eq!(binding.token, "99");

        let stored = binding_repo.list().await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, binding.id);
    }

    #[tokio::test]
    async fn send_text_delegates_to_adapter() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "7".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "7",
            labels,
            Some("wezterm://pane/7".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();

        let adapter = Arc::new(StubAdapter::new("wezterm", binding.clone()));
        let send_log = adapter.send_log.clone();

        let service = TermBridgeService::new(state_repo, binding_repo, vec![adapter], None);

        let request = TermBridgeCommandRequest {
            binding_id: Some(binding_id),
            terminal: None,
            payload: Some("echo hi".into()),
            bracketed_paste: Some(true),
        };

        service.send_text(request).await.unwrap();

        let log = send_log.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0], "echo hi");
    }

    #[tokio::test]
    async fn update_cwd_updates_binding_label() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let binding =
            TerminalBinding::new(TerminalId::new("wezterm"), "token", HashMap::new(), None);
        let binding_id = binding.id.clone();
        binding_repo.save(binding).await.unwrap();

        let service = TermBridgeService::new(state_repo, binding_repo.clone(), Vec::new(), None);
        let cwd = CurrentWorkingDirectory::new("/workspace").unwrap();
        let updated = service.update_cwd(&binding_id, cwd.clone()).await.unwrap();
        assert_eq!(updated.labels.get("cwd"), Some(&String::from(cwd.clone())));

        let persisted = binding_repo.get(&binding_id).await.unwrap().unwrap();
        assert_eq!(persisted.labels.get("cwd"), Some(&String::from(cwd)));
    }

    #[tokio::test]
    async fn update_cwd_returns_not_found_for_missing_binding() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let service = TermBridgeService::new(state_repo, binding_repo, Vec::new(), None);
        let missing_id = TerminalBindingId::new();
        let cwd = CurrentWorkingDirectory::new("/workspace").unwrap();
        let err = service
            .update_cwd(&missing_id, cwd)
            .await
            .expect_err("expected not found error");
        assert!(matches!(err, TermBridgeServiceError::NotFound(_)));
    }

    #[tokio::test]
    async fn send_text_propagates_not_supported() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "7".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "7",
            labels,
            Some("wezterm://pane/7".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();

        let adapter = StubAdapter::new("wezterm", binding.clone()).with_send_not_supported(
            TerminalId::new("wezterm"),
            "send_text",
            "not enabled",
        );
        let service =
            TermBridgeService::new(state_repo, binding_repo, vec![Arc::new(adapter)], None);

        let request = TermBridgeCommandRequest {
            binding_id: Some(binding_id),
            terminal: None,
            payload: Some("echo hi".into()),
            bracketed_paste: Some(true),
        };

        let err = service
            .send_text(request)
            .await
            .expect_err("expected not supported error");
        assert!(matches!(err, TermBridgeServiceError::NotSupported { .. }));
    }

    #[tokio::test]
    async fn focus_returns_not_found_when_binding_absent() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let adapter = StubAdapter::new(
            "wezterm",
            TerminalBinding::new(TerminalId::new("wezterm"), "7", HashMap::new(), None),
        );
        let service =
            TermBridgeService::new(state_repo, binding_repo, vec![Arc::new(adapter)], None);
        let err = service
            .focus(&TerminalBindingId::new())
            .await
            .expect_err("expected not found error");
        assert!(matches!(err, TermBridgeServiceError::NotFound(_)));
    }

    #[tokio::test]
    async fn focus_propagates_not_supported() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let binding =
            TerminalBinding::new(TerminalId::new("wezterm"), "token", HashMap::new(), None);
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();

        let adapter = StubAdapter::new("wezterm", binding.clone()).with_focus_not_supported(
            TerminalId::new("wezterm"),
            "focus",
            "not implemented",
        );
        let service =
            TermBridgeService::new(state_repo, binding_repo, vec![Arc::new(adapter)], None);
        let err = service
            .focus(&binding_id)
            .await
            .expect_err("expected not supported error");
        assert!(matches!(err, TermBridgeServiceError::NotSupported { .. }));
    }
}
