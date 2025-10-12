use super::super::ports::{
    AgentFrame, AgentFrameStatus, ApprovalFrame, ApprovalSource, ExperienceTelemetryPort,
    PersonaFrame, TelemetrySnapshot,
};
use crate::experience::ports::telemetry_port::{
    StateSnapshotFrame, TermBridgeDeltaSnapshot, TermBridgeTerminalCapabilities,
    TermBridgeTerminalChange,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dirs_next::config_dir;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::convert::{TryFrom, TryInto};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, thread};

const DISCOVERY_ENV_KEY: &str = "SHELLDONE_AGENTD_DISCOVERY";
const DISCOVERY_RELATIVE_PATH: &str = "shelldone/agentd.json";
const PENDING_APPROVALS_FILE: &str = "approvals/pending.json";
const DISCOVERY_TOKEN_ENV_KEY: &str = "SHELLDONE_AGENTD_DISCOVERY_TOKEN";
const ALLOW_INSECURE_ENV_KEY: &str = "SHELLDONE_GUI_ALLOW_INSECURE_AGENTD";
const DISCOVER_RETRY_ATTEMPTS: usize = 3;
const DISCOVER_RETRY_BACKOFF_MS: u64 = 125;

struct ApprovalFetch {
    approvals: Vec<ApprovalFrame>,
    source: ApprovalSource,
}

impl ApprovalFetch {
    fn new(approvals: Vec<ApprovalFrame>, source: ApprovalSource) -> Self {
        Self { approvals, source }
    }
}

