use crate::domain::termbridge::{
    CapabilityRecord, CurrentWorkingDirectory, TermBridgeState, TerminalBinding, TerminalBindingId,
};
use crate::ports::termbridge::{
    DuplicateOptions, SpawnRequest, TermBridgeCommandRequest, TermBridgeError,
    TermBridgeStateRepository, TerminalBindingRepository, TerminalControlPort,
};
use crate::telemetry::PrismMetrics;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};
use tokio::time::{timeout, Duration};

use async_trait::async_trait;

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
    #[error("termbridge overloaded for action={action}, terminal={terminal}")]
    Overloaded { action: String, terminal: String },
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

    fn overloaded(action: impl Into<String>, terminal: impl Into<String>) -> Self {
        Self::Overloaded {
            action: action.into(),
            terminal: terminal.into(),
        }
    }

    fn metric_reason(&self) -> String {
        match self {
            Self::Internal(msg) => msg.clone(),
            Self::NotFound(msg) => msg.clone(),
            Self::NotSupported { reason, .. } => reason.clone(),
            Self::Overloaded { .. } => "overloaded".to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermBridgeDiscoveryDiff {
    pub added: Vec<CapabilityRecord>,
    pub removed: Vec<CapabilityRecord>,
    pub updated: Vec<CapabilityRecord>,
}

impl TermBridgeDiscoveryDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }
}

pub struct TermBridgeDiscoveryOutcome {
    pub state: TermBridgeState,
    pub diff: TermBridgeDiscoveryDiff,
    pub changed: bool,
}

#[async_trait]
pub trait TermBridgeSyncPort: Send + Sync {
    async fn apply_external_snapshot(
        &self,
        source: &str,
        records: Vec<CapabilityRecord>,
    ) -> Result<TermBridgeDiscoveryOutcome, TermBridgeServiceError>;
}

#[derive(Clone)]
pub struct TermBridgeServiceConfig {
    pub max_inflight: usize,
    pub queue_timeout: Duration,
    pub snapshot_ttl: Duration,
}

impl Default for TermBridgeServiceConfig {
    fn default() -> Self {
        Self {
            max_inflight: 32,
            queue_timeout: Duration::from_secs(5),
            snapshot_ttl: Duration::from_secs(60),
        }
    }
}

