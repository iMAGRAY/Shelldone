use crate::experience::adapters::AgentdTelemetryPort;
use crate::experience::ports::{ExperienceTelemetryPort, TelemetrySnapshot};
use async_lock::RwLock;
use once_cell::sync::Lazy;
use promise::spawn::spawn;
use smol::Timer;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(1);

static TELEMETRY_MANAGER: Lazy<TelemetryManager> = Lazy::new(TelemetryManager::start);

pub fn experience_telemetry() -> &'static TelemetryManager {
    &TELEMETRY_MANAGER
}

pub struct TelemetryManager {
    store: Arc<TelemetryStore>,
}

struct TelemetryStore {
    snapshot: RwLock<TelemetrySnapshot>,
    version: AtomicU64,
}

impl TelemetryManager {
    fn start() -> Self {
        let store = Arc::new(TelemetryStore {
            snapshot: RwLock::new(TelemetrySnapshot::default()),
            version: AtomicU64::new(0),
        });
        let store_clone = store.clone();
        let port = AgentdTelemetryPort::new();

        spawn(async move {
            loop {
                match port.snapshot() {
                    Ok(snapshot) => store_clone.update(snapshot).await,
                    Err(err) => log::debug!("experience.telemetry: snapshot failed: {err:#}",),
                }
                Timer::after(POLL_INTERVAL).await;
            }
        })
        .detach();

        Self { store }
    }

    pub async fn snapshot(&self) -> TelemetrySnapshot {
        self.store.snapshot.read().await.clone()
    }

    #[allow(dead_code)]
    pub fn version(&self) -> u64 {
        self.store.version.load(Ordering::SeqCst)
    }
}

impl TelemetryStore {
    async fn update(&self, new_snapshot: TelemetrySnapshot) {
        let mut guard = self.snapshot.write().await;
        if *guard != new_snapshot {
            *guard = new_snapshot;
            self.version.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl ExperienceTelemetryPort for TelemetryManager {
    fn snapshot(&self) -> anyhow::Result<TelemetrySnapshot> {
        Ok(smol::block_on(self.snapshot()))
    }
}

impl ExperienceTelemetryPort for &TelemetryManager {
    fn snapshot(&self) -> anyhow::Result<TelemetrySnapshot> {
        Ok(smol::block_on((*self).snapshot()))
    }
}