fn read_env_trimmed(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_truthy(key: &str) -> bool {
    match env::var(key) {
        Ok(value) => {
            let trimmed = value.trim();
            trimmed == "1"
                || trimmed.eq_ignore_ascii_case("true")
                || trimmed.eq_ignore_ascii_case("yes")
                || trimmed.eq_ignore_ascii_case("on")
        }
        Err(_) => false,
    }
}

enum HttpFetchResult {
    Success(Vec<ApprovalFrame>),
    Failed,
    NotConfigured,
}

enum DiskFetchResult {
    Present(Vec<ApprovalFrame>),
    Missing,
}

#[derive(Debug)]
pub struct AgentdTelemetryPort {
    discovery_override: Option<PathBuf>,
}

impl AgentdTelemetryPort {
    pub fn new() -> Self {
        let override_path = env::var_os(DISCOVERY_ENV_KEY).map(PathBuf::from);
        Self {
            discovery_override: override_path,
        }
    }

    fn discovery_path(&self) -> Option<PathBuf> {
        if let Some(path) = &self.discovery_override {
            return Some(path.clone());
        }
        let mut path = config_dir()?;
        path.push(DISCOVERY_RELATIVE_PATH);
        Some(path)
    }

    fn read_discovery(&self) -> Result<Option<DiscoveryDocument>> {
        let Some(path) = self.discovery_path() else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path)
            .with_context(|| format!("reading agentd discovery file at {}", path.display()))?;
        let doc: DiscoveryDocument = serde_json::from_slice(&data)
            .with_context(|| format!("parsing agentd discovery file at {}", path.display()))?;
        Ok(Some(doc))
    }

    fn read_pending_approvals(&self, discovery: &DiscoveryDocument) -> ApprovalFetch {
        match self.fetch_pending_approvals_http(discovery) {
            HttpFetchResult::Success(approvals) => {
                log::info!(
                    "experience.telemetry: approvals sourced via HTTP ({} entries)",
                    approvals.len()
                );
                metrics::counter!("experience.approvals.source", "source" => "http").increment(1);
                return ApprovalFetch::new(approvals, ApprovalSource::Http);
            }
            HttpFetchResult::Failed => {
                log::warn!(
                    "experience.telemetry: approvals HTTP fetch failed; falling back to local snapshot"
                );
                metrics::counter!("experience.approvals.source", "source" => "http_fail")
                    .increment(1);
            }
            HttpFetchResult::NotConfigured => {
                log::debug!("experience.telemetry: approvals HTTP endpoint not configured");
                metrics::counter!("experience.approvals.source", "source" => "http_missing")
                    .increment(1);
            }
        }

        match self.read_pending_approvals_from_disk(discovery) {
            DiskFetchResult::Present(approvals) => {
                metrics::counter!("experience.approvals.source", "source" => "local").increment(1);
                ApprovalFetch::new(approvals, ApprovalSource::Local)
            }
            DiskFetchResult::Missing => {
                metrics::counter!("experience.approvals.source", "source" => "none").increment(1);
                ApprovalFetch::new(Vec::new(), ApprovalSource::None)
            }
        }
    }

    fn read_pending_approvals_from_disk(&self, discovery: &DiscoveryDocument) -> DiskFetchResult {
        let Some(paths) = discovery.paths.as_ref() else {
            return DiskFetchResult::Missing;
        };
        let Ok(state_dir) = PathBuf::from(&paths.state_dir).canonicalize() else {
            log::debug!(
                "experience.telemetry: state_dir {} missing or inaccessible",
                paths.state_dir
            );
            return DiskFetchResult::Missing;
        };
        let path = state_dir.join(PENDING_APPROVALS_FILE);
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    log::warn!(
                        "experience.telemetry: failed to read approvals file {}: {err}",
                        path.display()
                    );
                }
                return DiskFetchResult::Missing;
            }
        };
        if data.is_empty() {
            return DiskFetchResult::Present(Vec::new());
        }
        let approvals: Vec<PendingApprovalRecord> = match serde_json::from_slice(&data) {
            Ok(records) => records,
            Err(err) => {
                log::warn!(
                    "experience.telemetry: failed to parse approvals file {}: {err}",
                    path.display()
                );
                return DiskFetchResult::Present(Vec::new());
            }
        };
        let frames = approvals
            .into_iter()
            .filter_map(|record| record.try_into().ok())
            .collect();
        DiskFetchResult::Present(frames)
    }

    fn read_state_snapshots(&self, discovery: &DiscoveryDocument) -> Vec<StateSnapshotFrame> {
        let Some(paths) = discovery.paths.as_ref() else {
            return Vec::new();
        };
        let Ok(state_dir) = PathBuf::from(&paths.state_dir).canonicalize() else {
            log::debug!(
                "experience.telemetry: state_dir {} missing or inaccessible",
                paths.state_dir
            );
            return Vec::new();
        };
        let snapshot_dir = state_dir.join("snapshots");
        let entries = match fs::read_dir(&snapshot_dir) {
            Ok(entries) => entries,
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    log::debug!(
                        "experience.telemetry: failed to read snapshots dir {}: {err}",
                        snapshot_dir.display()
                    );
                }
                return Vec::new();
            }
        };

        let mut frames = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(meta) => meta,
                Err(err) => {
                    log::debug!(
                        "experience.telemetry: failed to stat snapshot {}: {err}",
                        path.display()
                    );
                    continue;
                }
            };
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let created_at = metadata
                .created()
                .or_else(|_| metadata.modified())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());
            let size_bytes = metadata.len();
            let tags = snapshot_tags(&path);
            frames.push(StateSnapshotFrame {
                id: stem.to_string(),
                created_at,
                size_bytes,
                path: path.display().to_string(),
                tags,
            });
        }

        frames.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        frames.truncate(12);
        frames
    }

    fn fetch_pending_approvals_http(&self, discovery: &DiscoveryDocument) -> HttpFetchResult {
        let Some(endpoints) = discovery.endpoints.as_ref() else {
            return HttpFetchResult::NotConfigured;
        };
        let Some(http) = endpoints.http.as_ref() else {
            return HttpFetchResult::NotConfigured;
        };
        let mut base = http.listen.trim().trim_end_matches('/').to_string();
        if base.is_empty() {
            return HttpFetchResult::NotConfigured;
        }
        if !base.starts_with("http://") && !base.starts_with("https://") {
            base = format!("http://{}", base);
        }
        let url = format!("{}/approvals/pending", base.trim_end_matches('/'));

        let client = Client::builder()
            .timeout(Duration::from_millis(750))
            .build()
            .map_err(|err| {
                log::warn!(
                    "experience.telemetry: failed to construct HTTP client for approvals: {err}"
                );
                err
            })
            .ok();

        let client = match client {
            Some(client) => client,
            None => return HttpFetchResult::Failed,
        };

        let response = match client.get(&url).header("Accept", "application/json").send() {
            Ok(resp) => resp,
            Err(err) => {
                log::warn!(
                    "experience.telemetry: approvals endpoint {} request failed: {err}",
                    url
                );
                return HttpFetchResult::Failed;
            }
        };

        if !response.status().is_success() {
            log::warn!(
                "experience.telemetry: approvals endpoint {} responded with status {}",
                url,
                response.status()
            );
            return HttpFetchResult::Failed;
        }

        let payload: PendingApprovalsResponse = match response.json() {
            Ok(data) => data,
            Err(err) => {
                log::warn!("experience.telemetry: failed to decode approvals JSON: {err}");
                return HttpFetchResult::Failed;
            }
        };

        let approvals: Vec<ApprovalFrame> = payload
            .approvals
            .into_iter()
            .filter(|record| record.status.eq_ignore_ascii_case("pending"))
            .filter_map(|record| record.try_into().ok())
            .collect();

        if approvals.is_empty() {
            log::debug!(
                "experience.telemetry: approvals endpoint {} returned 0 entries",
                url
            );
        }

        HttpFetchResult::Success(approvals)
    }

    fn map_agents(&self, discovery: &DiscoveryDocument) -> Vec<AgentFrame> {
        discovery
            .agents
            .iter()
            .filter_map(|agent| {
                agent
                    .clone()
                    .try_into()
                    .map_err(|err| {
                        log::warn!(
                            "experience.telemetry: skipping agent {} due to {err}",
                            agent.id
                        );
                    })
                    .ok()
            })
            .collect()
    }

    fn fetch_termbridge_delta_http(
        &self,
        discovery: &DiscoveryDocument,
    ) -> Option<TermBridgeDeltaSnapshot> {
        let endpoints = discovery.endpoints.as_ref()?;
        let http = endpoints.http.as_ref()?;
        let listen = http.listen.trim();
        if listen.is_empty() {
            return None;
        }

        let allow_insecure = env_truthy(ALLOW_INSECURE_ENV_KEY);
        let mut base = listen.trim_end_matches('/').to_string();
        let mut upgraded_scheme = false;

        if base.starts_with("http://") {
            if !allow_insecure {
                base = format!("https://{}", base.trim_start_matches("http://"));
                upgraded_scheme = true;
            }
        } else if !base.starts_with("https://") {
            base = format!("https://{}", base);
            upgraded_scheme = true;
        }

        if base.starts_with("http://") && !allow_insecure {
            log::warn!(
                "experience.telemetry: refusing HTTP discover endpoint {}; set {}=1 to allow insecure fallback",
                base,
                ALLOW_INSECURE_ENV_KEY
            );
            return None;
        }

        if upgraded_scheme {
            log::debug!(
                "experience.telemetry: discovery endpoint upgraded to {}",
                base
            );
        }

        let url = format!("{}/termbridge/discover", base.trim_end_matches('/'));
        let token = read_env_trimmed(DISCOVERY_TOKEN_ENV_KEY);

        let client = Client::builder()
            .timeout(Duration::from_millis(1000))
            .build()
            .ok()?;

        let mut last_error: Option<String> = None;
        for attempt in 0..DISCOVER_RETRY_ATTEMPTS {
            let mut request = client.post(&url).header("Accept", "application/json");
            if let Some(value) = token.as_ref() {
                request = request.bearer_auth(value);
            }

            match request.send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<TermBridgeDiscoverResponse>() {
                            Ok(payload) => {
                                let diff = payload.diff.unwrap_or_default();
                                let terminals = payload
                                    .terminals
                                    .into_iter()
                                    .map(TermBridgeTerminalChange::from)
                                    .collect();
                                return Some(TermBridgeDeltaSnapshot {
                                    changed: payload.changed,
                                    terminals,
                                    added: diff
                                        .added
                                        .into_iter()
                                        .map(TermBridgeTerminalChange::from)
                                        .collect(),
                                    updated: diff
                                        .updated
                                        .into_iter()
                                        .map(TermBridgeTerminalChange::from)
                                        .collect(),
                                    removed: diff
                                        .removed
                                        .into_iter()
                                        .map(TermBridgeTerminalChange::from)
                                        .collect(),
                                });
                            }
                            Err(err) => {
                                last_error = Some(format!("decode error: {err}"));
                            }
                        }
                    } else if status.as_u16() == 401 {
                        log::warn!(
                            "experience.telemetry: termbridge discover {} rejected with 401 (check {})",
                            url,
                            DISCOVERY_TOKEN_ENV_KEY
                        );
                        return None;
                    } else {
                        last_error = Some(format!("status {}", status));
                    }
                }
                Err(err) => {
                    last_error = Some(format!("request error: {err}"));
                }
            }

            if attempt + 1 < DISCOVER_RETRY_ATTEMPTS {
                let backoff = DISCOVER_RETRY_BACKOFF_MS * (attempt as u64 + 1);
                thread::sleep(Duration::from_millis(backoff));
            }
        }

        if let Some(reason) = last_error {
            log::debug!(
                "experience.telemetry: termbridge discover {} exhausted retries: {}",
                url,
                reason
            );
        }

        None
    }
}

