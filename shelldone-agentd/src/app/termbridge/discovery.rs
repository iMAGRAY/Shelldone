use crate::telemetry::PrismMetrics;
use crate::TermBridgeServiceType;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{self, Duration, MissedTickBehavior};
use tracing::{debug, error, info, warn};

const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);
const CHANNEL_CAPACITY: usize = 16;

#[derive(Clone)]
pub struct TermBridgeDiscoveryHandle {
    tx: mpsc::Sender<DiscoveryCommand>,
}

impl TermBridgeDiscoveryHandle {
    pub fn notify_refresh(&self, reason: &'static str) {
        let tx = self.tx.clone();
        // Fire-and-forget; best effort to avoid queue saturation.
        if let Err(err) = tx.try_send(DiscoveryCommand::Trigger(reason)) {
            debug!(%reason, %err, "termbridge discovery trigger queue full");
        }
    }

    #[allow(dead_code)]
    pub async fn shutdown(&self) {
        let _ = self.tx.send(DiscoveryCommand::Shutdown).await;
    }
}

enum DiscoveryCommand {
    Trigger(&'static str),
    #[allow(dead_code)]
    Shutdown,
}

pub fn spawn_discovery_task(
    service: Arc<TermBridgeServiceType>,
    metrics: Option<Arc<PrismMetrics>>,
    interval: Option<Duration>,
) -> TermBridgeDiscoveryHandle {
    let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
    let handle = TermBridgeDiscoveryHandle { tx: tx.clone() };
    let mut ticker = time::interval(interval.unwrap_or(DEFAULT_INTERVAL));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let service_ref = Arc::clone(&service);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                Some(cmd) = rx.recv() => {
                    match cmd {
                        DiscoveryCommand::Trigger(reason) => {
                            if let Err(err) = run_discovery(Arc::clone(&service_ref), metrics.as_ref(), Some(reason)).await {
                                warn!(%reason, %err, "termbridge discovery trigger failed");
                            }
                        }
                        DiscoveryCommand::Shutdown => {
                            info!("termbridge discovery worker received shutdown signal");
                            break;
                        }
                    }
                }
                _ = ticker.tick() => {
                    if let Err(err) = run_discovery(Arc::clone(&service_ref), metrics.as_ref(), None).await {
                        warn!(%err, "termbridge scheduled discovery failed");
                    }
                }
            }
        }
    });

    handle
}

async fn run_discovery(
    service: Arc<TermBridgeServiceType>,
    metrics: Option<&Arc<PrismMetrics>>,
    reason: Option<&'static str>,
) -> anyhow::Result<()> {
    let started = time::Instant::now();
    match service.discover().await {
        Ok(state) => {
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            if let Some(metrics) = metrics {
                metrics.record_termbridge_action(
                    "discover",
                    reason.unwrap_or("scheduled"),
                    elapsed,
                    "success",
                );
            }
            debug!(
                count = state.capabilities().len(),
                ?reason,
                elapsed_ms = elapsed,
                "termbridge discovery completed"
            );
            Ok(())
        }
        Err(err) => {
            if let Some(metrics) = metrics {
                metrics.record_termbridge_error("discover", "all", &err.to_string());
            }
            error!(?reason, %err, "termbridge discovery failure");
            Err(anyhow::Error::new(err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::termbridge::{
        FileTermBridgeStateRepository, InMemoryTermBridgeBindingRepository,
    };
    use crate::app::termbridge::TermBridgeService;
    use crate::domain::termbridge::{TerminalCapabilities, TerminalId};
    use crate::ports::termbridge::{CapabilityObservation, TerminalControlPort};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    #[derive(Clone)]
    struct StubAdapter {
        id: TerminalId,
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TerminalControlPort for StubAdapter {
        fn terminal_id(&self) -> TerminalId {
            self.id.clone()
        }

        async fn detect(&self) -> CapabilityObservation {
            self.counter.fetch_add(1, Ordering::SeqCst);
            CapabilityObservation::new(
                self.id.as_str(),
                TerminalCapabilities::builder().send_text(true).build(),
                false,
                Vec::new(),
            )
        }
    }

    #[tokio::test]
    async fn notify_refresh_triggers_discovery() {
        let dir = tempdir().unwrap();
        let state_repo = Arc::new(FileTermBridgeStateRepository::new(
            dir.path().join("capabilities.json"),
        ));
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let counter = Arc::new(AtomicUsize::new(0));
        let adapter = StubAdapter {
            id: TerminalId::new("testterm"),
            counter: counter.clone(),
        };
        let service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo,
            vec![Arc::new(adapter)],
            None,
        ));
        let handle = spawn_discovery_task(service.clone(), None, Some(Duration::from_secs(3600)));
        handle.notify_refresh("test");

        tokio::time::timeout(Duration::from_millis(200), async {
            while counter.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("discovery trigger timed out");

        let snapshot = service.snapshot().await.expect("snapshot");
        assert_eq!(snapshot.capabilities().len(), 1);
        handle.shutdown().await;
    }
}