impl TermBridgeServiceConfig {
    pub fn from_env() -> Self {
        use std::env;

        let max_inflight = env::var("SHELLDONE_TERMBRIDGE_MAX_INFLIGHT")
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(Self::default().max_inflight);

        let timeout_ms = env::var("SHELLDONE_TERMBRIDGE_QUEUE_TIMEOUT_MS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(5_000);

        let snapshot_ttl_ms = env::var("SHELLDONE_TERMBRIDGE_SNAPSHOT_TTL_MS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(60_000);

        Self {
            max_inflight,
            queue_timeout: Duration::from_millis(timeout_ms),
            snapshot_ttl: Duration::from_millis(snapshot_ttl_ms),
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
    cache_ts: Arc<RwLock<Option<std::time::Instant>>>,
    queue: Arc<Semaphore>,
    queue_timeout: Duration,
    snapshot_ttl: Duration,
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
        config: TermBridgeServiceConfig,
    ) -> Self {
        let permits = config.max_inflight.max(1);
        Self {
            state_repo,
            binding_repo,
            adapters,
            metrics,
            cache: Arc::new(RwLock::new(None)),
            cache_ts: Arc::new(RwLock::new(None)),
            queue: Arc::new(Semaphore::new(permits)),
            queue_timeout: config.queue_timeout,
            snapshot_ttl: config.snapshot_ttl,
        }
    }

    async fn run_with_backpressure<F, Fut, T>(
        &self,
        action: &str,
        terminal: &str,
        fut: F,
    ) -> Result<T, TermBridgeServiceError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, TermBridgeServiceError>>,
    {
        let permit = match timeout(self.queue_timeout, self.queue.clone().acquire_owned()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error(action, terminal, "semaphore closed");
                }
                return Err(TermBridgeServiceError::internal(
                    "termbridge semaphore closed",
                ));
            }
            Err(_) => {
                if let Some(metrics) = &self.metrics {
                    metrics.record_termbridge_error(action, terminal, "overloaded");
                }
                return Err(TermBridgeServiceError::overloaded(action, terminal));
            }
        };

        self.execute_with_metrics(action, terminal, permit, fut)
            .await
    }

    async fn execute_with_metrics<F, Fut, T>(
        &self,
        action: &str,
        terminal: &str,
        _permit: OwnedSemaphorePermit,
        fut: F,
    ) -> Result<T, TermBridgeServiceError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, TermBridgeServiceError>>,
    {
        let started = Instant::now();
        let result = fut().await;
        if let Some(metrics) = &self.metrics {
            let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
            match &result {
                Ok(_) => metrics.record_termbridge_action(action, terminal, latency_ms, "success"),
                Err(err) => metrics.record_termbridge_error(action, terminal, &err.metric_reason()),
            }
        }
        result
    }

    pub async fn discover(
        &self,
        source: &str,
    ) -> Result<TermBridgeDiscoveryOutcome, TermBridgeServiceError> {
        let started = Instant::now();
        let mut tasks = FuturesUnordered::new();
        for adapter in &self.adapters {
            let adapter = Arc::clone(adapter);
            tasks.push(async move {
                let terminal = adapter.terminal_id();
                let observation = adapter.detect().await;
                (terminal, observation)
            });
        }

        let mut observations = Vec::with_capacity(self.adapters.len());
        while let Some(entry) = tasks.next().await {
            observations.push(entry);
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
        self.persist_snapshot(source, started, records).await
    }

    #[allow(dead_code)]
    pub async fn apply_external_snapshot(
        &self,
        source: &str,
        records: Vec<CapabilityRecord>,
    ) -> Result<TermBridgeDiscoveryOutcome, TermBridgeServiceError> {
        self.persist_snapshot(source, Instant::now(), records).await
    }
    pub async fn snapshot(&self) -> Result<TermBridgeState, TermBridgeServiceError> {
        if let Some(state) = self.cache.read().await.clone() {
            if !state.capabilities().is_empty() {
                let expired = if let Some(ts) = *self.cache_ts.read().await {
                    ts.elapsed() > self.snapshot_ttl
                } else {
                    true
                };
                if !expired {
                    return Ok(state);
                }
            }
        }

        // On cache miss or TTL expiry prefer live discovery for freshness
        self.discover("snapshot_ttl_expired")
            .await
            .map(|outcome| outcome.state)
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
        let adapter = self
            .find_adapter(request.terminal.as_str())
            .ok_or_else(|| {
                TermBridgeServiceError::internal(format!(
                    "adapter for terminal {} not found",
                    request.terminal
                ))
            })?;

        let terminal = request.terminal.clone();
        let repo = Arc::clone(&self.binding_repo);

        self.run_with_backpressure("spawn", terminal.as_str(), move || {
            let adapter = Arc::clone(&adapter);
            let repo = Arc::clone(&repo);
            let request = request.clone();
            async move {
                let binding = adapter.spawn(&request).await.map_err(|err| match err {
                    TermBridgeError::NotSupported {
                        terminal,
                        action,
                        reason,
                    } => TermBridgeServiceError::not_supported(terminal.as_str(), action, reason),
                    TermBridgeError::BindingNotFound { terminal } => {
                        TermBridgeServiceError::not_found(format!(
                            "binding for terminal {}",
                            terminal.as_str()
                        ))
                    }
                    TermBridgeError::Internal(err) => TermBridgeServiceError::internal(err),
                })?;

                repo.save(binding.clone())
                    .await
                    .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?;

                Ok(binding)
            }
        })
        .await
    }

    pub async fn duplicate(
        &self,
        binding_id: &TerminalBindingId,
        options: DuplicateOptions,
    ) -> Result<TerminalBinding, TermBridgeServiceError> {
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
                    "adapter for terminal {} not found",
                    binding.terminal
                ))
            })?;

        let terminal = binding.terminal.clone();
        let repo = Arc::clone(&self.binding_repo);

        self.run_with_backpressure("duplicate", terminal.as_str(), move || {
            let adapter = Arc::clone(&adapter);
            let repo = Arc::clone(&repo);
            let binding = binding.clone();
            let options = options.clone();
            async move {
                let new_binding =
                    adapter
                        .duplicate(&binding, &options)
                        .await
                        .map_err(|err| match err {
                            TermBridgeError::NotSupported {
                                terminal,
                                action,
                                reason,
                            } => TermBridgeServiceError::not_supported(
                                terminal.as_str(),
                                action,
                                reason,
                            ),
                            TermBridgeError::BindingNotFound { terminal } => {
                                TermBridgeServiceError::not_found(format!(
                                    "binding for terminal {}",
                                    terminal.as_str()
                                ))
                            }
                            TermBridgeError::Internal(err) => TermBridgeServiceError::internal(err),
                        })?;

                repo.save(new_binding.clone())
                    .await
                    .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?;

                Ok(new_binding)
            }
        })
        .await
    }

    pub async fn send_text(
        &self,
        request: TermBridgeCommandRequest,
    ) -> Result<(), TermBridgeServiceError> {
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

        let action_terminal = binding.terminal.clone();
        let adapter = Arc::clone(&adapter);

        self.run_with_backpressure("send_text", action_terminal.as_str(), move || {
            let adapter = Arc::clone(&adapter);
            let binding = binding.clone();
            let payload = payload.clone();
            async move {
                adapter
                    .send_text(&binding, &payload, bracketed)
                    .await
                    .map_err(|err| match err {
                        TermBridgeError::NotSupported {
                            terminal,
                            action,
                            reason,
                        } => {
                            TermBridgeServiceError::not_supported(terminal.as_str(), action, reason)
                        }
                        TermBridgeError::BindingNotFound { terminal } => {
                            TermBridgeServiceError::not_found(format!(
                                "binding for terminal {}",
                                terminal.as_str()
                            ))
                        }
                        TermBridgeError::Internal(err) => TermBridgeServiceError::internal(err),
                    })
            }
        })
        .await
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

        let terminal = binding.terminal.clone();
        let adapter = Arc::clone(&adapter);

        self.run_with_backpressure("focus", terminal.as_str(), move || {
            let adapter = Arc::clone(&adapter);
            let binding = binding.clone();
            async move {
                adapter.focus(&binding).await.map_err(|err| match err {
                    TermBridgeError::NotSupported {
                        terminal,
                        action,
                        reason,
                    } => TermBridgeServiceError::not_supported(terminal.as_str(), action, reason),
                    TermBridgeError::BindingNotFound { terminal } => {
                        TermBridgeServiceError::not_found(format!(
                            "binding for terminal {}",
                            terminal.as_str()
                        ))
                    }
                    TermBridgeError::Internal(err) => TermBridgeServiceError::internal(err),
                })
            }
        })
        .await
    }

    pub async fn close(
        &self,
        binding_id: &TerminalBindingId,
    ) -> Result<TerminalBinding, TermBridgeServiceError> {
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

        let terminal = binding.terminal.clone();
        let repo = Arc::clone(&self.binding_repo);

        self.run_with_backpressure("close", terminal.as_str(), move || {
            let adapter = Arc::clone(&adapter);
            let repo = Arc::clone(&repo);
            let binding = binding.clone();
            async move {
                adapter.close(&binding).await.map_err(|err| match err {
                    TermBridgeError::NotSupported {
                        terminal,
                        action,
                        reason,
                    } => TermBridgeServiceError::not_supported(terminal.as_str(), action, reason),
                    TermBridgeError::BindingNotFound { terminal } => {
                        TermBridgeServiceError::not_found(format!(
                            "binding for terminal {}",
                            terminal.as_str()
                        ))
                    }
                    TermBridgeError::Internal(err) => TermBridgeServiceError::internal(err),
                })?;

                repo.delete(&binding.id)
                    .await
                    .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?;

                Ok(binding)
            }
        })
        .await
    }

    fn find_adapter(&self, terminal: impl AsRef<str>) -> Option<Arc<dyn TerminalControlPort>> {
        let terminal = terminal.as_ref();
        self.adapters
            .iter()
            .find(|adapter| adapter.terminal_id().as_str() == terminal)
            .cloned()
    }

    fn compute_discovery_diff(
        previous: Option<&TermBridgeState>,
        current: &TermBridgeState,
    ) -> TermBridgeDiscoveryDiff {
        use std::collections::BTreeMap;

        let mut diff = TermBridgeDiscoveryDiff::default();
        let prev_records = previous
            .map(|state| state.capabilities())
            .unwrap_or_default();
        let curr_records = current.capabilities();

        let prev_map: BTreeMap<String, CapabilityRecord> = prev_records
            .into_iter()
            .map(|record| (record.terminal.as_str().to_string(), record))
            .collect();
        let curr_map: BTreeMap<String, CapabilityRecord> = curr_records
            .into_iter()
            .map(|record| (record.terminal.as_str().to_string(), record))
            .collect();

        for (terminal, record) in &curr_map {
            match prev_map.get(terminal) {
                None => diff.added.push(record.clone()),
                Some(prev_record) => {
                    if prev_record != record {
                        diff.updated.push(record.clone());
                    }
                }
            }
        }

        for (terminal, record) in &prev_map {
            if !curr_map.contains_key(terminal) {
                diff.removed.push(record.clone());
            }
        }

        diff.added
            .sort_by(|a, b| a.terminal.as_str().cmp(b.terminal.as_str()));
        diff.updated
            .sort_by(|a, b| a.terminal.as_str().cmp(b.terminal.as_str()));
        diff.removed
            .sort_by(|a, b| a.terminal.as_str().cmp(b.terminal.as_str()));

        diff
    }
}