impl Default for AgentdTelemetryPort {
    fn default() -> Self {
        Self::new()
    }
}

impl ExperienceTelemetryPort for AgentdTelemetryPort {
    fn snapshot(&self) -> Result<TelemetrySnapshot> {
        let Some(discovery) = self.read_discovery()? else {
            return Ok(TelemetrySnapshot::default());
        };

        let generated_at = parse_datetime(&discovery.generated_at);
        let agents = self.map_agents(&discovery);
        let persona = discovery.persona_env.as_ref().map(|name| PersonaFrame {
            name: name.clone(),
            intent_hint: discovery
                .termbridge
                .as_ref()
                .and_then(|status| status.intent_hint()),
            tone_hint: None,
        });

        let approvals = self.read_pending_approvals(&discovery);
        let state_snapshots = self.read_state_snapshots(&discovery);

        Ok(TelemetrySnapshot {
            generated_at,
            persona,
            agents,
            approvals: approvals.approvals,
            telemetry_ready: discovery.telemetry_ready.unwrap_or(false),
            approvals_source: approvals.source,
            termbridge_delta: self.fetch_termbridge_delta_http(&discovery),
            state_snapshots,
        })
    }
}

fn parse_datetime(input: &Option<String>) -> Option<DateTime<Utc>> {
    input.as_ref().and_then(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%:z")
                    .map(|dt| dt.with_timezone(&Utc))
            })
            .ok()
    })
}

