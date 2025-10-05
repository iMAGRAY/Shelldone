use chrono::Utc;
use config::CACHE_DIR;
use crossbeam::channel::{bounded, Receiver, Sender, TrySendError};
use mux::sigma_proxy::{
    set_sigma_policy_reporter, SigmaDirection, SigmaPolicyReporter, SigmaViolation,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fs, thread};

const DEFAULT_AGENTD_URL: &str = "http://127.0.0.1:17717/journal/event";
const QUEUE_BOUND: usize = 256;
const ERROR_THROTTLE: Duration = Duration::from_secs(5);
const DEFAULT_SPOOL_FILE: &str = "sigma_guard_spool.jsonl";
const DEFAULT_SPOOL_MAX_BYTES: u64 = 1_048_576; // 1 MiB

pub fn install_sigma_reporter() {
    let disabled = std::env::var("SHELLDONE_SIGMA_REPORTER")
        .map(|value| value.trim() == "0" || value.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    if disabled {
        log::info!("Sigma policy reporter disabled via SHELLDONE_SIGMA_REPORTER");
        return;
    }

    let endpoint =
        std::env::var("SHELLDONE_AGENTD_URL").unwrap_or_else(|_| DEFAULT_AGENTD_URL.to_string());
    let spool_disabled = std::env::var("SHELLDONE_SIGMA_SPOOL")
        .map(|value| value.trim() == "0" || value.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    let spool_config = if spool_disabled {
        None
    } else {
        let max_bytes = std::env::var("SHELLDONE_SIGMA_SPOOL_MAX_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_SPOOL_MAX_BYTES);
        Some(Arc::new(SpoolConfig {
            path: CACHE_DIR.join(DEFAULT_SPOOL_FILE),
            max_bytes,
        }))
    };

    let (tx, rx) = bounded::<SigmaViolation>(QUEUE_BOUND);
    let reporter = Arc::new(AgentdSigmaReporter::new(tx, spool_config.clone()));
    set_sigma_policy_reporter(reporter);

    thread::Builder::new()
        .name("sigma-guard-reporter".into())
        .spawn(move || worker_loop(endpoint, rx, spool_config))
        .expect("spawn sigma reporter worker");
}

#[derive(Debug)]
struct SpoolConfig {
    path: PathBuf,
    max_bytes: u64,
}

struct AgentdSigmaReporter {
    tx: Sender<SigmaViolation>,
    dropped: AtomicUsize,
    spool: Option<Arc<SpoolConfig>>,
}

impl AgentdSigmaReporter {
    fn new(tx: Sender<SigmaViolation>, spool: Option<Arc<SpoolConfig>>) -> Self {
        Self {
            tx,
            dropped: AtomicUsize::new(0),
            spool,
        }
    }
}

impl SigmaPolicyReporter for AgentdSigmaReporter {
    fn report(&self, violation: SigmaViolation) {
        match self.tx.try_send(violation) {
            Ok(_) => {
                self.dropped.store(0, Ordering::Relaxed);
            }
            Err(TrySendError::Full(violation)) => {
                if self.dropped.fetch_add(1, Ordering::Relaxed) == 0 {
                    log::warn!("Sigma policy reporter queue is full; dropping violations");
                }
                if let Some(spool) = &self.spool {
                    let request = JournalRequest::from_violation(&violation);
                    if let Err(err) = persist_request(spool, &request) {
                        log::error!("Failed to persist sigma guard event: {err:#}");
                    }
                }
            }
            Err(TrySendError::Disconnected(violation)) => {
                if self.dropped.fetch_add(1, Ordering::Relaxed) == 0 {
                    log::error!("Sigma policy reporter channel disconnected");
                }
                if let Some(spool) = &self.spool {
                    let request = JournalRequest::from_violation(&violation);
                    if let Err(err) = persist_request(spool, &request) {
                        log::error!("Failed to persist sigma guard event: {err:#}");
                    }
                }
            }
        }
    }
}

fn worker_loop(endpoint: String, rx: Receiver<SigmaViolation>, spool: Option<Arc<SpoolConfig>>) {
    let client = match Client::builder()
        .timeout(Duration::from_millis(200))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            log::error!("Failed to build reqwest client for Sigma reporter: {err:#}");
            return;
        }
    };

    let mut last_error: Option<Instant> = None;

    if let Some(spool) = &spool {
        if let Err(err) = replay_spool(&client, &endpoint, spool, &mut last_error) {
            log::error!("Failed to replay sigma guard spool: {err:#}");
        }
    }

    for violation in rx.iter() {
        let request = JournalRequest::from_violation(&violation);
        if !send_request(&client, &endpoint, &request, &mut last_error) {
            if let Some(spool) = &spool {
                if let Err(err) = persist_request(spool, &request) {
                    log::error!("Failed to persist sigma guard event: {err:#}");
                }
            }
        }
    }
}

fn throttle_warn(last_error: &mut Option<Instant>, message: String) {
    let now = Instant::now();
    let should_log = match last_error {
        Some(ts) => now.duration_since(*ts) >= ERROR_THROTTLE,
        None => true,
    };
    if should_log {
        log::warn!("{message}");
        *last_error = Some(now);
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct JournalRequest {
    kind: String,
    persona: Option<String>,
    payload: JournalPayload,
    spectral_tag: Option<String>,
    bytes: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize)]
struct JournalPayload {
    reason: String,
    direction: String,
    sequence_preview: String,
    sequence_len: usize,
    occurred_at: String,
}

impl JournalRequest {
    fn from_violation(violation: &SigmaViolation) -> Self {
        Self {
            kind: "sigma.guard".to_string(),
            persona: None,
            payload: JournalPayload {
                reason: violation.reason.to_string(),
                direction: match violation.direction {
                    SigmaDirection::Input => "input".to_string(),
                    SigmaDirection::Output => "output".to_string(),
                },
                sequence_preview: violation.sequence_preview.clone(),
                sequence_len: violation.sequence_len,
                occurred_at: chrono::DateTime::<Utc>::from(violation.occurred_at).to_rfc3339(),
            },
            spectral_tag: Some("sigma::guard".to_string()),
            bytes: Some(violation.sequence_len),
        }
    }
}

fn send_request(
    client: &Client,
    endpoint: &str,
    request: &JournalRequest,
    last_error: &mut Option<Instant>,
) -> bool {
    match client.post(endpoint).json(request).send() {
        Ok(response) => {
            if response.status().is_success() {
                true
            } else {
                throttle_warn(
                    last_error,
                    format!(
                        "Sigma reporter received status {} from {}",
                        response.status(),
                        endpoint
                    ),
                );
                false
            }
        }
        Err(err) => {
            throttle_warn(
                last_error,
                format!("Sigma reporter failed to POST to {endpoint}: {err:#}"),
            );
            false
        }
    }
}

fn persist_request(spool: &SpoolConfig, request: &JournalRequest) -> anyhow::Result<()> {
    let path = &spool.path;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut file, request)?;
    file.write_all(b"\n")?;
    file.flush()?;

    if file.metadata()?.len() > spool.max_bytes {
        trim_spool(path, spool.max_bytes)?;
    }
    Ok(())
}

fn replay_spool(
    client: &Client,
    endpoint: &str,
    spool: &SpoolConfig,
    last_error: &mut Option<Instant>,
) -> anyhow::Result<()> {
    let path = &spool.path;
    if !path.exists() {
        return Ok(());
    }
    let data = fs::read_to_string(path)?;
    let mut remaining = Vec::new();
    for (idx, line) in data.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<JournalRequest>(trimmed) {
            Ok(request) => {
                if !send_request(client, endpoint, &request, last_error) {
                    remaining.push(request);
                }
            }
            Err(err) => {
                log::error!("Failed to parse sigma guard spooled line {}: {err:#}", idx);
            }
        }
    }

    if remaining.is_empty() {
        fs::remove_file(path).ok();
    } else {
        let mut file = fs::File::create(path)?;
        for request in remaining {
            serde_json::to_writer(&mut file, &request)?;
            file.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn trim_spool(path: &Path, max_bytes: u64) -> anyhow::Result<()> {
    let data = fs::read_to_string(path)?;
    let mut size: u64 = 0;
    let mut lines = Vec::new();
    for line in data.lines().rev() {
        let line_bytes = (line.len() + 1) as u64;
        if size + line_bytes > max_bytes {
            break;
        }
        size += line_bytes;
        lines.push(line);
    }
    lines.reverse();
    let mut file = fs::File::create(path)?;
    for line in lines {
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
    }
    Ok(())
}