impl<R, B> TermBridgeService<R, B>
where
    R: TermBridgeStateRepository + 'static,
    B: TerminalBindingRepository + 'static,
{
    async fn persist_snapshot(
        &self,
        source: &str,
        started: Instant,
        records: Vec<CapabilityRecord>,
    ) -> Result<TermBridgeDiscoveryOutcome, TermBridgeServiceError> {
        let previous_state = self
            .state_repo
            .load()
            .await
            .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?;

        let mut next_state = previous_state.clone().unwrap_or_else(TermBridgeState::new);
        let changed = next_state.update_capabilities(records);
        let diff = Self::compute_discovery_diff(previous_state.as_ref(), &next_state);

        if changed {
            self.state_repo
                .save(next_state.clone())
                .await
                .map_err(|e| TermBridgeServiceError::internal(e.to_string()))?;
            let mut cache = self.cache.write().await;
            *cache = Some(next_state.clone());
            let mut ts = self.cache_ts.write().await;
            *ts = Some(Instant::now());
        } else if previous_state.is_some() {
            let mut cache = self.cache.write().await;
            if cache.is_none() {
                *cache = previous_state.clone();
                let mut ts = self.cache_ts.write().await;
                *ts = Some(Instant::now());
            }
        } else {
            let mut cache = self.cache.write().await;
            if cache.is_none() {
                *cache = Some(next_state.clone());
                let mut ts = self.cache_ts.write().await;
                *ts = Some(Instant::now());
            }
        }

        if let Some(metrics) = &self.metrics {
            let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
            let outcome = if changed || !diff.is_empty() {
                "changed"
            } else {
                "noop"
            };
            metrics.record_termbridge_action("discover", source, latency_ms, outcome);
            if changed || !diff.is_empty() {
                for added in &diff.added {
                    metrics.record_termbridge_capability_update(
                        added.terminal.as_str(),
                        source,
                        "added",
                    );
                }
                for updated in &diff.updated {
                    metrics.record_termbridge_capability_update(
                        updated.terminal.as_str(),
                        source,
                        "updated",
                    );
                }
                for removed in &diff.removed {
                    metrics.record_termbridge_capability_update(
                        removed.terminal.as_str(),
                        source,
                        "removed",
                    );
                }
            }
        }

        let state = if changed {
            next_state
        } else if let Some(previous) = previous_state {
            previous
        } else {
            next_state
        };

        Ok(TermBridgeDiscoveryOutcome {
            state,
            diff,
            changed,
        })
    }
}