fn snapshot_tags(path: &Path) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        tags.push(ext.to_ascii_lowercase());
    }
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
        let lower = file_name.to_ascii_lowercase();
        if lower.contains("auto") {
            tags.push("auto".to_string());
        }
        if lower.contains("manual") {
            tags.push("manual".to_string());
        }
        if lower.contains("lock") || lower.contains("protected") {
            tags.push("protected".to_string());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryDocument {
    generated_at: Option<String>,
    persona_env: Option<String>,
    #[serde(default)]
    telemetry_ready: Option<bool>,
    agents: Vec<DiscoveryAgent>,
    #[serde(default)]
    termbridge: Option<DiscoveryTermBridge>,
    #[serde(default)]
    endpoints: Option<DiscoveryEndpoints>,
    #[serde(default)]
    paths: Option<DiscoveryPaths>,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct DiscoveryEndpoints {
    #[serde(default)]
    http: Option<EndpointInfo>,
    #[allow(dead_code)]
    #[serde(default)]
    grpc: Option<DiscoveryGrpcInfo>,
}

#[derive(Clone, Debug, Deserialize)]
struct EndpointInfo {
    listen: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize, Default)]
struct DiscoveryGrpcInfo {
    #[serde(default)]
    listen: Option<String>,
    #[serde(default)]
    tls_policy: Option<String>,
    #[serde(default)]
    cert: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    ca: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryPaths {
    state_dir: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryTermBridge {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    terminals: Vec<DiscoveryTermBridgeTerminal>,
}

impl DiscoveryTermBridge {
    fn intent_hint(&self) -> Option<String> {
        if let Some(enabled) = self.enabled {
            if !enabled {
                return Some("recover".to_string());
            }
        }
        if self
            .terminals
            .iter()
            .any(|terminal| terminal.requires_opt_in.unwrap_or(false))
        {
            Some("focus".to_string())
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryTermBridgeTerminal {
    #[serde(default)]
    requires_opt_in: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryAgent {
    id: String,
    provider: String,
    version: String,
    channel: String,
    status: String,
    capabilities: Vec<String>,
    #[serde(default)]
    last_heartbeat_at: Option<String>,
    registered_at: String,
}

impl TryFrom<DiscoveryAgent> for AgentFrame {
    type Error = anyhow::Error;

    fn try_from(value: DiscoveryAgent) -> Result<Self> {
        let status = match value.status.as_str() {
            "active" => AgentFrameStatus::Active,
            "registered" => AgentFrameStatus::Registered,
            "disabled" => AgentFrameStatus::Disabled,
            other => anyhow::bail!("unknown agent status {other}"),
        };
        let registered_at = DateTime::parse_from_rfc3339(&value.registered_at)
            .map(|dt| dt.with_timezone(&Utc))
            .with_context(|| format!("parsing registered_at for agent {}", value.id))?;
        let last_heartbeat_at = parse_datetime(&value.last_heartbeat_at);
        let label = format_provider_label(
            &value.provider,
            &value.channel,
            &value.version,
            value.capabilities.len(),
        );
        Ok(Self {
            id: value.id,
            label,
            provider: value.provider,
            channel: Some(value.channel),
            status,
            last_heartbeat_at,
            registered_at,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct PendingApprovalRecord {
    id: String,
    command: String,
    persona: Option<String>,
    reason: String,
    requested_at: String,
    #[allow(dead_code)]
    spectral_tag: Option<String>,
    #[allow(dead_code)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PendingApprovalsResponse {
    approvals: Vec<PendingApprovalHttpRecord>,
}

#[derive(Debug, Deserialize)]
struct TermBridgeDiscoverResponse {
    changed: bool,
    #[serde(default)]
    terminals: Vec<TermBridgeDiscoverTerminal>,
    #[serde(default)]
    diff: Option<TermBridgeDiscoverDiff>,
}

#[derive(Debug, Default, Deserialize)]
struct TermBridgeDiscoverDiff {
    #[serde(default)]
    added: Vec<TermBridgeDiscoverTerminal>,
    #[serde(default)]
    updated: Vec<TermBridgeDiscoverTerminal>,
    #[serde(default)]
    removed: Vec<TermBridgeDiscoverTerminal>,
}

#[derive(Debug, Deserialize)]
struct TermBridgeDiscoverTerminal {
    terminal: String,
    #[serde(default)]
    requires_opt_in: bool,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    capabilities: TermBridgeDiscoverCapabilities,
}

#[derive(Debug, Default, Deserialize)]
struct TermBridgeDiscoverCapabilities {
    #[serde(default)]
    spawn: bool,
    #[serde(default)]
    split: bool,
    #[serde(default)]
    focus: bool,
    #[serde(default)]
    duplicate: bool,
    #[serde(default)]
    close: bool,
    #[serde(default)]
    send_text: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct PendingApprovalHttpRecord {
    id: String,
    command: String,
    persona: Option<String>,
    reason: String,
    requested_at: String,
    #[serde(default)]
    status: String,
    #[allow(dead_code)]
    #[serde(default)]
    resolved_at: Option<String>,
}

impl TryFrom<PendingApprovalRecord> for ApprovalFrame {
    type Error = anyhow::Error;

    fn try_from(value: PendingApprovalRecord) -> Result<Self> {
        let requested_at = DateTime::parse_from_rfc3339(&value.requested_at)
            .map(|dt| dt.with_timezone(&Utc))
            .with_context(|| format!("parsing requested_at for approval {}", value.id))?;
        Ok(Self {
            id: value.id,
            command: value.command,
            persona: value.persona,
            reason: value.reason,
            requested_at,
        })
    }
}

impl TryFrom<PendingApprovalHttpRecord> for ApprovalFrame {
    type Error = anyhow::Error;

    fn try_from(value: PendingApprovalHttpRecord) -> Result<Self> {
        let requested_at = DateTime::parse_from_rfc3339(&value.requested_at)
            .map(|dt| dt.with_timezone(&Utc))
            .with_context(|| format!("parsing requested_at for approval {}", value.id))?;
        Ok(Self {
            id: value.id,
            command: value.command,
            persona: value.persona,
            reason: value.reason,
            requested_at,
        })
    }
}

impl From<TermBridgeDiscoverTerminal> for TermBridgeTerminalChange {
    fn from(value: TermBridgeDiscoverTerminal) -> Self {
        let TermBridgeDiscoverTerminal {
            terminal,
            requires_opt_in,
            source,
            capabilities,
        } = value;
        Self {
            terminal,
            requires_opt_in,
            source: source.and_then(|entry| {
                let trimmed = entry.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            capabilities: capabilities.into(),
        }
    }
}

impl From<TermBridgeDiscoverCapabilities> for TermBridgeTerminalCapabilities {
    fn from(value: TermBridgeDiscoverCapabilities) -> Self {
        Self {
            spawn: value.spawn,
            split: value.split,
            focus: value.focus,
            duplicate: value.duplicate,
            close: value.close,
            send_text: value.send_text,
        }
    }
}

fn format_provider_label(
    provider: &str,
    channel: &str,
    version: &str,
    capability_count: usize,
) -> String {
    lazy_static! {
        static ref ALNUM: Regex = Regex::new(r"[^A-Za-z0-9]+").expect("regex compile");
    }
    let pretty_provider = if provider.is_empty() {
        "agent".to_string()
    } else {
        let cleaned = ALNUM.replace_all(provider, " ");
        cleaned
            .split_whitespace()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!("{pretty_provider} {channel} ({version}, {capability_count} caps)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::thread;
    use tempfile::tempdir;

    lazy_static! {
        static ref TEST_GUARD: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn snapshot_reads_agents_and_approvals() {
        let _guard = TEST_GUARD.lock().unwrap();
        let temp = tempdir().unwrap();
        let discovery_path = temp.path().join("agentd.json");
        let state_dir = temp.path().join("state");
        std::fs::create_dir_all(state_dir.join("approvals")).unwrap();
        let approval_json = serde_json::json!([
            {
                "id": "approval-1",
                "command": "agent.exec",
                "persona": "Nova",
                "reason": "dangerous",
                "requested_at": "2025-10-05T03:10:00Z",
                "status": "pending"
            }
        ]);
        std::fs::write(
            state_dir.join("approvals").join("pending.json"),
            serde_json::to_vec_pretty(&approval_json).unwrap(),
        )
        .unwrap();

        let discovery = serde_json::json!({
            "generated_at": "2025-10-05T03:12:23Z",
            "persona_env": "Nova",
            "telemetry_ready": true,
            "paths": {"state_dir": state_dir.display().to_string()},
            "agents": [
                {
                    "id": "agent-1",
                    "provider": "openai",
                    "version": "1.2.0",
                    "channel": "stable",
                    "status": "active",
                    "capabilities": ["agent.exec"],
                    "last_heartbeat_at": "2025-10-05T03:11:10Z",
                    "registered_at": "2025-10-05T03:00:00Z"
                }
            ]
        });
        std::fs::write(
            &discovery_path,
            serde_json::to_vec_pretty(&discovery).unwrap(),
        )
        .unwrap();

        std::env::set_var(ALLOW_INSECURE_ENV_KEY, "1");
        std::env::set_var("SHELLDONE_AGENTD_DISCOVERY", &discovery_path);
        let port = AgentdTelemetryPort::new();
        let snapshot = port.snapshot().unwrap();
        assert_eq!(snapshot.agents.len(), 1);
        assert_eq!(snapshot.approvals.len(), 1);
        assert!(snapshot.telemetry_ready);
        assert_eq!(snapshot.approvals_source, ApprovalSource::Local);
        let agent = &snapshot.agents[0];
        assert_eq!(agent.provider, "openai");
        assert_eq!(agent.channel.as_deref(), Some("stable"));
        assert_eq!(agent.status, AgentFrameStatus::Active);
        assert!(agent.last_heartbeat_at.is_some());
        let approval = &snapshot.approvals[0];
        assert_eq!(approval.command, "agent.exec");
        assert_eq!(approval.persona.as_deref(), Some("Nova"));
        assert_eq!(approval.reason, "dangerous");
        // Clean up override
        std::env::remove_var("SHELLDONE_AGENTD_DISCOVERY");
    }

    #[test]
    fn snapshot_prefers_http_pending_approvals() {
        let _guard = TEST_GUARD.lock().unwrap();
        let temp = tempdir().unwrap();
        let discovery_path = temp.path().join("agentd.json");
        let state_dir = temp.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let body = serde_json::json!({
                    "approvals": [
                        {
                            "id": "approval-http",
                            "command": "agent.exec",
                            "persona": "Nova",
                            "reason": "requires consent",
                            "requested_at": "2025-10-05T03:10:00Z",
                            "status": "pending"
                        }
                    ]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let discovery = serde_json::json!({
            "generated_at": "2025-10-05T03:12:23Z",
            "persona_env": "Nova",
            "telemetry_ready": true,
            "endpoints": {"http": {"listen": addr.to_string()}},
            "paths": {"state_dir": state_dir.display().to_string()},
            "agents": []
        });
        std::fs::write(
            &discovery_path,
            serde_json::to_vec_pretty(&discovery).unwrap(),
        )
        .unwrap();

        std::env::set_var(ALLOW_INSECURE_ENV_KEY, "1");
        std::env::set_var("SHELLDONE_AGENTD_DISCOVERY", &discovery_path);
        let port = AgentdTelemetryPort::new();
        let snapshot = port.snapshot().unwrap();
        assert_eq!(snapshot.approvals.len(), 1);
        assert_eq!(snapshot.approvals[0].id, "approval-http");
        assert_eq!(snapshot.approvals[0].reason, "requires consent");
        assert_eq!(snapshot.approvals_source, ApprovalSource::Http);
        std::env::remove_var("SHELLDONE_AGENTD_DISCOVERY");
        std::env::remove_var(ALLOW_INSECURE_ENV_KEY);

        server.join().unwrap();
    }

    #[test]
    fn snapshot_restores_http_after_failure() {
        let _guard = TEST_GUARD.lock().unwrap();
        let temp = tempdir().unwrap();
        let discovery_path = temp.path().join("agentd.json");
        let state_dir = temp.path().join("state");
        std::fs::create_dir_all(state_dir.join("approvals")).unwrap();
        let local_approvals = serde_json::json!([
            {
                "id": "local-1",
                "command": "agent.exec",
                "persona": "Nova",
                "reason": "cached fallback",
                "requested_at": "2025-10-05T04:00:00Z",
                "status": "pending"
            }
        ]);
        std::fs::write(
            state_dir.join("approvals").join("pending.json"),
            serde_json::to_vec_pretty(&local_approvals).unwrap(),
        )
        .unwrap();

        let discovery_unreachable = serde_json::json!({
            "generated_at": "2025-10-05T04:05:00Z",
            "persona_env": "Nova",
            "telemetry_ready": false,
            "endpoints": {"http": {"listen": "127.0.0.1:9"}},
            "paths": {"state_dir": state_dir.display().to_string()},
            "agents": []
        });
        std::fs::write(
            &discovery_path,
            serde_json::to_vec_pretty(&discovery_unreachable).unwrap(),
        )
        .unwrap();

        std::env::set_var("SHELLDONE_AGENTD_DISCOVERY", &discovery_path);
        // No HTTP server yet -> should fallback to local snapshot
        let port = AgentdTelemetryPort::new();
        let snapshot = port.snapshot().unwrap();
        assert_eq!(snapshot.approvals_source, ApprovalSource::Local);
        assert_eq!(snapshot.approvals.len(), 1);

        // Start HTTP responder returning fresh data
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let discovery_live = serde_json::json!({
            "generated_at": "2025-10-05T04:10:00Z",
            "persona_env": "Nova",
            "telemetry_ready": true,
            "endpoints": {"http": {"listen": addr.to_string()}},
            "paths": {"state_dir": state_dir.display().to_string()},
            "agents": []
        });
        std::fs::write(
            &discovery_path,
            serde_json::to_vec_pretty(&discovery_live).unwrap(),
        )
        .unwrap();

        use std::sync::mpsc;
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            ready_tx.send(()).ok();
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let body = serde_json::json!({
                    "approvals": [
                        {
                            "id": "http-1",
                            "command": "agent.exec",
                            "persona": "Nova",
                            "reason": "live",
                            "requested_at": "2025-10-05T04:10:00Z",
                            "status": "pending"
                        }
                    ]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        ready_rx.recv().unwrap();
        let refreshed = AgentdTelemetryPort::new().snapshot().unwrap();
        assert_eq!(refreshed.approvals_source, ApprovalSource::Http);
        assert_eq!(refreshed.approvals.len(), 1);
        assert_eq!(refreshed.approvals[0].id, "http-1");
        std::env::remove_var("SHELLDONE_AGENTD_DISCOVERY");
        std::env::remove_var(ALLOW_INSECURE_ENV_KEY);

        server.join().unwrap();
    }

    #[test]
    fn termbridge_delta_rejects_http_without_override() {
        let _guard = TEST_GUARD.lock().unwrap();
        std::env::remove_var(ALLOW_INSECURE_ENV_KEY);
        std::env::remove_var(DISCOVERY_TOKEN_ENV_KEY);

        let discovery: DiscoveryDocument = serde_json::from_value(serde_json::json!({
            "generated_at": null,
            "persona_env": null,
            "telemetry_ready": false,
            "agents": [],
            "termbridge": null,
            "endpoints": {"http": {"listen": "127.0.0.1:8123"}},
            "paths": null
        }))
        .unwrap();

        let port = AgentdTelemetryPort::new();
        assert!(port.fetch_termbridge_delta_http(&discovery).is_none());
    }

    #[test]
    fn termbridge_delta_sends_bearer_token() {
        use std::io::{Read, Write};

        let _guard = TEST_GUARD.lock().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::env::set_var(ALLOW_INSECURE_ENV_KEY, "1");
        std::env::set_var(DISCOVERY_TOKEN_ENV_KEY, "delta-secret");

        let discovery: DiscoveryDocument = serde_json::from_value(serde_json::json!({
            "generated_at": null,
            "persona_env": null,
            "telemetry_ready": true,
            "agents": [],
            "termbridge": null,
            "endpoints": {"http": {"listen": format!("http://{}", addr)}},
            "paths": null
        }))
        .unwrap();

        let server = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 2048];
                let size = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..size]);
                let lower = request.to_ascii_lowercase();
                assert!(
                    lower.contains("authorization: bearer delta-secret"),
                    "request missing bearer token: {}",
                    request
                );
                let body = serde_json::json!({
                    "changed": false,
                    "diff": {
                        "added": [],
                        "updated": [],
                        "removed": []
                    }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let port = AgentdTelemetryPort::new();
        let delta = port.fetch_termbridge_delta_http(&discovery);
        assert!(delta.is_some());

        std::env::remove_var(ALLOW_INSECURE_ENV_KEY);
        std::env::remove_var(DISCOVERY_TOKEN_ENV_KEY);

        server.join().unwrap();
    }

    #[test]
    fn termbridge_delta_preserves_source_field() {
        use std::io::{Read, Write};

        let _guard = TEST_GUARD.lock().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::env::set_var(ALLOW_INSECURE_ENV_KEY, "1");

        let discovery: DiscoveryDocument = serde_json::from_value(serde_json::json!({
            "generated_at": null,
            "persona_env": null,
            "telemetry_ready": true,
            "agents": [],
            "termbridge": null,
            "endpoints": {"http": {"listen": format!("http://{}", addr)}},
            "paths": null
        }))
        .unwrap();

        let server = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 2048];
                let size = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..size]);
                assert!(request.contains("POST /termbridge/discover"));
                let body = serde_json::json!({
                    "changed": true,
                    "terminals": [
                        {
                            "terminal": "mcp-sync-e2e",
                            "requires_opt_in": false,
                            "source": "mcp",
                            "capabilities": {
                                "spawn": true,
                                "split": false,
                                "focus": true,
                                "duplicate": true,
                                "close": false,
                                "send_text": true
                            }
                        }
                    ],
                    "diff": {
                        "added": [
                            {
                                "terminal": "mcp-sync-e2e",
                                "requires_opt_in": false,
                                "source": "mcp",
                                "capabilities": {
                                    "spawn": true,
                                    "split": false,
                                    "focus": true,
                                    "duplicate": true,
                                    "close": false,
                                    "send_text": true
                                }
                            }
                        ],
                        "updated": [],
                        "removed": []
                    }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let port = AgentdTelemetryPort::new();
        let delta = port
            .fetch_termbridge_delta_http(&discovery)
            .expect("termbridge delta");
        assert_eq!(delta.terminals.len(), 1);
        let entry = &delta.terminals[0];
        assert_eq!(entry.terminal, "mcp-sync-e2e");
        assert_eq!(entry.source.as_deref(), Some("mcp"));
        assert!(entry.capabilities.send_text);
        assert!(entry.capabilities.focus);

        std::env::remove_var(ALLOW_INSECURE_ENV_KEY);

        server.join().unwrap();
    }
}