#[async_trait]
impl<R, B> TermBridgeSyncPort for TermBridgeService<R, B>
where
    R: TermBridgeStateRepository + 'static,
    B: TerminalBindingRepository + 'static,
{
    async fn apply_external_snapshot(
        &self,
        source: &str,
        records: Vec<CapabilityRecord>,
    ) -> Result<TermBridgeDiscoveryOutcome, TermBridgeServiceError> {
        self.persist_snapshot(source, Instant::now(), records).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::termbridge::{
        InMemoryTermBridgeBindingRepository, InMemoryTermBridgeStateRepository,
    };
    use crate::domain::termbridge::{
        CurrentWorkingDirectory, TerminalBindingId, TerminalCapabilities, TerminalId,
    };
    use crate::ports::termbridge::{CapabilityObservation, DuplicateOptions, TermBridgeError};
    use async_trait::async_trait;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::{Arc, Mutex};
    use tokio::sync::Notify;
    use tokio::time::{sleep, Duration};

    struct StubAdapter {
        terminal: &'static str,
        spawn_binding: TerminalBinding,
        send_log: Arc<Mutex<Vec<String>>>,
        send_error: Option<(TerminalId, String, String)>,
        focus_error: Option<(TerminalId, String, String)>,
        duplicate_binding: Option<TerminalBinding>,
        duplicate_error: Option<(TerminalId, String, String)>,
        close_error: Option<(TerminalId, String, String)>,
        closed_ids: Arc<Mutex<Vec<TerminalBindingId>>>,
    }

    impl StubAdapter {
        fn new(terminal: &'static str, spawn_binding: TerminalBinding) -> Self {
            Self {
                terminal,
                spawn_binding,
                send_log: Arc::new(Mutex::new(Vec::new())),
                send_error: None,
                focus_error: None,
                duplicate_binding: None,
                duplicate_error: None,
                close_error: None,
                closed_ids: Arc::new(Mutex::new(Vec::new())),
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

        fn with_duplicate_result(mut self, binding: TerminalBinding) -> Self {
            self.duplicate_binding = Some(binding);
            self
        }

        fn with_duplicate_not_supported(
            mut self,
            terminal: TerminalId,
            action: impl Into<String>,
            reason: impl Into<String>,
        ) -> Self {
            self.duplicate_error = Some((terminal, action.into(), reason.into()));
            self
        }

        fn with_close_not_supported(
            mut self,
            terminal: TerminalId,
            action: impl Into<String>,
            reason: impl Into<String>,
        ) -> Self {
            self.close_error = Some((terminal, action.into(), reason.into()));
            self
        }

        fn closed_ids(&self) -> Arc<Mutex<Vec<TerminalBindingId>>> {
            Arc::clone(&self.closed_ids)
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

        async fn duplicate(
            &self,
            _binding: &TerminalBinding,
            _options: &DuplicateOptions,
        ) -> Result<TerminalBinding, TermBridgeError> {
            if let Some((terminal, action, reason)) = &self.duplicate_error {
                return Err(TermBridgeError::not_supported(
                    terminal.clone(),
                    action.clone(),
                    reason.clone(),
                ));
            }
            Ok(self
                .duplicate_binding
                .clone()
                .unwrap_or_else(|| self.spawn_binding.clone()))
        }

        async fn close(&self, binding: &TerminalBinding) -> Result<(), TermBridgeError> {
            if let Some((terminal, action, reason)) = &self.close_error {
                return Err(TermBridgeError::not_supported(
                    terminal.clone(),
                    action.clone(),
                    reason.clone(),
                ));
            }
            self.closed_ids.lock().unwrap().push(binding.id.clone());
            Ok(())
        }
    }

    #[derive(Clone)]
    struct BlockingGate {
        enabled: bool,
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    impl BlockingGate {
        fn new(enabled: bool) -> Self {
            Self {
                enabled,
                started: Arc::new(Notify::new()),
                release: Arc::new(Notify::new()),
            }
        }

        async fn wait(&self) {
            if self.enabled {
                self.started.notify_one();
                self.release.notified().await;
            }
        }

        fn started(&self) -> Arc<Notify> {
            Arc::clone(&self.started)
        }

        fn release(&self) -> Arc<Notify> {
            Arc::clone(&self.release)
        }
    }

    struct BlockingAdapter {
        terminal: &'static str,
        spawn_binding: TerminalBinding,
        detect_gate: BlockingGate,
        spawn_gate: BlockingGate,
        send_gate: BlockingGate,
        focus_gate: BlockingGate,
    }

    impl BlockingAdapter {
        fn new(
            terminal: &'static str,
            spawn_binding: TerminalBinding,
            block_spawn: bool,
            block_send: bool,
            block_focus: bool,
        ) -> Self {
            Self {
                terminal,
                spawn_binding,
                detect_gate: BlockingGate::new(false),
                spawn_gate: BlockingGate::new(block_spawn),
                send_gate: BlockingGate::new(block_send),
                focus_gate: BlockingGate::new(block_focus),
            }
        }

        fn enable_detect_blocking(&mut self) {
            self.detect_gate = BlockingGate::new(true);
        }

        fn detect_started(&self) -> Arc<Notify> {
            self.detect_gate.started()
        }

        fn detect_release(&self) -> Arc<Notify> {
            self.detect_gate.release()
        }

        fn spawn_started(&self) -> Arc<Notify> {
            self.spawn_gate.started()
        }

        fn spawn_release(&self) -> Arc<Notify> {
            self.spawn_gate.release()
        }

        fn send_started(&self) -> Arc<Notify> {
            self.send_gate.started()
        }

        fn send_release(&self) -> Arc<Notify> {
            self.send_gate.release()
        }

        fn focus_started(&self) -> Arc<Notify> {
            self.focus_gate.started()
        }

        fn focus_release(&self) -> Arc<Notify> {
            self.focus_gate.release()
        }
    }

    #[async_trait]
    impl TerminalControlPort for BlockingAdapter {
        fn terminal_id(&self) -> TerminalId {
            TerminalId::new(self.terminal)
        }

        async fn detect(&self) -> CapabilityObservation {
            self.detect_gate.wait().await;
            CapabilityObservation::new(
                self.terminal,
                TerminalCapabilities::builder()
                    .spawn(true)
                    .send_text(true)
                    .focus(true)
                    .build(),
                false,
                Vec::new(),
            )
        }

        async fn spawn(&self, _request: &SpawnRequest) -> Result<TerminalBinding, TermBridgeError> {
            self.spawn_gate.wait().await;
            Ok(self.spawn_binding.clone())
        }

        async fn send_text(
            &self,
            _binding: &TerminalBinding,
            _payload: &str,
            _as_bracketed: bool,
        ) -> Result<(), TermBridgeError> {
            self.send_gate.wait().await;
            Ok(())
        }

        async fn focus(&self, _binding: &TerminalBinding) -> Result<(), TermBridgeError> {
            self.focus_gate.wait().await;
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
        let service = TermBridgeService::new(
            state_repo,
            binding_repo.clone(),
            vec![adapter],
            None,
            TermBridgeServiceConfig::default(),
        );

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
    async fn duplicate_creates_binding_and_persists() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());

        let mut base_labels = HashMap::new();
        base_labels.insert("pane_id".into(), "1".into());
        let existing_binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "1",
            base_labels,
            Some("wezterm://pane/1".into()),
        );
        let existing_id = existing_binding.id.clone();
        binding_repo.save(existing_binding.clone()).await.unwrap();

        let mut dup_labels = HashMap::new();
        dup_labels.insert("pane_id".into(), "2".into());
        let duplicate_binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "2",
            dup_labels,
            Some("wezterm://pane/2".into()),
        );

        let adapter = Arc::new(
            StubAdapter::new("wezterm", existing_binding.clone())
                .with_duplicate_result(duplicate_binding.clone()),
        );
        let service = TermBridgeService::new(
            state_repo,
            binding_repo.clone(),
            vec![adapter],
            None,
            TermBridgeServiceConfig::default(),
        );

        let created = service
            .duplicate(&existing_id, DuplicateOptions::default())
            .await
            .expect("duplicate succeeds");

        assert_eq!(created.token, duplicate_binding.token);
        assert_ne!(created.id, existing_id);
        assert_eq!(created.terminal.as_str(), "wezterm");

        let stored_existing = binding_repo.get(&existing_id).await.unwrap();
        assert!(stored_existing.is_some());
        let stored_new = binding_repo.get(&created.id).await.unwrap();
        assert!(stored_new.is_some());
    }

    #[tokio::test]
    async fn duplicate_propagates_not_supported_error() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());

        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "5".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "5",
            labels,
            Some("wezterm://pane/5".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();

        let adapter = Arc::new(
            StubAdapter::new("wezterm", binding.clone()).with_duplicate_not_supported(
                TerminalId::new("wezterm"),
                "duplicate",
                "unsupported",
            ),
        );
        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![adapter],
            None,
            TermBridgeServiceConfig::default(),
        );

        let err = service
            .duplicate(&binding_id, DuplicateOptions::default())
            .await
            .expect_err("duplicate should fail");
        assert!(matches!(err, TermBridgeServiceError::NotSupported { .. }));
    }

    #[tokio::test]
    async fn close_removes_binding_and_records_adapter_call() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "8".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "8",
            labels,
            Some("wezterm://pane/8".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();

        let adapter = Arc::new(StubAdapter::new("wezterm", binding.clone()));
        let closed_log = adapter.closed_ids();

        let service = TermBridgeService::new(
            state_repo,
            binding_repo.clone(),
            vec![adapter],
            None,
            TermBridgeServiceConfig::default(),
        );

        let closed = service.close(&binding_id).await.expect("close succeeds");
        assert_eq!(closed.id, binding_id);
        assert!(binding_repo.get(&binding_id).await.unwrap().is_none());

        let log = closed_log.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0], binding_id);
    }

    #[tokio::test]
    async fn close_propagates_not_supported_error() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "9".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "9",
            labels,
            Some("wezterm://pane/9".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();

        let adapter = Arc::new(
            StubAdapter::new("wezterm", binding.clone()).with_close_not_supported(
                TerminalId::new("wezterm"),
                "close",
                "unsupported",
            ),
        );

        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![adapter],
            None,
            TermBridgeServiceConfig::default(),
        );

        let err = service
            .close(&binding_id)
            .await
            .expect_err("close should fail");
        assert!(matches!(err, TermBridgeServiceError::NotSupported { .. }));
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

        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![adapter],
            None,
            TermBridgeServiceConfig::default(),
        );

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

        let service = TermBridgeService::new(
            state_repo,
            binding_repo.clone(),
            Vec::new(),
            None,
            TermBridgeServiceConfig::default(),
        );
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
        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            Vec::new(),
            None,
            TermBridgeServiceConfig::default(),
        );
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
        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(adapter)],
            None,
            TermBridgeServiceConfig::default(),
        );

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
        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(adapter)],
            None,
            TermBridgeServiceConfig::default(),
        );
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
        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(adapter)],
            None,
            TermBridgeServiceConfig::default(),
        );
        let err = service
            .focus(&binding_id)
            .await
            .expect_err("expected not supported error");
        assert!(matches!(err, TermBridgeServiceError::NotSupported { .. }));
    }

    struct DetectionAdapter {
        terminal: &'static str,
        display_name: &'static str,
        requires_opt_in: bool,
        supports_spawn: bool,
    }

    #[async_trait]
    impl TerminalControlPort for DetectionAdapter {
        fn terminal_id(&self) -> TerminalId {
            TerminalId::new(self.terminal)
        }

        async fn detect(&self) -> CapabilityObservation {
            CapabilityObservation::new(
                self.display_name,
                TerminalCapabilities::builder()
                    .spawn(self.supports_spawn)
                    .send_text(true)
                    .build(),
                self.requires_opt_in,
                Vec::new(),
            )
        }
    }

    #[tokio::test]
    async fn discover_diff_detects_added_and_removed_terminals() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());

        let service = TermBridgeService::new(
            state_repo.clone(),
            binding_repo.clone(),
            vec![
                Arc::new(DetectionAdapter {
                    terminal: "wezterm",
                    display_name: "WezTerm",
                    requires_opt_in: false,
                    supports_spawn: true,
                }),
                Arc::new(DetectionAdapter {
                    terminal: "kitty",
                    display_name: "Kitty",
                    requires_opt_in: false,
                    supports_spawn: true,
                }),
            ],
            None,
            TermBridgeServiceConfig::default(),
        );

        let outcome = service.discover("bootstrap").await.unwrap();
        assert_eq!(outcome.diff.added.len(), 2);
        assert!(outcome.diff.removed.is_empty());
        assert!(outcome.diff.updated.is_empty());

        let service_without_kitty = TermBridgeService::new(
            state_repo.clone(),
            binding_repo,
            vec![Arc::new(DetectionAdapter {
                terminal: "wezterm",
                display_name: "WezTerm",
                requires_opt_in: false,
                supports_spawn: true,
            })],
            None,
            TermBridgeServiceConfig::default(),
        );

        let outcome = service_without_kitty.discover("scheduled").await.unwrap();
        assert_eq!(outcome.diff.removed.len(), 1);
        assert_eq!(outcome.diff.removed[0].terminal.as_str(), "kitty");
        assert!(outcome.diff.added.is_empty());
        assert!(outcome.diff.updated.is_empty());
    }

    #[tokio::test]
    async fn discover_diff_detects_capability_updates() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());

        let initial_service = TermBridgeService::new(
            state_repo.clone(),
            binding_repo.clone(),
            vec![Arc::new(DetectionAdapter {
                terminal: "wezterm",
                display_name: "WezTerm",
                requires_opt_in: false,
                supports_spawn: true,
            })],
            None,
            TermBridgeServiceConfig::default(),
        );
        initial_service.discover("bootstrap").await.unwrap();

        let updated_service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(DetectionAdapter {
                terminal: "wezterm",
                display_name: "WezTerm",
                requires_opt_in: true,
                supports_spawn: true,
            })],
            None,
            TermBridgeServiceConfig::default(),
        );
        let outcome = updated_service.discover("mcp").await.unwrap();
        assert!(outcome.diff.added.is_empty());
        assert!(outcome.diff.removed.is_empty());
        assert_eq!(outcome.diff.updated.len(), 1);
        assert_eq!(outcome.diff.updated[0].terminal.as_str(), "wezterm");
        assert!(outcome.changed);
    }

    #[tokio::test]
    async fn discover_without_changes_returns_cached_state() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let service = TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(DetectionAdapter {
                terminal: "wezterm",
                display_name: "WezTerm",
                requires_opt_in: false,
                supports_spawn: true,
            })],
            None,
            TermBridgeServiceConfig::default(),
        );

        let initial = service.discover("bootstrap").await.unwrap();
        assert!(initial.changed);
        assert!(!initial.diff.is_empty());

        let repeat = service.discover("scheduled").await.unwrap();
        assert!(!repeat.changed);
        assert!(repeat.diff.is_empty());
        assert_eq!(repeat.state.capabilities(), initial.state.capabilities());
    }

    #[tokio::test]
    async fn spawn_backpressure_returns_overloaded() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "1".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "1",
            labels,
            Some("wezterm://pane/1".into()),
        );
        let adapter = BlockingAdapter::new("wezterm", binding.clone(), true, false, false);
        let spawn_started = adapter.spawn_started();
        let spawn_release = adapter.spawn_release();
        let service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(adapter)],
            None,
            TermBridgeServiceConfig {
                max_inflight: 1,
                queue_timeout: Duration::from_millis(50),
                snapshot_ttl: Duration::from_secs(60),
            },
        ));

        let request = SpawnRequest {
            terminal: TerminalId::new("wezterm"),
            command: None,
            cwd: None,
            env: BTreeMap::new(),
        };

        let first_service = Arc::clone(&service);
        let first_request = request.clone();
        let first_task = tokio::spawn(async move { first_service.spawn(first_request).await });

        spawn_started.notified().await;

        let overloaded = service.spawn(request).await;

        spawn_release.notify_one();
        let first_result = first_task.await.expect("spawn join");

        assert!(matches!(
            overloaded,
            Err(TermBridgeServiceError::Overloaded { action, terminal })
                if action == "spawn" && terminal == "wezterm"
        ));
        assert!(first_result.is_ok());
    }

    #[tokio::test]
    async fn send_text_backpressure_returns_overloaded() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "2".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "2",
            labels,
            Some("wezterm://pane/2".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();
        let adapter = BlockingAdapter::new("wezterm", binding, false, true, false);
        let send_started = adapter.send_started();
        let send_release = adapter.send_release();
        let service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(adapter)],
            None,
            TermBridgeServiceConfig {
                max_inflight: 1,
                queue_timeout: Duration::from_millis(50),
                snapshot_ttl: Duration::from_secs(60),
            },
        ));

        let request = TermBridgeCommandRequest {
            binding_id: Some(binding_id.clone()),
            terminal: None,
            payload: Some("echo first".into()),
            bracketed_paste: Some(true),
        };

        let first_service = Arc::clone(&service);
        let first_req = request.clone();
        let first_task = tokio::spawn(async move { first_service.send_text(first_req).await });

        send_started.notified().await;

        let overloaded = service
            .send_text(TermBridgeCommandRequest {
                payload: Some("echo second".into()),
                ..request
            })
            .await;

        send_release.notify_one();
        let first_result = first_task.await.expect("send_text join");

        assert!(matches!(
            overloaded,
            Err(TermBridgeServiceError::Overloaded { action, terminal })
                if action == "send_text" && terminal == "wezterm"
        ));
        assert!(first_result.is_ok());
    }

    #[tokio::test]
    async fn focus_backpressure_returns_overloaded() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let mut labels = HashMap::new();
        labels.insert("pane_id".into(), "3".into());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "3",
            labels,
            Some("wezterm://pane/3".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding.clone()).await.unwrap();
        let adapter = BlockingAdapter::new("wezterm", binding, false, false, true);
        let focus_started = adapter.focus_started();
        let focus_release = adapter.focus_release();
        let service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(adapter)],
            None,
            TermBridgeServiceConfig {
                max_inflight: 1,
                queue_timeout: Duration::from_millis(50),
                snapshot_ttl: Duration::from_secs(60),
            },
        ));

        let first_service = Arc::clone(&service);
        let first_binding = binding_id.clone();
        let first_task = tokio::spawn(async move { first_service.focus(&first_binding).await });

        focus_started.notified().await;

        let overloaded = service.focus(&binding_id).await;

        focus_release.notify_one();
        let first_result = first_task.await.expect("focus join");

        assert!(matches!(
            overloaded,
            Err(TermBridgeServiceError::Overloaded { action, terminal })
                if action == "focus" && terminal == "wezterm"
        ));
        assert!(first_result.is_ok());
    }

    #[tokio::test]
    async fn discover_runs_detect_in_parallel() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());

        let adapter1_binding =
            TerminalBinding::new(TerminalId::new("wezterm"), "token-1", HashMap::new(), None);
        let adapter2_binding =
            TerminalBinding::new(TerminalId::new("kitty"), "token-2", HashMap::new(), None);

        let mut adapter1_inner =
            BlockingAdapter::new("wezterm", adapter1_binding, false, false, false);
        adapter1_inner.enable_detect_blocking();
        let adapter1 = Arc::new(adapter1_inner);

        let mut adapter2_inner =
            BlockingAdapter::new("kitty", adapter2_binding, false, false, false);
        adapter2_inner.enable_detect_blocking();
        let adapter2 = Arc::new(adapter2_inner);

        let service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![
                Arc::clone(&adapter1) as Arc<dyn TerminalControlPort>,
                Arc::clone(&adapter2) as Arc<dyn TerminalControlPort>,
            ],
            None,
            TermBridgeServiceConfig::default(),
        ));

        let detect1_started = adapter1.detect_started();
        let detect2_started = adapter2.detect_started();
        let detect1_release = adapter1.detect_release();
        let detect2_release = adapter2.detect_release();

        let discover_service = Arc::clone(&service);
        let discover_task =
            tokio::spawn(async move { discover_service.discover("parallel").await });

        timeout(Duration::from_millis(100), detect1_started.notified())
            .await
            .expect("first adapter detect should start");
        timeout(Duration::from_millis(100), detect2_started.notified())
            .await
            .expect("second adapter detect should start");

        detect1_release.notify_one();
        detect2_release.notify_one();

        let outcome = timeout(Duration::from_millis(200), discover_task)
            .await
            .expect("discover completes")
            .expect("discover join")
            .expect("discover result");

        assert_eq!(outcome.state.capabilities().len(), 2);
    }

    struct CountingDetectAdapter {
        terminal: TerminalId,
        counter: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl CountingDetectAdapter {
        fn new(name: &str, counter: Arc<std::sync::atomic::AtomicUsize>) -> Self {
            Self {
                terminal: TerminalId::new(name),
                counter,
            }
        }
    }

    #[async_trait]
    impl TerminalControlPort for CountingDetectAdapter {
        fn terminal_id(&self) -> TerminalId {
            self.terminal.clone()
        }
        async fn detect(&self) -> CapabilityObservation {
            use std::sync::atomic::Ordering;
            self.counter.fetch_add(1, Ordering::SeqCst);
            CapabilityObservation::new(
                self.terminal.as_str(),
                TerminalCapabilities::builder().spawn(true).build(),
                false,
                Vec::new(),
            )
        }
    }

    #[tokio::test]
    async fn snapshot_uses_cache_within_ttl_and_refreshes_after_ttl() {
        let state_repo = Arc::new(InMemoryTermBridgeStateRepository::default());
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let adapter = Arc::new(CountingDetectAdapter::new("wezterm", counter.clone()));
        let cfg = TermBridgeServiceConfig {
            snapshot_ttl: Duration::from_millis(50),
            ..TermBridgeServiceConfig::default()
        };
        let service = TermBridgeService::new(state_repo, binding_repo, vec![adapter], None, cfg);

        // First snapshot triggers detection and warms cache
        let _ = service.snapshot().await.unwrap();
        let first = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert!(first >= 1);

        // Within TTL — no additional detects
        let _ = service.snapshot().await.unwrap();
        let second = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(first, second);

        // After TTL — detect again
        sleep(Duration::from_millis(60)).await;
        let _ = service.snapshot().await.unwrap();
        let third = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert!(third > second);
    }
}
