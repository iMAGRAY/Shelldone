mod adapters;
mod app;
mod continuum;
mod domain;
pub mod policy_engine;
mod ports;
mod telemetry; // Public for benchmarks

pub use adapters::mcp::tls::CipherPolicy;

use adapters::ack::command_runner::ShellCommandRunner;
use adapters::agents::InMemoryAgentBindingRepository;
use adapters::mcp::grpc::GrpcBridge;
use adapters::mcp::repo_file::FileMcpSessionRepository;
use adapters::mcp::tls::{load_tls_snapshot, snapshots_equal, TlsPaths, TlsSnapshot};
use adapters::termbridge::{
    default_clipboard_backends, AlacrittyAdapter, CommandExecutor, FileTermBridgeStateRepository,
    ITerm2Adapter, InMemoryTermBridgeBindingRepository, KittyAdapter, KonsoleAdapter,
    SystemCommandExecutor, TilixAdapter, WezTermAdapter, WindowsTerminalAdapter,
};
use anyhow::{anyhow, Context, Result as AnyResult};
use app::ack::approvals::{ApprovalRegistry, ApprovalStatus, PendingApproval};
use app::ack::model::{EventRecord, ExecArgs, ExecRequest, UndoRequest};
use app::ack::service::{AckError, AckService};
use app::agents::AgentBindingService;
use app::mcp::service::{McpBridgeError, McpBridgeService};
use app::termbridge::{
    spawn_discovery_task, ClipboardBridgeService, TermBridgeDiscoveryHandle, TermBridgeService,
    TermBridgeServiceError,
};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::Utc;
use config::CACHE_DIR;
use continuum::ContinuumStore;
use dirs::config_dir;
use domain::agents::{
    AgentBinding, AgentProvider, BindingStatus, CapabilityName, SdkChannel, SdkVersion,
};
use domain::mcp::{McpSession, SessionStatus};
use domain::termbridge::{
    CapabilityRecord, ClipboardBackendDescriptor, ClipboardChannel, ClipboardContent,
    ClipboardMime, CurrentWorkingDirectory, TermBridgeState, TerminalBinding as TermBridgeBinding,
    TerminalBindingId, TerminalCapabilities, TerminalId,
};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use policy_engine::{PolicyEngine, TermBridgePolicyInput};
use ports::termbridge::{
    ClipboardBackend, ClipboardError, ClipboardReadRequest, ClipboardServiceError,
    ClipboardWriteRequest, SpawnRequest, TermBridgeCommandRequest, TerminalControlPort,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{env, thread};
use tokio::fs;
use tokio::net::TcpListener;
use tokio::signal::ctrl_c;
use tokio::sync::{broadcast, mpsc, watch, RwLock};
use tokio::time::{Duration, Instant};
use tonic::transport::Server;
use tracing::{info, warn};

type AgentBridgeService = AgentBindingService<InMemoryAgentBindingRepository>;
type TermBridgeServiceType =
    TermBridgeService<FileTermBridgeStateRepository, InMemoryTermBridgeBindingRepository>;
type ClipboardBridgeServiceType = ClipboardBridgeService;

const DEFAULT_SIGMA_SPOOL_MAX_BYTES: u64 = 1_048_576;
const DEFAULT_AGENT_BINDINGS: &[(&str, &str, &str, &[&str])] = &[
    (
        "openai",
        "1.2.0",
        "stable",
        &["agent.exec", "fs.read", "telemetry.push"],
    ),
    (
        "claude",
        "1.1.0",
        "stable",
        &["agent.exec", "fs.read", "telemetry.push"],
    ),
    (
        "microsoft",
        "1.0.0",
        "preview",
        &["agent.exec", "fs.read", "telemetry.push", "persona.sync"],
    ),
];

#[derive(Clone)]
struct AppState {
    ack_service: Arc<AckService<ShellCommandRunner>>,
    mcp_service: Arc<McpBridgeService<AckService<ShellCommandRunner>, FileMcpSessionRepository>>,
    agent_service: Arc<AgentBridgeService>,
    termbridge_service: Arc<TermBridgeServiceType>,
    termbridge_discovery: TermBridgeDiscoveryHandle,
    clipboard_service: Arc<ClipboardBridgeServiceType>,
    policy_engine: Arc<Mutex<PolicyEngine>>,
    approvals: Arc<ApprovalRegistry>,
    listen: SocketAddr,
    grpc_listen: SocketAddr,
    grpc_tls_policy: CipherPolicy,
    state_dir: PathBuf,
    started_at: Instant,
    metrics: Option<Arc<telemetry::PrismMetrics>>,
    tls_status: Arc<RwLock<TlsStatusReport>>,
}

impl AppState {
    fn new(
        listen: SocketAddr,
        grpc_listen: SocketAddr,
        state_dir: PathBuf,
        grpc_tls_policy: CipherPolicy,
        policy_path: Option<PathBuf>,
        metrics: Option<Arc<telemetry::PrismMetrics>>,
    ) -> anyhow::Result<Self> {
        let journal_path = state_dir.join("journal").join("continuum.log");
        let policy_engine = PolicyEngine::new(policy_path.as_deref()).unwrap_or_else(|e| {
            warn!("Failed to load policy engine: {e}. Policy enforcement disabled.");
            PolicyEngine::new(None).expect("creating disabled policy engine")
        });

        let policy_engine = Arc::new(Mutex::new(policy_engine));
        let continuum_store = Arc::new(tokio::sync::Mutex::new({
            let snapshot_dir = state_dir.join("snapshots");
            ContinuumStore::new(
                state_dir.join("journal").join("continuum.log"),
                snapshot_dir,
            )
        }));
        let approvals = Arc::new(ApprovalRegistry::new(&state_dir)?);
        let command_runner = Arc::new(ShellCommandRunner::new());
        let ack_service = Arc::new(AckService::new(
            policy_engine.clone(),
            continuum_store,
            journal_path.clone(),
            command_runner,
            metrics.clone(),
            approvals.clone(),
        ));

        let session_store = state_dir.join("mcp_sessions.json");
        let repo = Arc::new(
            FileMcpSessionRepository::new(session_store)
                .context("initializing MCP session store")?,
        );

        let agent_repo = Arc::new(InMemoryAgentBindingRepository::new());
        let agent_service = Arc::new(AgentBindingService::new(agent_repo));

        let termbridge_state_repo = Arc::new(FileTermBridgeStateRepository::new(
            state_dir.join("termbridge").join("capabilities.json"),
        ));
        let termbridge_binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let termbridge_adapters: Vec<Arc<dyn TerminalControlPort>> = vec![
            Arc::new(KittyAdapter::new()),
            Arc::new(WezTermAdapter::new()),
            Arc::new(WindowsTerminalAdapter::new()),
            Arc::new(AlacrittyAdapter::new()),
            Arc::new(KonsoleAdapter::new()),
            Arc::new(TilixAdapter::new()),
            Arc::new(ITerm2Adapter::new()),
        ];
        let termbridge_service = Arc::new(TermBridgeService::new(
            termbridge_state_repo,
            termbridge_binding_repo,
            termbridge_adapters,
            metrics.clone(),
        ));
        let discovery_handle =
            spawn_discovery_task(termbridge_service.clone(), metrics.clone(), None);
        discovery_handle.notify_refresh("bootstrap");

        let mcp_service = Arc::new(McpBridgeService::new(
            repo,
            ack_service.clone(),
            Some(discovery_handle.clone()),
        ));

        let clipboard_executor: Arc<dyn CommandExecutor> = Arc::new(SystemCommandExecutor);
        let clipboard_backends_raw = default_clipboard_backends(Arc::clone(&clipboard_executor));
        let clipboard_backends: Vec<Arc<dyn ClipboardBackend>> = clipboard_backends_raw
            .into_iter()
            .map(|backend| backend as Arc<dyn ClipboardBackend>)
            .collect();
        if clipboard_backends.is_empty() {
            warn!("Clipboard bridge: no platform backends detected");
        }
        let clipboard_service = Arc::new(ClipboardBridgeService::new(
            clipboard_backends,
            metrics.clone(),
            None,
        ));

        Ok(Self {
            ack_service,
            mcp_service,
            agent_service,
            termbridge_service,
            termbridge_discovery: discovery_handle,
            clipboard_service,
            policy_engine,
            listen,
            grpc_listen,
            grpc_tls_policy,
            state_dir,
            started_at: Instant::now(),
            metrics,
            tls_status: Arc::new(RwLock::new(TlsStatusReport::disabled())),
            approvals,
        })
    }

    #[cfg(test)]
    fn for_termbridge_test(
        termbridge_service: Arc<TermBridgeServiceType>,
        state_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        use crate::adapters::mcp::repo_file::FileMcpSessionRepository;
        use crate::ports::termbridge::ClipboardBackend;
        use std::fs;

        fs::create_dir_all(state_dir.join("journal"))?;
        fs::create_dir_all(state_dir.join("snapshots"))?;

        let journal_path = state_dir.join("journal").join("continuum.log");
        let policy_engine = PolicyEngine::new(None)?;
        let policy_engine = Arc::new(Mutex::new(policy_engine));
        let continuum_store = Arc::new(tokio::sync::Mutex::new(ContinuumStore::new(
            journal_path.clone(),
            state_dir.join("snapshots"),
        )));
        let approvals = Arc::new(ApprovalRegistry::new(&state_dir)?);
        let command_runner = Arc::new(ShellCommandRunner::new());
        let ack_service = Arc::new(AckService::new(
            policy_engine.clone(),
            continuum_store,
            journal_path,
            command_runner,
            None,
            approvals.clone(),
        ));

        let session_store = state_dir.join("mcp_sessions.json");
        let repo = Arc::new(FileMcpSessionRepository::new(session_store)?);
        let discovery_handle = spawn_discovery_task(
            termbridge_service.clone(),
            None,
            Some(Duration::from_secs(86400)),
        );
        let mcp_service = Arc::new(McpBridgeService::new(
            repo,
            ack_service.clone(),
            Some(discovery_handle.clone()),
        ));

        let agent_repo = Arc::new(InMemoryAgentBindingRepository::new());
        let agent_service = Arc::new(AgentBindingService::new(agent_repo));

        let clipboard_backends: Vec<Arc<dyn ClipboardBackend>> = Vec::new();
        let clipboard_service =
            Arc::new(ClipboardBridgeService::new(clipboard_backends, None, None));

        Ok(Self {
            ack_service,
            mcp_service,
            agent_service,
            termbridge_service,
            termbridge_discovery: discovery_handle,
            clipboard_service,
            policy_engine,
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            grpc_listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            grpc_tls_policy: CipherPolicy::Balanced,
            state_dir,
            started_at: Instant::now(),
            metrics: None,
            tls_status: Arc::new(RwLock::new(TlsStatusReport::disabled())),
            approvals,
        })
    }

    fn ack(&self) -> Arc<AckService<ShellCommandRunner>> {
        self.ack_service.clone()
    }

    fn mcp(
        &self,
    ) -> Arc<McpBridgeService<AckService<ShellCommandRunner>, FileMcpSessionRepository>> {
        self.mcp_service.clone()
    }

    fn agent_service(&self) -> Arc<AgentBridgeService> {
        self.agent_service.clone()
    }

    fn journal_path(&self) -> &Path {
        self.ack_service.journal_path()
    }

    async fn append_event(&self, event: &EventRecord) -> anyhow::Result<()> {
        self.ack_service.append_event(event).await
    }

    fn policy_engine(&self) -> Arc<Mutex<PolicyEngine>> {
        self.policy_engine.clone()
    }

    fn metrics(&self) -> Option<Arc<telemetry::PrismMetrics>> {
        self.metrics.clone()
    }

    fn termbridge(&self) -> Arc<TermBridgeServiceType> {
        self.termbridge_service.clone()
    }

    fn termbridge_discovery(&self) -> TermBridgeDiscoveryHandle {
        self.termbridge_discovery.clone()
    }

    fn clipboard(&self) -> Arc<ClipboardBridgeServiceType> {
        self.clipboard_service.clone()
    }

    fn approvals(&self) -> Arc<ApprovalRegistry> {
        self.approvals.clone()
    }

    fn tls_status(&self) -> Arc<RwLock<TlsStatusReport>> {
        self.tls_status.clone()
    }

    fn listen(&self) -> SocketAddr {
        self.listen
    }

    fn grpc_listen(&self) -> SocketAddr {
        self.grpc_listen
    }

    fn grpc_tls_policy(&self) -> CipherPolicy {
        self.grpc_tls_policy
    }

    fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    fn uptime(&self) -> Duration {
        Instant::now().saturating_duration_since(self.started_at)
    }
}

struct TlsWatchGuard {
    stop_tx: std::sync::mpsc::Sender<()>,
    watcher_handle: Option<std::thread::JoinHandle<()>>,
    update_task: tokio::task::JoinHandle<()>,
}

impl TlsWatchGuard {
    async fn shutdown(self) {
        let TlsWatchGuard {
            stop_tx,
            watcher_handle,
            update_task,
        } = self;

        let _ = stop_tx.send(());

        if let Some(handle) = watcher_handle {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = handle.join();
            })
            .await;
        }

        let _ = update_task.await;
    }
}

fn ack_error_to_api(command: &str, err: AckError) -> ApiError {
    match err {
        AckError::PolicyDenied { reason } => ApiError::forbidden("policy_denied", reason),
        AckError::Invalid(message) => ApiError::invalid("invalid_request", message),
        AckError::Internal(message) => {
            let code = match command {
                "agent.exec" => "exec_failed",
                "agent.undo" => "undo_failed",
                _ => "internal_error",
            };
            ApiError::internal(code, anyhow!(message))
        }
    }
}

#[derive(Serialize)]
struct StatusResponse {
    version: &'static str,
    uptime_ms: u128,
    active_sessions: usize,
    journal_path: String,
    sigma: SigmaSpoolInfo,
    agents: Vec<AgentBindingSummary>,
    telemetry_ready: bool,
    tls: TlsStatusReport,
    termbridge: TermBridgeStatus,
}

#[derive(Clone, Debug, Default, Serialize)]
struct TlsStatusReport {
    enabled: bool,
    policy: Option<String>,
    fingerprint: Option<String>,
    ca_fingerprint: Option<String>,
    client_auth_required: bool,
    tls_versions: Vec<String>,
    last_reload_latency_ms: Option<f64>,
    last_reload_at: Option<String>,
    last_error: Option<String>,
    last_error_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
struct TermBridgeStatus {
    enabled: bool,
    last_discovery_at: Option<String>,
    terminals: Vec<TermBridgeTerminalStatus>,
    clipboard_backends: Vec<ClipboardBackendStatusDto>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct TermBridgeTerminalStatus {
    terminal: String,
    display_name: String,
    requires_opt_in: bool,
    capabilities: TerminalCapabilitiesDto,
    notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct TerminalCapabilitiesDto {
    spawn: bool,
    split: bool,
    focus: bool,
    send_text: bool,
    clipboard_write: bool,
    clipboard_read: bool,
    cwd_sync: bool,
    bracketed_paste: bool,
    max_clipboard_kb: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
struct ClipboardBackendStatusDto {
    id: String,
    channels: Vec<String>,
    can_read: bool,
    can_write: bool,
    notes: Vec<String>,
}

impl TlsStatusReport {
    fn disabled() -> Self {
        Self::default()
    }

    fn success(policy: CipherPolicy, snapshot: &TlsSnapshot, latency_ms: f64) -> Self {
        Self {
            enabled: true,
            policy: Some(policy.to_string()),
            fingerprint: Some(snapshot.certificate_fingerprint_sha256.clone()),
            ca_fingerprint: snapshot.ca_fingerprint_sha256.clone(),
            client_auth_required: snapshot.client_auth_required,
            tls_versions: snapshot.tls_versions.clone(),
            last_reload_latency_ms: Some(latency_ms),
            last_reload_at: Some(Utc::now().to_rfc3339()),
            last_error: None,
            last_error_at: None,
        }
    }

    fn register_failure(&mut self, policy: CipherPolicy, reason: String) {
        self.enabled = true;
        self.policy = Some(policy.to_string());
        self.last_error = Some(reason);
        self.last_error_at = Some(Utc::now().to_rfc3339());
    }
}

impl TermBridgeStatus {
    fn from_state(state: TermBridgeState, clipboard: Vec<ClipboardBackendDescriptor>) -> Self {
        let last_discovery_at = state.discovered_at().map(|ts| ts.to_rfc3339());
        let terminals: Vec<TermBridgeTerminalStatus> = state
            .capabilities()
            .into_iter()
            .map(TermBridgeTerminalStatus::from)
            .collect();
        let clipboard_backends = clipboard
            .into_iter()
            .map(ClipboardBackendStatusDto::from)
            .collect::<Vec<_>>();
        Self {
            enabled: !terminals.is_empty() || !clipboard_backends.is_empty(),
            last_discovery_at,
            terminals,
            clipboard_backends,
            error: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            enabled: false,
            last_discovery_at: None,
            terminals: Vec::new(),
            clipboard_backends: Vec::new(),
            error: Some(message.into()),
        }
    }
}

impl From<CapabilityRecord> for TermBridgeTerminalStatus {
    fn from(record: CapabilityRecord) -> Self {
        Self {
            terminal: record.terminal.to_string(),
            display_name: record.display_name,
            requires_opt_in: record.requires_opt_in,
            notes: record.notes,
            capabilities: TerminalCapabilitiesDto::from(record.capabilities),
        }
    }
}

impl From<ClipboardBackendDescriptor> for ClipboardBackendStatusDto {
    fn from(descriptor: ClipboardBackendDescriptor) -> Self {
        let channels = descriptor
            .channels
            .iter()
            .map(|channel| channel.as_str().to_string())
            .collect();
        Self {
            id: descriptor.id,
            channels,
            can_read: descriptor.can_read,
            can_write: descriptor.can_write,
            notes: descriptor.notes,
        }
    }
}

impl From<TerminalCapabilities> for TerminalCapabilitiesDto {
    fn from(value: TerminalCapabilities) -> Self {
        Self {
            spawn: value.spawn,
            split: value.split,
            focus: value.focus,
            send_text: value.send_text,
            clipboard_write: value.clipboard_write,
            clipboard_read: value.clipboard_read,
            cwd_sync: value.cwd_sync,
            bracketed_paste: value.bracketed_paste,
            max_clipboard_kb: value.max_clipboard_kb,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize)]
struct SigmaSpoolInfo {
    enabled: bool,
    path: Option<String>,
    max_bytes: Option<u64>,
}

#[derive(Serialize)]
struct AgentBindingSummary {
    id: String,
    provider: String,
    version: String,
    channel: String,
    status: String,
    capabilities: Vec<String>,
    last_heartbeat_at: Option<String>,
    registered_at: String,
}

impl From<AgentBinding> for AgentBindingSummary {
    fn from(binding: AgentBinding) -> Self {
        let capabilities = binding
            .capabilities()
            .iter()
            .map(|cap| cap.as_str().to_string())
            .collect::<Vec<_>>();
        Self {
            id: binding.id().to_string(),
            provider: binding.provider().to_string(),
            version: binding.sdk_version().to_string(),
            channel: binding.channel().to_string(),
            status: binding.status().to_string(),
            capabilities,
            last_heartbeat_at: binding.last_heartbeat_at().map(|ts| ts.to_rfc3339()),
            registered_at: binding.registered_at().to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
struct TermBridgeCapabilitiesResponse {
    last_discovery_at: Option<String>,
    terminals: Vec<TermBridgeTerminalStatus>,
    clipboard_backends: Vec<ClipboardBackendStatusDto>,
}

#[derive(Debug, Serialize)]
struct TermBridgeBindingsResponse {
    bindings: Vec<TermBridgeBindingSummary>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TermBridgeClipboardWriteRequest {
    text: Option<String>,
    base64: Option<String>,
    mime: Option<String>,
    channel: Option<String>,
    backend: Option<String>,
}

#[derive(Debug, Serialize)]
struct TermBridgeClipboardWriteResponse {
    backend: String,
    bytes: usize,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TermBridgeClipboardReadRequest {
    channel: Option<String>,
    backend: Option<String>,
    as_base64: Option<bool>,
}

#[derive(Debug, Serialize)]
struct TermBridgeClipboardReadResponse {
    backend: String,
    bytes: usize,
    text: Option<String>,
    base64: Option<String>,
    mime: String,
}

#[derive(Debug, Serialize)]
struct TermBridgeBindingSummary {
    id: String,
    terminal: String,
    token: String,
    labels: HashMap<String, String>,
    created_at: String,
    ipc_endpoint: Option<String>,
}

impl From<TermBridgeBinding> for TermBridgeBindingSummary {
    fn from(binding: TermBridgeBinding) -> Self {
        Self {
            id: binding.id.to_string(),
            terminal: binding.terminal.to_string(),
            token: binding.token.clone(),
            labels: binding.labels.into_iter().collect(),
            created_at: binding.created_at.to_rfc3339(),
            ipc_endpoint: binding.ipc_endpoint.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TermBridgeSendTextRequest {
    binding_id: String,
    payload: String,
    #[serde(default = "default_true")]
    bracketed_paste: bool,
}

#[derive(Debug, Deserialize)]
struct TermBridgeCwdUpdateRequest {
    binding_id: String,
    cwd: String,
}

#[derive(Debug, Deserialize)]
struct TermBridgeFocusRequest {
    binding_id: String,
}

#[derive(Debug, Deserialize)]
struct TermBridgeSpawnRequest {
    terminal: String,
    command: Option<String>,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
}

#[derive(Serialize)]
struct ContextFullResponse {
    version: &'static str,
    uptime_ms: u128,
    persona_env: Option<String>,
    endpoints: ContextEndpoints,
    paths: ContextPaths,
    sigma: SigmaSpoolInfo,
    sessions: Vec<ContextSession>,
    agents: Vec<AgentBindingSummary>,
    telemetry_ready: bool,
    termbridge: TermBridgeStatus,
}

#[derive(Serialize)]
struct ContextEndpoints {
    http: EndpointInfo,
    grpc: GrpcContextInfo,
}

#[derive(Serialize)]
struct EndpointInfo {
    listen: String,
}

#[derive(Serialize)]
struct GrpcContextInfo {
    listen: String,
    tls_policy: String,
}

#[derive(Serialize)]
struct ContextPaths {
    state_dir: String,
    journal: String,
    mcp_sessions: String,
}

#[derive(Serialize)]
struct ContextSession {
    id: String,
    persona: String,
    status: SessionStatus,
    protocol_version: Option<String>,
    capabilities: Vec<String>,
    created_at: String,
    last_active_at: String,
}

#[derive(Serialize)]
struct DiscoveryDocument {
    version: u8,
    build: &'static str,
    generated_at: String,
    persona_env: Option<String>,
    endpoints: DiscoveryEndpoints,
    paths: DiscoveryPaths,
    sigma: SigmaSpoolInfo,
    agents: Vec<AgentBindingSummary>,
    telemetry_ready: bool,
    termbridge: TermBridgeStatus,
}

#[derive(Serialize)]
struct DiscoveryEndpoints {
    http: EndpointInfo,
    grpc: DiscoveryGrpcInfo,
}

#[derive(Serialize)]
struct DiscoveryGrpcInfo {
    listen: String,
    tls_policy: String,
    cert: Option<String>,
    key: Option<String>,
    ca: Option<String>,
}

#[derive(Serialize)]
struct DiscoveryPaths {
    state_dir: String,
    journal: String,
    mcp_sessions: String,
}

async fn collect_agent_summaries(service: Arc<AgentBridgeService>) -> Vec<AgentBindingSummary> {
    match service.list_bindings().await {
        Ok(bindings) => bindings
            .into_iter()
            .map(AgentBindingSummary::from)
            .collect(),
        Err(err) => {
            warn!("agent binding listing failed: {err}");
            Vec::new()
        }
    }
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let sessions = state.mcp().list_sessions().await;
    let sigma = sigma_spool_info();
    let agents = collect_agent_summaries(state.agent_service()).await;
    let tls = state.tls_status().read().await.clone();
    let clipboard_backends: Vec<ClipboardBackendDescriptor> = state.clipboard().list_backends();
    let termbridge_status = match state.termbridge().snapshot().await {
        Ok(snapshot) => TermBridgeStatus::from_state(snapshot, clipboard_backends),
        Err(err) => {
            warn!(%err, "termbridge snapshot failed");
            TermBridgeStatus::error(err.to_string())
        }
    };
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION"),
        uptime_ms: state.uptime().as_millis(),
        active_sessions: sessions.len(),
        journal_path: state.journal_path().to_string_lossy().into_owned(),
        sigma,
        agents,
        telemetry_ready: state.metrics().is_some(),
        tls,
        termbridge: termbridge_status,
    })
}

async fn context_full(State(state): State<AppState>) -> Json<ContextFullResponse> {
    let sessions = state.mcp().list_sessions().await;
    let sigma = sigma_spool_info();
    let agents = collect_agent_summaries(state.agent_service()).await;
    let persona_env = env::var("SHELLDONE_PERSONA").ok();
    let session_summaries = sessions
        .into_iter()
        .map(|session| {
            let snapshot = session.clone().to_snapshot();
            ContextSession {
                id: snapshot.id.to_string(),
                persona: snapshot.persona.name().to_string(),
                status: snapshot.status,
                protocol_version: snapshot.protocol_version.clone(),
                capabilities: snapshot
                    .capabilities
                    .into_iter()
                    .map(|cap| cap.as_str().to_string())
                    .collect(),
                created_at: snapshot.created_at.to_rfc3339(),
                last_active_at: snapshot.last_active_at.to_rfc3339(),
            }
        })
        .collect();

    let endpoints = ContextEndpoints {
        http: EndpointInfo {
            listen: state.listen().to_string(),
        },
        grpc: GrpcContextInfo {
            listen: state.grpc_listen().to_string(),
            tls_policy: state.grpc_tls_policy().to_string(),
        },
    };

    let paths = ContextPaths {
        state_dir: state.state_dir().to_string_lossy().into_owned(),
        journal: state.journal_path().to_string_lossy().into_owned(),
        mcp_sessions: state
            .state_dir()
            .join("mcp_sessions.json")
            .to_string_lossy()
            .into_owned(),
    };

    let clipboard_backends: Vec<ClipboardBackendDescriptor> = state.clipboard().list_backends();
    let termbridge_status = match state.termbridge().snapshot().await {
        Ok(snapshot) => TermBridgeStatus::from_state(snapshot, clipboard_backends),
        Err(err) => {
            warn!(%err, "termbridge snapshot failed (context)");
            TermBridgeStatus::error(err.to_string())
        }
    };

    Json(ContextFullResponse {
        version: env!("CARGO_PKG_VERSION"),
        uptime_ms: state.uptime().as_millis(),
        persona_env,
        endpoints,
        paths,
        sigma,
        sessions: session_summaries,
        agents,
        telemetry_ready: state.metrics().is_some(),
        termbridge: termbridge_status,
    })
}

async fn write_discovery_file(settings: &Settings, state: &AppState) -> anyhow::Result<()> {
    let Some(mut dir) = config_dir() else {
        warn!("config_dir unavailable; skipping agentd discovery file");
        return Ok(());
    };

    dir.push("shelldone");
    fs::create_dir_all(&dir).await?;
    dir.push("agentd.json");

    let sigma = sigma_spool_info();
    let agents = collect_agent_summaries(state.agent_service()).await;
    let clipboard_backends: Vec<ClipboardBackendDescriptor> = state.clipboard().list_backends();
    let termbridge_status = match state.termbridge().snapshot().await {
        Ok(snapshot) => TermBridgeStatus::from_state(snapshot, clipboard_backends),
        Err(err) => {
            warn!(%err, "termbridge snapshot failed (discovery)");
            TermBridgeStatus::error(err.to_string())
        }
    };

    let document = DiscoveryDocument {
        version: 1,
        build: env!("CARGO_PKG_VERSION"),
        generated_at: Utc::now().to_rfc3339(),
        persona_env: env::var("SHELLDONE_PERSONA").ok(),
        endpoints: DiscoveryEndpoints {
            http: EndpointInfo {
                listen: settings.listen.to_string(),
            },
            grpc: DiscoveryGrpcInfo {
                listen: settings.grpc_listen.to_string(),
                tls_policy: settings.grpc_tls_policy.to_string(),
                cert: settings
                    .grpc_tls_cert
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                key: settings
                    .grpc_tls_key
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                ca: settings
                    .grpc_tls_ca
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
            },
        },
        paths: DiscoveryPaths {
            state_dir: state.state_dir().to_string_lossy().into_owned(),
            journal: state.journal_path().to_string_lossy().into_owned(),
            mcp_sessions: state
                .state_dir()
                .join("mcp_sessions.json")
                .to_string_lossy()
                .into_owned(),
        },
        sigma,
        agents,
        telemetry_ready: state.metrics().is_some(),
        termbridge: termbridge_status,
    };

    let serialized = serde_json::to_vec_pretty(&document)?;
    fs::write(&dir, serialized).await?;
    Ok(())
}

fn sigma_spool_info() -> SigmaSpoolInfo {
    if !sigma_spool_enabled() {
        return SigmaSpoolInfo {
            enabled: false,
            path: None,
            max_bytes: None,
        };
    }

    let path = sigma_spool_path().map(|p| p.to_string_lossy().into_owned());
    let max_bytes = Some(sigma_spool_max_bytes());
    SigmaSpoolInfo {
        enabled: true,
        path,
        max_bytes,
    }
}

fn sigma_spool_enabled() -> bool {
    !matches!(env::var("SHELLDONE_SIGMA_SPOOL").ok(), Some(ref v) if is_disabled_flag(v))
}

fn sigma_spool_path() -> Option<PathBuf> {
    if !sigma_spool_enabled() {
        return None;
    }
    Some(CACHE_DIR.join("sigma_guard_spool.jsonl"))
}

fn sigma_spool_max_bytes() -> u64 {
    env::var("SHELLDONE_SIGMA_SPOOL_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SIGMA_SPOOL_MAX_BYTES)
}

fn is_disabled_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "disable" | "disabled"
    )
}

async fn seed_default_agent_bindings(service: Arc<AgentBridgeService>) -> anyhow::Result<()> {
    let existing = service
        .list_bindings()
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    if !existing.is_empty() {
        return Ok(());
    }

    for (provider, version, channel, capabilities) in DEFAULT_AGENT_BINDINGS {
        let provider = provider.parse::<AgentProvider>().map_err(|e| anyhow!(e))?;
        let sdk_version = SdkVersion::new(*version).map_err(|e| anyhow!(e))?;
        let channel = channel.parse::<SdkChannel>().map_err(|e| anyhow!(e))?;
        let caps = capabilities
            .iter()
            .map(|cap| CapabilityName::new(*cap).map_err(|e| anyhow!(e)))
            .collect::<AnyResult<Vec<_>>>()?;

        let (binding, _) = service
            .register_binding(provider, sdk_version, channel, caps)
            .await
            .map_err(|e| anyhow!(e.to_string()))?;
        let binding_id = binding.id();
        if let Ok(required) = CapabilityName::new("agent.exec") {
            if !binding.capabilities().contains(&required) {
                warn!(
                    "Agent binding {} missing required capability agent.exec",
                    binding.provider()
                );
            }
        }
        let capability_count = binding.capabilities().len();
        service
            .activate_binding(&binding_id)
            .await
            .map_err(|e| anyhow!(e.to_string()))?;
        info!(
            "Registered default agent binding {} (capabilities={})",
            binding.provider(),
            capability_count
        );
    }

    Ok(())
}

async fn apply_agent_overrides(service: Arc<AgentBridgeService>) -> anyhow::Result<()> {
    let disabled = parse_env_list("SHELLDONE_AGENT_DISABLE");
    let purge = parse_env_list("SHELLDONE_AGENT_PURGE");

    let bindings = service
        .list_bindings()
        .await
        .map_err(|e| anyhow!(e.to_string()))?;

    for binding in bindings {
        let binding_id = binding.id();
        let provider_slug = binding.provider().to_string();
        let status = binding.status().clone();

        if purge.contains(&provider_slug) {
            service
                .remove_binding(&binding_id)
                .await
                .map_err(|e| anyhow!(e.to_string()))?;
            info!("Agent binding {} purged via override", provider_slug);
            continue;
        }

        if disabled.contains(&provider_slug) {
            service
                .deactivate_binding(&binding_id)
                .await
                .map_err(|e| anyhow!(e.to_string()))?;
            info!("Agent binding {} disabled via override", provider_slug);
            continue;
        }

        if let Some(capabilities) = capability_override_env(&provider_slug)? {
            service
                .set_capabilities(&binding_id, capabilities)
                .await
                .map_err(|e| anyhow!(e.to_string()))?;
            info!("Agent binding {} capabilities overridden", provider_slug);
        }

        if matches!(status, BindingStatus::Active) {
            if let Err(err) = service.record_heartbeat(&binding_id).await {
                warn!(
                    "Agent binding heartbeat update failed for {}: {err}",
                    provider_slug
                );
            }
        }
    }

    let active = service
        .list_active()
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    info!("Active agent bindings after overrides: {}", active.len());
    Ok(())
}

fn capability_override_env(provider_slug: &str) -> AnyResult<Option<Vec<CapabilityName>>> {
    let key = format!(
        "SHELLDONE_AGENT_CAPABILITIES_{}",
        provider_slug.to_ascii_uppercase()
    );
    match env::var(&key) {
        Ok(value) => {
            let capabilities = value
                .split(',')
                .filter_map(|cap| {
                    let trimmed = cap.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                })
                .map(|cap| CapabilityName::new(cap).map_err(|e| anyhow!(e)))
                .collect::<AnyResult<Vec<_>>>()?;
            if capabilities.is_empty() {
                Ok(None)
            } else {
                Ok(Some(capabilities))
            }
        }
        Err(env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(anyhow!(format!("failed to read {key}: {err}"))),
    }
}

fn parse_env_list(key: &str) -> HashSet<String> {
    env::var(key)
        .unwrap_or_default()
        .split(',')
        .filter_map(|value| {
            let slug = value.trim().to_ascii_lowercase();
            if slug.is_empty() {
                None
            } else {
                Some(slug)
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub listen: SocketAddr,
    pub grpc_listen: SocketAddr,
    pub grpc_tls_cert: Option<PathBuf>,
    pub grpc_tls_key: Option<PathBuf>,
    pub grpc_tls_ca: Option<PathBuf>,
    pub grpc_tls_policy: CipherPolicy,
    pub state_dir: PathBuf,
    pub policy_path: Option<PathBuf>,
    pub otlp_endpoint: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 17717)),
            grpc_listen: SocketAddr::from(([127, 0, 0, 1], 17718)),
            grpc_tls_cert: None,
            grpc_tls_key: None,
            grpc_tls_ca: None,
            grpc_tls_policy: CipherPolicy::Balanced,
            state_dir: PathBuf::from("state"),
            policy_path: Some(PathBuf::from("policies/default.rego")),
            otlp_endpoint: None,
        }
    }
}

pub async fn run(settings: Settings) -> anyhow::Result<()> {
    // Initialize Prism OTLP telemetry if endpoint provided
    let (metrics, provider) = if let Some(ref endpoint) = settings.otlp_endpoint {
        let (provider, metrics) =
            telemetry::init_prism(Some(endpoint.clone()), "shelldone-agentd")?;
        (Some(Arc::new(metrics)), Some(provider))
    } else {
        (None, None)
    };

    let state = AppState::new(
        settings.listen,
        settings.grpc_listen,
        settings.state_dir.clone(),
        settings.grpc_tls_policy,
        settings.policy_path.clone(),
        metrics,
    )?;
    fs::create_dir_all(
        state
            .journal_path()
            .parent()
            .unwrap_or_else(|| Path::new(".")),
    )
    .await?;

    seed_default_agent_bindings(state.agent_service()).await?;
    apply_agent_overrides(state.agent_service()).await?;

    if let Err(err) = state.termbridge().snapshot().await {
        warn!(%err, "initial termbridge snapshot failed");
    }

    write_discovery_file(&settings, &state).await?;

    let policy_engine = state.policy_engine();

    let tls_paths = match (
        settings.grpc_tls_cert.clone(),
        settings.grpc_tls_key.clone(),
    ) {
        (Some(cert), Some(key)) => Some(TlsPaths {
            cert,
            key,
            ca: settings.grpc_tls_ca.clone(),
        }),
        (None, None) => None,
        (Some(_), None) | (None, Some(_)) => {
            return Err(anyhow!(
                "Both --grpc-tls-cert and --grpc-tls-key must be provided to enable TLS"
            ))
        }
    };

    if settings.grpc_tls_ca.is_some() && tls_paths.is_none() {
        return Err(anyhow!(
            "--grpc-tls-ca requires --grpc-tls-cert and --grpc-tls-key"
        ));
    }

    let (tls_sender, tls_receiver) = watch::channel::<Option<Arc<TlsSnapshot>>>(None);
    let mut tls_watch_guard: Option<TlsWatchGuard> = None;

    if let Some(paths) = tls_paths.clone() {
        let initial_snapshot = tokio::task::spawn_blocking({
            let paths = paths.clone();
            let policy_engine = policy_engine.clone();
            let policy = settings.grpc_tls_policy;
            let listener = settings.grpc_listen;
            move || load_tls_snapshot(&paths, policy, listener, &policy_engine)
        })
        .await??;
        let initial_snapshot = Arc::new(initial_snapshot);
        if let Some(metrics) = state.metrics() {
            metrics.record_tls_reload_success(
                &settings.grpc_tls_policy.to_string(),
                Some(&initial_snapshot.certificate_fingerprint_sha256),
                0.0,
            );
        }
        {
            let tls_state = state.tls_status();
            let mut status = tls_state.write().await;
            *status =
                TlsStatusReport::success(settings.grpc_tls_policy, initial_snapshot.as_ref(), 0.0);
        }
        if let Err(err) = state
            .append_event(&EventRecord::new(
                "tls.reload",
                None,
                json!({
                    "policy": settings.grpc_tls_policy.to_string(),
                    "fingerprint": initial_snapshot.certificate_fingerprint_sha256.clone(),
                    "ca_fingerprint": initial_snapshot.ca_fingerprint_sha256.clone(),
                    "client_auth_required": initial_snapshot.client_auth_required,
                    "tls_versions": initial_snapshot.tls_versions.clone(),
                    "latency_ms": 0.0,
                    "initial": true,
                }),
                None,
                Some("tls".to_string()),
                None,
            ))
            .await
        {
            tracing::warn!(%err, "failed to record initial TLS reload event");
        }
        tls_sender.send_replace(Some(initial_snapshot));

        let (event_tx, mut event_rx) = mpsc::channel::<()>(16);
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let watcher_paths = paths.clone();
        let event_tx_clone = event_tx.clone();
        let watcher_handle = thread::spawn(move || {
            let mut watcher = match RecommendedWatcher::new(
                move |res: Result<notify::Event, notify::Error>| match res {
                    Ok(event) => {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            let _ = event_tx_clone.try_send(());
                        }
                    }
                    Err(err) => {
                        tracing::error!(%err, "TLS watcher error");
                    }
                },
                notify::Config::default(),
            ) {
                Ok(w) => w,
                Err(err) => {
                    tracing::error!(%err, "failed to initialize TLS watcher");
                    return;
                }
            };

            for dir in watcher_paths.watch_dirs() {
                if let Err(err) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
                    tracing::error!(directory = %dir.display(), %err, "failed to watch TLS directory");
                }
            }

            let _ = stop_rx.recv();
        });

        let sender_clone = tls_sender.clone();
        let paths_for_task = paths.clone();
        let policy_engine_for_task = policy_engine.clone();
        let policy = settings.grpc_tls_policy;
        let listener = settings.grpc_listen;
        let state_for_task = state.clone();
        let update_task = tokio::spawn(async move {
            let debounce = Duration::from_millis(200);
            let mut last_reload = Instant::now() - debounce;
            while event_rx.recv().await.is_some() {
                let elapsed = last_reload.elapsed();
                if elapsed < debounce {
                    tokio::time::sleep(debounce - elapsed).await;
                }
                let reload_start = Instant::now();
                last_reload = reload_start;
                match tokio::task::spawn_blocking({
                    let paths = paths_for_task.clone();
                    let policy_engine = policy_engine_for_task.clone();
                    move || load_tls_snapshot(&paths, policy, listener, &policy_engine)
                })
                .await
                {
                    Ok(Ok(snapshot)) => {
                        let snapshot = Arc::new(snapshot);
                        let current = sender_clone.borrow().clone();
                        if !snapshots_equal(&current, &Some(snapshot.clone())) {
                            sender_clone.send_replace(Some(snapshot.clone()));
                            let latency_ms = reload_start.elapsed().as_secs_f64() * 1000.0;
                            let policy_label = policy.to_string();
                            tracing::info!(policy = %policy_label, latency_ms, "reloaded gRPC TLS materials");
                            {
                                let tls_state = state_for_task.tls_status();
                                let mut status = tls_state.write().await;
                                *status =
                                    TlsStatusReport::success(policy, snapshot.as_ref(), latency_ms);
                            }
                            if let Some(metrics) = state_for_task.metrics() {
                                metrics.record_tls_reload_success(
                                    &policy_label,
                                    Some(&snapshot.certificate_fingerprint_sha256),
                                    latency_ms,
                                );
                            }
                            if let Err(err) = state_for_task
                                .append_event(&EventRecord::new(
                                    "tls.reload",
                                    None,
                                    json!({
                                        "policy": policy_label,
                                        "fingerprint": snapshot.certificate_fingerprint_sha256.clone(),
                                        "ca_fingerprint": snapshot.ca_fingerprint_sha256.clone(),
                                        "client_auth_required": snapshot.client_auth_required,
                                        "tls_versions": snapshot.tls_versions.clone(),
                                        "latency_ms": latency_ms,
                                        "initial": false,
                                    }),
                                    None,
                                    Some("tls".to_string()),
                                    None,
                                ))
                                .await
                            {
                                tracing::warn!(%err, "failed to append tls.reload event");
                            }
                        }
                    }
                    Ok(Err(err)) => {
                        let policy_label = policy.to_string();
                        let reason = err.to_string();
                        tracing::error!(error = %reason, policy = %policy_label, "failed to reload gRPC TLS materials");
                        if let Some(metrics) = state_for_task.metrics() {
                            metrics.record_tls_reload_error(&policy_label, &reason);
                        }
                        {
                            let tls_state = state_for_task.tls_status();
                            let mut status = tls_state.write().await;
                            status.register_failure(policy, reason.clone());
                        }
                        if let Err(write_err) = state_for_task
                            .append_event(&EventRecord::new(
                                "tls.reload_error",
                                None,
                                json!({
                                    "policy": policy_label,
                                    "error": reason,
                                }),
                                None,
                                Some("tls".to_string()),
                                None,
                            ))
                            .await
                        {
                            tracing::warn!(%write_err, "failed to append tls.reload_error event");
                        }
                    }
                    Err(join_err) => {
                        let policy_label = policy.to_string();
                        let reason = join_err.to_string();
                        tracing::error!(error = %reason, policy = %policy_label, "TLS reload task panicked");
                        if let Some(metrics) = state_for_task.metrics() {
                            metrics.record_tls_reload_error(&policy_label, &reason);
                        }
                        {
                            let tls_state = state_for_task.tls_status();
                            let mut status = tls_state.write().await;
                            status.register_failure(policy, reason.clone());
                        }
                        if let Err(write_err) = state_for_task
                            .append_event(&EventRecord::new(
                                "tls.reload_error",
                                None,
                                json!({
                                    "policy": policy_label,
                                    "error": reason,
                                }),
                                None,
                                Some("tls".to_string()),
                                None,
                            ))
                            .await
                        {
                            tracing::warn!(%write_err, "failed to append tls.reload_error event");
                        }
                    }
                }
            }
        });

        tls_watch_guard = Some(TlsWatchGuard {
            stop_tx,
            watcher_handle: Some(watcher_handle),
            update_task,
        });
    } else {
        tls_sender.send_replace(None);
        {
            let tls_state = state.tls_status();
            let mut status = tls_state.write().await;
            *status = TlsStatusReport::disabled();
        }
    }

    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    let grpc_addr = settings.grpc_listen;
    info!("grpc_listen" = %grpc_addr, "msg" = "starting MCP gRPC bridge");
    let grpc_handle = tokio::spawn(manage_grpc_server(
        grpc_addr,
        state.mcp(),
        tls_receiver.clone(),
        shutdown_tx.subscribe(),
    ));

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/status", get(status))
        .route("/context/full", get(context_full))
        .route("/termbridge/capabilities", get(termbridge_capabilities))
        .route("/termbridge/discover", post(termbridge_discover))
        .route("/termbridge/bindings", get(termbridge_bindings))
        .route("/termbridge/spawn", post(termbridge_spawn))
        .route("/termbridge/send-text", post(termbridge_send_text))
        .route("/termbridge/focus", post(termbridge_focus))
        .route("/termbridge/cwd", post(termbridge_update_cwd))
        .route(
            "/termbridge/clipboard/write",
            post(termbridge_clipboard_write),
        )
        .route(
            "/termbridge/clipboard/read",
            post(termbridge_clipboard_read),
        )
        .route("/sigma/handshake", post(handshake))
        .route("/ack/exec", post(agent_exec))
        .route("/journal/event", post(journal_event))
        .route("/ack/undo", post(agent_undo))
        .route("/approvals/pending", get(list_pending_approvals))
        .route("/approvals/grant", post(grant_approval))
        .route("/mcp", get(mcp_ws_upgrade))
        .with_state(state.clone());

    let listener = TcpListener::bind(settings.listen).await?;
    info!("listening" = %settings.listen, "state_dir" = %settings.state_dir.display(), "msg" = "shelldone-agentd started");

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown({
            let shutdown_tx = shutdown_tx.clone();
            async move {
                shutdown_signal().await;
                let _ = shutdown_tx.send(());
            }
        })
        .await?;

    let _ = shutdown_tx.send(());

    if let Err(err) = grpc_handle.await {
        warn!(%err, "MCP gRPC bridge task join error");
    }

    if let Some(guard) = tls_watch_guard {
        guard.shutdown().await;
    }

    // Graceful telemetry shutdown
    if let Some(provider) = provider {
        if let Err(e) = telemetry::shutdown_prism(provider) {
            warn!("Failed to shutdown telemetry: {}", e);
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReloadOutcome {
    Reload,
    Shutdown,
}

async fn manage_grpc_server(
    addr: SocketAddr,
    service: Arc<McpBridgeService<AckService<ShellCommandRunner>, FileMcpSessionRepository>>,
    mut tls_rx: watch::Receiver<Option<Arc<TlsSnapshot>>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> AnyResult<()> {
    loop {
        let current_snapshot = tls_rx.borrow().clone();
        let mut builder = Server::builder();
        if let Some(snapshot) = current_snapshot.clone() {
            builder = builder
                .tls_config(snapshot.as_server_tls_config())
                .context("invalid gRPC TLS configuration")?;
        }

        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let bridge = GrpcBridge::new(service.clone()).into_server();
        let server = builder
            .add_service(bridge)
            .serve_with_shutdown(addr, async move {
                let _ = stop_rx.await;
            });

        tokio::pin!(server);

        let mut server_finished = false;
        let mut server_result: Option<Result<(), tonic::transport::Error>> = None;

        let outcome = tokio::select! {
            res = &mut server => {
                server_finished = true;
                server_result = Some(res);
                ReloadOutcome::Shutdown
            }
            res = tls_rx.changed() => {
                if res.is_ok() {
                    ReloadOutcome::Reload
                } else {
                    ReloadOutcome::Shutdown
                }
            }
            res = shutdown_rx.recv() => {
                let _ = res;
                ReloadOutcome::Shutdown
            }
        };

        if server_finished {
            if let Some(result) = server_result {
                result.map_err(|err| anyhow!(err))?;
            }
        } else {
            let _ = stop_tx.send(());
            if let Err(err) = server.await {
                warn!(%err, "MCP gRPC bridge terminated");
            }
        }

        if outcome == ReloadOutcome::Reload {
            continue;
        } else {
            break;
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let _ = ctrl_c().await;
    info!("msg" = "shutdown signal received");
}

async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "timestamp": Utc::now().to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct HandshakeRequest {
    version: Option<u32>,
    capabilities: Option<HashMap<String, Value>>,
    persona: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct HandshakeResponse {
    accepted: Vec<CapabilityAck>,
    fallback: Vec<CapabilityAck>,
    persona: String,
}

#[derive(Debug, Serialize, Clone)]
struct CapabilityAck {
    name: String,
    value: Value,
}

async fn handshake(
    State(state): State<AppState>,
    Json(payload): Json<HandshakeRequest>,
) -> Result<Json<HandshakeResponse>, ApiError> {
    let response = negotiate(&payload);
    let event = EventRecord::new(
        "handshake",
        payload.persona.clone(),
        json!({
            "request": payload,
            "response": response.clone(),
        }),
        None,
        None,
        None,
    );
    state
        .append_event(&event)
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;
    Ok(Json(response))
}

async fn termbridge_capabilities(
    State(state): State<AppState>,
) -> Result<Json<TermBridgeCapabilitiesResponse>, ApiError> {
    let snapshot = state
        .termbridge()
        .snapshot()
        .await
        .map_err(|err| termbridge_service_error("termbridge_snapshot", err))?;
    let clipboard_backends: Vec<ClipboardBackendDescriptor> = state.clipboard().list_backends();
    let status = TermBridgeStatus::from_state(snapshot, clipboard_backends);
    Ok(Json(TermBridgeCapabilitiesResponse {
        last_discovery_at: status.last_discovery_at.clone(),
        terminals: status.terminals,
        clipboard_backends: status.clipboard_backends,
    }))
}

async fn termbridge_discover(
    State(state): State<AppState>,
) -> Result<Json<TermBridgeCapabilitiesResponse>, ApiError> {
    let snapshot = state
        .termbridge()
        .discover()
        .await
        .map_err(|err| termbridge_service_error("termbridge_discover", err))?;
    let clipboard_backends = state.clipboard().list_backends();
    let status = TermBridgeStatus::from_state(snapshot, clipboard_backends);
    Ok(Json(TermBridgeCapabilitiesResponse {
        last_discovery_at: status.last_discovery_at.clone(),
        terminals: status.terminals,
        clipboard_backends: status.clipboard_backends,
    }))
}

async fn termbridge_bindings(
    State(state): State<AppState>,
) -> Result<Json<TermBridgeBindingsResponse>, ApiError> {
    let bindings = state
        .termbridge()
        .list_bindings()
        .await
        .map_err(|err| termbridge_service_error("termbridge_bindings", err))?;
    let summaries = bindings
        .into_iter()
        .map(TermBridgeBindingSummary::from)
        .collect();
    Ok(Json(TermBridgeBindingsResponse {
        bindings: summaries,
    }))
}

async fn termbridge_spawn(
    State(state): State<AppState>,
    Json(req): Json<TermBridgeSpawnRequest>,
) -> Result<Json<TermBridgeBindingSummary>, ApiError> {
    if req.terminal.trim().is_empty() {
        return Err(ApiError::invalid(
            "invalid_terminal",
            "terminal field cannot be empty",
        ));
    }
    let terminal = TerminalId::new(req.terminal.clone());
    let env_map: BTreeMap<String, String> = req.env.unwrap_or_default().into_iter().collect();
    let request = SpawnRequest {
        terminal,
        command: req.command,
        cwd: req.cwd,
        env: env_map,
    };
    let persona = env::var("SHELLDONE_PERSONA").ok().filter(|v| !v.is_empty());
    let policy_input = TermBridgePolicyInput {
        action: "spawn".to_string(),
        persona: persona.clone(),
        terminal: Some(req.terminal.clone()),
        command: request.command.clone(),
        backend: None,
        channel: None,
        bytes: None,
        cwd: None,
    };

    let decision = {
        let engine = state.policy_engine();
        let guard = engine
            .lock()
            .map_err(|e| ApiError::internal("policy_lock", anyhow!(e.to_string())))?;
        guard
            .evaluate_termbridge(&policy_input)
            .map_err(|err| ApiError::internal("policy_eval", err))?
    };

    if let Some(metrics) = state.metrics() {
        metrics.record_policy_evaluation(decision.is_allowed());
        if !decision.is_allowed() {
            metrics.record_policy_denial("termbridge.spawn", persona.as_deref());
        }
    }

    if !decision.is_allowed() {
        warn!(
            "Policy denied termbridge.spawn terminal={} reasons={:?}",
            req.terminal, decision.deny_reasons
        );
        let denial_payload = json!({
            "action": "spawn",
            "terminal": req.terminal,
            "command": request.command,
            "cwd": request.cwd,
            "allowed": false,
            "reasons": decision.deny_reasons,
        });
        state
            .append_event(&EventRecord::new(
                "termbridge.spawn.denied",
                persona.clone(),
                denial_payload,
                None,
                Some("termbridge::policy".to_string()),
                None,
            ))
            .await
            .map_err(|err| ApiError::internal("journal_write", err))?;
        return Err(ApiError::forbidden(
            "policy_denied",
            "TermBridge spawn denied by policy",
        ));
    }

    let binding = state
        .termbridge()
        .spawn(request)
        .await
        .map_err(|err| termbridge_service_error("termbridge_spawn", err))?;
    let labels = serde_json::to_value(&binding.labels).unwrap_or(Value::Null);
    let event_payload = json!({
        "action": "spawn",
        "terminal": binding.terminal.as_str(),
        "binding_id": binding.id.to_string(),
        "token": binding.token,
        "labels": labels,
        "ipc_endpoint": binding.ipc_endpoint,
    });
    state
        .append_event(&EventRecord::new(
            "termbridge.spawn",
            persona,
            event_payload,
            None,
            Some("termbridge::action".to_string()),
            None,
        ))
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;

    state.termbridge_discovery().notify_refresh("binding_spawn");

    Ok(Json(TermBridgeBindingSummary::from(binding)))
}

async fn termbridge_send_text(
    State(state): State<AppState>,
    Json(req): Json<TermBridgeSendTextRequest>,
) -> Result<Json<Value>, ApiError> {
    let binding_id = TerminalBindingId::parse(&req.binding_id)
        .map_err(|err| ApiError::invalid("invalid_binding_id", err))?;
    let persona = env::var("SHELLDONE_PERSONA").ok().filter(|v| !v.is_empty());

    let binding = state
        .termbridge()
        .get_binding(&binding_id)
        .await
        .map_err(|err| termbridge_service_error("termbridge_get_binding", err))?
        .ok_or_else(|| ApiError::invalid("binding_not_found", "binding not found"))?;

    let payload_len = req.payload.len();
    let policy_input = TermBridgePolicyInput {
        action: "send_text".to_string(),
        persona: persona.clone(),
        terminal: Some(binding.terminal.as_str().to_string()),
        command: None,
        backend: None,
        channel: None,
        bytes: Some(payload_len),
        cwd: None,
    };

    let decision = {
        let engine = state.policy_engine();
        let guard = engine
            .lock()
            .map_err(|e| ApiError::internal("policy_lock", anyhow!(e.to_string())))?;
        guard
            .evaluate_termbridge(&policy_input)
            .map_err(|err| ApiError::internal("policy_eval", err))?
    };

    if let Some(metrics) = state.metrics() {
        metrics.record_policy_evaluation(decision.is_allowed());
        if !decision.is_allowed() {
            metrics.record_policy_denial("termbridge.send_text", persona.as_deref());
        }
    }

    if !decision.is_allowed() {
        warn!(
            "Policy denied termbridge.send_text binding={} terminal={} bytes={} reasons={:?}",
            binding.id,
            binding.terminal.as_str(),
            payload_len,
            decision.deny_reasons
        );
        let denial_payload = json!({
            "action": "send_text",
            "binding_id": binding.id.to_string(),
            "terminal": binding.terminal.as_str(),
            "bytes": payload_len,
            "bracketed": req.bracketed_paste,
            "allowed": false,
            "reasons": decision.deny_reasons,
        });
        state
            .append_event(&EventRecord::new(
                "termbridge.send_text.denied",
                persona.clone(),
                denial_payload,
                None,
                Some("termbridge::policy".to_string()),
                Some(payload_len),
            ))
            .await
            .map_err(|err| ApiError::internal("journal_write", err))?;
        return Err(ApiError::forbidden(
            "policy_denied",
            decision.deny_reasons.join("; "),
        ));
    }

    let request = TermBridgeCommandRequest {
        binding_id: Some(binding_id.clone()),
        terminal: None,
        payload: Some(req.payload.clone()),
        bracketed_paste: Some(req.bracketed_paste),
    };
    state
        .termbridge()
        .send_text(request)
        .await
        .map_err(|err| termbridge_service_error("termbridge_send_text", err))?;

    let labels = serde_json::to_value(&binding.labels).unwrap_or(Value::Null);
    let event_payload = json!({
        "action": "send_text",
        "binding_id": binding.id.to_string(),
        "terminal": binding.terminal.as_str(),
        "bytes": payload_len,
        "bracketed": req.bracketed_paste,
        "labels": labels,
    });
    state
        .append_event(&EventRecord::new(
            "termbridge.send_text",
            persona,
            event_payload,
            None,
            Some("termbridge::action".to_string()),
            Some(payload_len),
        ))
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;

    Ok(Json(json!({
        "status": "accepted",
        "bracketed_paste": req.bracketed_paste,
    })))
}

async fn termbridge_focus(
    State(state): State<AppState>,
    Json(req): Json<TermBridgeFocusRequest>,
) -> Result<Json<Value>, ApiError> {
    let binding_id = TerminalBindingId::parse(&req.binding_id)
        .map_err(|err| ApiError::invalid("invalid_binding_id", err))?;
    let persona = env::var("SHELLDONE_PERSONA").ok().filter(|v| !v.is_empty());

    let binding = state
        .termbridge()
        .get_binding(&binding_id)
        .await
        .map_err(|err| termbridge_service_error("termbridge_get_binding", err))?
        .ok_or_else(|| ApiError::invalid("binding_not_found", "binding not found"))?;

    let policy_input = TermBridgePolicyInput {
        action: "focus".to_string(),
        persona: persona.clone(),
        terminal: Some(binding.terminal.as_str().to_string()),
        command: None,
        backend: None,
        channel: None,
        bytes: None,
        cwd: None,
    };

    let decision = {
        let engine = state.policy_engine();
        let guard = engine
            .lock()
            .map_err(|e| ApiError::internal("policy_lock", anyhow!(e.to_string())))?;
        guard
            .evaluate_termbridge(&policy_input)
            .map_err(|err| ApiError::internal("policy_eval", err))?
    };

    if let Some(metrics) = state.metrics() {
        metrics.record_policy_evaluation(decision.is_allowed());
        if !decision.is_allowed() {
            metrics.record_policy_denial("termbridge.focus", persona.as_deref());
        }
    }

    if !decision.is_allowed() {
        warn!(
            "Policy denied termbridge.focus binding={} terminal={} reasons={:?}",
            binding.id,
            binding.terminal.as_str(),
            decision.deny_reasons
        );
        let labels = serde_json::to_value(&binding.labels).unwrap_or(Value::Null);
        let denial_payload = json!({
            "action": "focus",
            "binding_id": binding.id.to_string(),
            "terminal": binding.terminal.as_str(),
            "labels": labels,
            "allowed": false,
            "reasons": decision.deny_reasons,
        });
        state
            .append_event(&EventRecord::new(
                "termbridge.focus.denied",
                persona.clone(),
                denial_payload,
                None,
                Some("termbridge::policy".to_string()),
                None,
            ))
            .await
            .map_err(|err| ApiError::internal("journal_write", err))?;
        return Err(ApiError::forbidden(
            "policy_denied",
            decision.deny_reasons.join("; "),
        ));
    }

    state
        .termbridge()
        .focus(&binding_id)
        .await
        .map_err(|err| termbridge_service_error("termbridge_focus", err))?;

    let labels = serde_json::to_value(&binding.labels).unwrap_or(Value::Null);
    let event_payload = json!({
        "action": "focus",
        "binding_id": binding.id.to_string(),
        "terminal": binding.terminal.as_str(),
        "labels": labels,
    });
    state
        .append_event(&EventRecord::new(
            "termbridge.focus",
            persona,
            event_payload,
            None,
            Some("termbridge::action".to_string()),
            None,
        ))
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;

    Ok(Json(json!({
        "status": "accepted",
    })))
}

async fn termbridge_update_cwd(
    State(state): State<AppState>,
    Json(req): Json<TermBridgeCwdUpdateRequest>,
) -> Result<Json<TermBridgeBindingSummary>, ApiError> {
    let binding_id = TerminalBindingId::parse(&req.binding_id)
        .map_err(|err| ApiError::invalid("invalid_binding_id", err))?;
    let cwd = CurrentWorkingDirectory::new(req.cwd.clone())
        .map_err(|err| ApiError::invalid("invalid_cwd", err))?;
    let persona = env::var("SHELLDONE_PERSONA").ok().filter(|v| !v.is_empty());

    let existing_binding = state
        .termbridge()
        .get_binding(&binding_id)
        .await
        .map_err(|err| termbridge_service_error("termbridge_get_binding", err))?
        .ok_or_else(|| ApiError::not_found("binding_not_found", "binding not found"))?;
    let previous_cwd = existing_binding.cwd().map(|value| value.to_string());

    let policy_input = TermBridgePolicyInput {
        action: "cwd.update".to_string(),
        persona: persona.clone(),
        terminal: Some(existing_binding.terminal.as_str().to_string()),
        command: None,
        backend: None,
        channel: None,
        bytes: None,
        cwd: Some(cwd.as_str().to_string()),
    };

    let decision = {
        let engine = state.policy_engine();
        let guard = engine
            .lock()
            .map_err(|e| ApiError::internal("policy_lock", anyhow!(e.to_string())))?;
        guard
            .evaluate_termbridge(&policy_input)
            .map_err(|err| ApiError::internal("policy_eval", err))?
    };

    if let Some(metrics) = state.metrics() {
        metrics.record_policy_evaluation(decision.is_allowed());
        if !decision.is_allowed() {
            metrics.record_policy_denial("termbridge.cwd_update", persona.as_deref());
        }
    }

    if !decision.is_allowed() {
        warn!(
            "Policy denied termbridge.cwd_update binding={} terminal={} cwd={} reasons={:?}",
            existing_binding.id,
            existing_binding.terminal.as_str(),
            cwd.as_str(),
            decision.deny_reasons
        );
        let denial_payload = json!({
            "action": "cwd.update",
            "binding_id": existing_binding.id.to_string(),
            "terminal": existing_binding.terminal.as_str(),
            "cwd": cwd.as_str(),
            "allowed": false,
            "reasons": decision.deny_reasons,
        });
        state
            .append_event(&EventRecord::new(
                "termbridge.cwd_update.denied",
                persona.clone(),
                denial_payload,
                None,
                Some("termbridge::policy".to_string()),
                None,
            ))
            .await
            .map_err(|err| ApiError::internal("journal_write", err))?;
        return Err(ApiError::forbidden(
            "policy_denied",
            decision.deny_reasons.join("; "),
        ));
    }

    let updated_binding = state
        .termbridge()
        .update_cwd(&binding_id, cwd.clone())
        .await
        .map_err(|err| termbridge_service_error("termbridge_update_cwd", err))?;

    let summary = TermBridgeBindingSummary::from(updated_binding.clone());
    let labels = serde_json::to_value(&updated_binding.labels).unwrap_or(Value::Null);
    let event_payload = json!({
        "action": "cwd.update",
        "binding_id": updated_binding.id.to_string(),
        "terminal": updated_binding.terminal.as_str(),
        "cwd": cwd.as_str(),
        "previous_cwd": previous_cwd,
        "labels": labels,
    });
    state
        .append_event(&EventRecord::new(
            "termbridge.cwd_update",
            persona,
            event_payload,
            None,
            Some("termbridge::action".to_string()),
            None,
        ))
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;

    Ok(Json(summary))
}

async fn termbridge_clipboard_write(
    State(state): State<AppState>,
    Json(req): Json<TermBridgeClipboardWriteRequest>,
) -> Result<Json<TermBridgeClipboardWriteResponse>, ApiError> {
    let TermBridgeClipboardWriteRequest {
        text,
        base64,
        mime,
        channel,
        backend,
    } = req;

    let persona = env::var("SHELLDONE_PERSONA").ok().filter(|v| !v.is_empty());

    let parsed_mime = mime
        .map(|value| {
            ClipboardMime::new(value)
                .map_err(|err| ApiError::invalid("clipboard.invalid_mime", err))
        })
        .transpose()?
        .unwrap_or_else(ClipboardMime::text_plain_utf8);

    let channel = channel
        .as_deref()
        .map(|value| {
            value
                .parse::<ClipboardChannel>()
                .map_err(|err| ApiError::invalid("clipboard.invalid_channel", err))
        })
        .transpose()?
        .unwrap_or(ClipboardChannel::Clipboard);

    let content = if let Some(text) = text {
        ClipboardContent::new(text.into_bytes(), parsed_mime.clone())
            .map_err(|err| ApiError::invalid("clipboard.invalid_payload", err))?
    } else if let Some(encoded) = base64 {
        let bytes = BASE64_STANDARD
            .decode(encoded.as_bytes())
            .map_err(|err| ApiError::invalid("clipboard.invalid_base64", err.to_string()))?;
        ClipboardContent::new(bytes, parsed_mime.clone())
            .map_err(|err| ApiError::invalid("clipboard.invalid_payload", err))?
    } else {
        return Err(ApiError::invalid(
            "clipboard.missing_payload",
            "text or base64 field is required",
        ));
    };

    let payload_len = content.len();
    let requested_backend = backend.clone();
    let policy_input = TermBridgePolicyInput {
        action: "clipboard.write".to_string(),
        persona: persona.clone(),
        terminal: None,
        command: None,
        backend: requested_backend.clone(),
        channel: Some(channel.as_str().to_string()),
        bytes: Some(payload_len),
        cwd: None,
    };

    let decision = {
        let engine = state.policy_engine();
        let guard = engine
            .lock()
            .map_err(|e| ApiError::internal("policy_lock", anyhow!(e.to_string())))?;
        guard
            .evaluate_termbridge(&policy_input)
            .map_err(|err| ApiError::internal("policy_eval", err))?
    };

    if let Some(metrics) = state.metrics() {
        metrics.record_policy_evaluation(decision.is_allowed());
        if !decision.is_allowed() {
            metrics.record_policy_denial("termbridge.clipboard.write", persona.as_deref());
        }
    }

    if !decision.is_allowed() {
        warn!(
            "Policy denied termbridge.clipboard.write backend={:?} channel={} bytes={} reasons={:?}",
            requested_backend,
            channel.as_str(),
            payload_len,
            decision.deny_reasons
        );
        let denial_payload = json!({
            "action": "clipboard.write",
            "backend": requested_backend,
            "channel": channel.as_str(),
            "bytes": payload_len,
            "allowed": false,
            "reasons": decision.deny_reasons,
        });
        state
            .append_event(&EventRecord::new(
                "termbridge.clipboard.denied",
                persona.clone(),
                denial_payload,
                None,
                Some("termbridge::policy".to_string()),
                Some(payload_len),
            ))
            .await
            .map_err(|err| ApiError::internal("journal_write", err))?;
        return Err(ApiError::forbidden(
            "policy_denied",
            decision.deny_reasons.join("; "),
        ));
    }

    let mut request = ClipboardWriteRequest::new(content).with_channel(channel);
    if let Some(backend_id) = backend {
        request = request.with_backend(backend_id);
    }

    let clipboard = state.clipboard();
    let result = clipboard
        .write(request)
        .await
        .map_err(|err| clipboard_service_error("clipboard.write", err))?;

    let event_payload = json!({
        "action": "clipboard.write",
        "backend": result.backend_id,
        "requested_backend": requested_backend,
        "channel": channel.as_str(),
        "bytes": result.bytes,
        "mime": parsed_mime.as_str(),
        "allowed": true,
    });
    state
        .append_event(&EventRecord::new(
            "termbridge.clipboard.write",
            persona.clone(),
            event_payload,
            None,
            Some("termbridge::clipboard".to_string()),
            Some(result.bytes),
        ))
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;

    Ok(Json(TermBridgeClipboardWriteResponse {
        backend: result.backend_id,
        bytes: result.bytes,
    }))
}

async fn termbridge_clipboard_read(
    State(state): State<AppState>,
    Json(req): Json<TermBridgeClipboardReadRequest>,
) -> Result<Json<TermBridgeClipboardReadResponse>, ApiError> {
    let persona = env::var("SHELLDONE_PERSONA").ok().filter(|v| !v.is_empty());

    let channel = req
        .channel
        .as_deref()
        .map(|value| {
            value
                .parse::<ClipboardChannel>()
                .map_err(|err| ApiError::invalid("clipboard.invalid_channel", err))
        })
        .transpose()?
        .unwrap_or(ClipboardChannel::Clipboard);

    let mut read_request = ClipboardReadRequest::new(channel);
    if let Some(backend) = req.backend.clone() {
        read_request = read_request.with_backend(backend);
    }

    let policy_input = TermBridgePolicyInput {
        action: "clipboard.read".to_string(),
        persona: persona.clone(),
        terminal: None,
        command: None,
        backend: req.backend.clone(),
        channel: Some(channel.as_str().to_string()),
        bytes: None,
        cwd: None,
    };

    let decision = {
        let engine = state.policy_engine();
        let guard = engine
            .lock()
            .map_err(|e| ApiError::internal("policy_lock", anyhow!(e.to_string())))?;
        guard
            .evaluate_termbridge(&policy_input)
            .map_err(|err| ApiError::internal("policy_eval", err))?
    };

    if let Some(metrics) = state.metrics() {
        metrics.record_policy_evaluation(decision.is_allowed());
        if !decision.is_allowed() {
            metrics.record_policy_denial("termbridge.clipboard.read", persona.as_deref());
        }
    }

    if !decision.is_allowed() {
        warn!(
            "Policy denied termbridge.clipboard.read backend={:?} channel={} reasons={:?}",
            req.backend,
            channel.as_str(),
            decision.deny_reasons
        );
        let denial_payload = json!({
            "action": "clipboard.read",
            "backend": req.backend,
            "channel": channel.as_str(),
            "allowed": false,
            "reasons": decision.deny_reasons,
        });
        state
            .append_event(&EventRecord::new(
                "termbridge.clipboard.denied",
                persona.clone(),
                denial_payload,
                None,
                Some("termbridge::policy".to_string()),
                None,
            ))
            .await
            .map_err(|err| ApiError::internal("journal_write", err))?;
        return Err(ApiError::forbidden(
            "policy_denied",
            decision.deny_reasons.join("; "),
        ));
    }

    let clipboard = state.clipboard();
    let result = clipboard
        .read(read_request)
        .await
        .map_err(|err| clipboard_service_error("clipboard.read", err))?;

    let content = result.content;
    let bytes = content.bytes().to_vec();
    let mime = content.mime().as_str().to_string();
    let text = String::from_utf8(bytes.clone()).ok();
    let base64 = if req.as_base64.unwrap_or(false) || text.is_none() {
        Some(BASE64_STANDARD.encode(&bytes))
    } else {
        None
    };

    let event_payload = json!({
        "action": "clipboard.read",
        "backend": result.backend_id,
        "requested_backend": req.backend,
        "channel": channel.as_str(),
        "bytes": bytes.len(),
        "mime": mime,
    });
    state
        .append_event(&EventRecord::new(
            "termbridge.clipboard.read",
            persona.clone(),
            event_payload,
            None,
            Some("termbridge::clipboard".to_string()),
            Some(bytes.len()),
        ))
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;

    Ok(Json(TermBridgeClipboardReadResponse {
        backend: result.backend_id,
        bytes: bytes.len(),
        text: if req.as_base64.unwrap_or(false) {
            text.clone()
        } else {
            text
        },
        base64,
        mime,
    }))
}

fn negotiate(payload: &HandshakeRequest) -> HandshakeResponse {
    let persona = payload
        .persona
        .clone()
        .unwrap_or_else(|| "core".to_string());
    let mut accepted = Vec::new();
    let mut fallback = Vec::new();

    let capabilities = payload.capabilities.clone().unwrap_or_default();

    accepted.push(CapabilityAck {
        name: "version".to_string(),
        value: json!(payload.version.unwrap_or(1)),
    });

    // Keyboard negotiation
    let keyboard = negotiate_list(capabilities.get("keyboard"), &["kitty", "legacy"], "legacy");
    accepted.push(CapabilityAck {
        name: "keyboard".to_string(),
        value: json!(keyboard.accepted),
    });
    if let Some(fb) = keyboard.fallback {
        fallback.push(CapabilityAck {
            name: "keyboard".to_string(),
            value: json!(fb),
        });
    }

    // Graphics negotiation
    let graphics = negotiate_list(
        capabilities.get("graphics"),
        &["kitty", "minimal"],
        "minimal",
    );
    accepted.push(CapabilityAck {
        name: "graphics".to_string(),
        value: json!(graphics.accepted),
    });
    if let Some(fb) = graphics.fallback {
        fallback.push(CapabilityAck {
            name: "graphics".to_string(),
            value: json!(fb),
        });
    }

    // OSC 52 policy
    let osc52_value = json!({
        "write": "whitelist",
        "read": "confirm",
    });
    accepted.push(CapabilityAck {
        name: "osc52".to_string(),
        value: osc52_value,
    });

    // Semantic zones & RGB discovery
    accepted.push(CapabilityAck {
        name: "semantic_zones".to_string(),
        value: json!("osc133"),
    });
    accepted.push(CapabilityAck {
        name: "term_caps".to_string(),
        value: json!(["rgb", "alt_screen_mirror"]),
    });

    HandshakeResponse {
        accepted,
        fallback,
        persona,
    }
}

struct NegotiationResult {
    accepted: Vec<String>,
    fallback: Option<String>,
}

fn negotiate_list(
    offered: Option<&Value>,
    supported: &[&str],
    default_fallback: &str,
) -> NegotiationResult {
    let mut accepted = Vec::new();
    let mut fallback = None;

    if let Some(values) = offered.and_then(|v| v.as_array()) {
        for value in values {
            if let Some(candidate) = value.as_str() {
                if supported.contains(&candidate) {
                    accepted.push(candidate.to_string());
                }
            }
        }
        if accepted.is_empty() {
            fallback = Some(default_fallback.to_string());
            accepted.push(default_fallback.to_string());
        }
    } else {
        accepted.push(default_fallback.to_string());
    }

    NegotiationResult { accepted, fallback }
}

#[derive(Debug, Deserialize)]
struct AckPacket {
    id: Option<String>,
    persona: Option<String>,
    command: String,
    args: Option<Value>,
    spectral_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExecArgsPayload {
    cmd: String,
    cwd: Option<PathBuf>,
    env: Option<HashMap<String, String>>,
    shell: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    protocol_version: Option<String>,
    #[serde(default)]
    identity: Option<InitializeIdentity>,
    #[serde(rename = "clientCapabilities", default)]
    client_capabilities: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct InitializeIdentity {
    persona: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExecResponse {
    status: &'static str,
    event_id: String,
    exit_code: i32,
    stdout: String,
    stderr: String,
    spectral_tag: String,
}

#[derive(Debug, Deserialize)]
struct JournalRequest {
    #[serde(default = "default_journal_kind")]
    kind: String,
    persona: Option<String>,
    payload: Value,
    spectral_tag: Option<String>,
    bytes: Option<usize>,
}

fn default_journal_kind() -> String {
    "custom".to_string()
}

async fn agent_exec(
    State(state): State<AppState>,
    Json(packet): Json<AckPacket>,
) -> Result<Json<ExecResponse>, ApiError> {
    if packet.command != "agent.exec" {
        return Err(ApiError::unsupported(
            "unsupported_command",
            "command not implemented",
        ));
    }
    let args_value = packet
        .args
        .clone()
        .ok_or_else(|| ApiError::invalid("missing_args", "agent.exec requires args"))?;
    let exec_args_payload: ExecArgsPayload = serde_json::from_value(args_value.clone())
        .map_err(|err| ApiError::invalid("invalid_args", err.to_string()))?;
    let exec_args = ExecArgs::try_new(
        exec_args_payload.cmd,
        exec_args_payload.cwd,
        exec_args_payload.env,
        exec_args_payload.shell,
    )
    .map_err(|err| ApiError::invalid("invalid_args", err))?;

    let request = ExecRequest {
        command_id: packet.id.clone(),
        persona: packet.persona.clone(),
        args: exec_args,
        spectral_tag: packet.spectral_tag.clone(),
    };

    let exec_result = state
        .ack()
        .exec(request)
        .await
        .map_err(|err| ack_error_to_api("agent.exec", err))?;

    Ok(Json(ExecResponse {
        status: "ok",
        event_id: exec_result.event_id,
        exit_code: exec_result.exit_code,
        stdout: exec_result.stdout,
        stderr: exec_result.stderr,
        spectral_tag: exec_result.spectral_tag,
    }))
}

async fn journal_event(
    State(state): State<AppState>,
    Json(req): Json<JournalRequest>,
) -> Result<Json<EventRecord>, ApiError> {
    let event = state
        .ack()
        .journal_custom(
            req.kind,
            req.persona.clone(),
            req.payload,
            req.spectral_tag.clone(),
            req.bytes,
        )
        .await
        .map_err(|err| ack_error_to_api("journal", err))?;

    Ok(Json(event))
}

#[derive(Debug, Serialize)]
struct PendingApprovalsResponse {
    approvals: Vec<PendingApprovalDto>,
}

#[derive(Debug, Serialize)]
struct PendingApprovalDto {
    id: String,
    command: String,
    persona: Option<String>,
    reason: String,
    requested_at: String,
    resolved_at: Option<String>,
    status: String,
}

impl From<PendingApproval> for PendingApprovalDto {
    fn from(approval: PendingApproval) -> Self {
        Self {
            id: approval.id,
            command: approval.command,
            persona: approval.persona,
            reason: approval.reason,
            requested_at: approval.requested_at.to_rfc3339(),
            resolved_at: approval.resolved_at.map(|ts| ts.to_rfc3339()),
            status: match approval.status {
                ApprovalStatus::Pending => "pending".to_string(),
                ApprovalStatus::Granted => "granted".to_string(),
                ApprovalStatus::Rejected => "rejected".to_string(),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApprovalGrantPayload {
    approval_id: String,
}

async fn list_pending_approvals(
    State(state): State<AppState>,
) -> Result<Json<PendingApprovalsResponse>, ApiError> {
    let approvals = state
        .approvals()
        .list_pending()
        .into_iter()
        .map(PendingApprovalDto::from)
        .collect();

    Ok(Json(PendingApprovalsResponse { approvals }))
}

async fn grant_approval(
    State(state): State<AppState>,
    Json(payload): Json<ApprovalGrantPayload>,
) -> Result<Json<PendingApprovalDto>, ApiError> {
    let approval = state
        .ack()
        .grant_approval(&payload.approval_id)
        .await
        .map_err(|err| ack_error_to_api("approval.grant", err))?;

    Ok(Json(PendingApprovalDto::from(approval)))
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    body: ErrorBody,
}

impl ApiError {
    fn invalid(code: &'static str, err: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ErrorBody {
                code,
                message: err.into(),
            },
        }
    }

    fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: ErrorBody {
                code,
                message: message.into(),
            },
        }
    }

    fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            body: ErrorBody {
                code,
                message: message.into(),
            },
        }
    }

    fn unsupported(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            body: ErrorBody {
                code,
                message: message.into(),
            },
        }
    }

    fn internal(code: &'static str, err: impl Into<anyhow::Error>) -> Self {
        let err = err.into();
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorBody {
                code,
                message: err.to_string(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status;
        let body = Json(self.body);
        (status, body).into_response()
    }
}

fn termbridge_service_error(code: &'static str, err: TermBridgeServiceError) -> ApiError {
    match err {
        TermBridgeServiceError::Internal(message) => {
            ApiError::internal(code, anyhow::anyhow!(message))
        }
        TermBridgeServiceError::NotFound(message) => ApiError::not_found(code, message),
        TermBridgeServiceError::NotSupported {
            terminal,
            action,
            reason,
        } => ApiError::unsupported(
            code,
            format!(
                "terminal {} does not support {}: {}",
                terminal, action, reason
            ),
        ),
    }
}

fn clipboard_service_error(code: &'static str, err: ClipboardServiceError) -> ApiError {
    match err {
        ClipboardServiceError::Clipboard(ClipboardError::PayloadTooLarge { size, limit }) => {
            ApiError::invalid(code, format!("payload too large: {size} > {limit}"))
        }
        ClipboardServiceError::Clipboard(ClipboardError::ChannelNotSupported(channel)) => {
            ApiError::invalid(code, format!("channel {channel} not supported"))
        }
        ClipboardServiceError::Clipboard(ClipboardError::OperationNotSupported { backend }) => {
            ApiError::unsupported(
                code,
                format!("backend {backend} does not support this operation"),
            )
        }
        ClipboardServiceError::Clipboard(ClipboardError::NoBackends) => {
            ApiError::unsupported(code, "no clipboard backends configured")
        }
        ClipboardServiceError::Clipboard(ClipboardError::BackendFailure { backend, reason }) => {
            ApiError::internal(code, anyhow::anyhow!(format!("{backend}: {reason}")))
        }
        ClipboardServiceError::ExhaustedBackends(failures) => {
            let detail = failures
                .into_iter()
                .map(|failure| format!("{}: {}", failure.backend_id, failure.reason))
                .collect::<Vec<_>>()
                .join("; ");
            ApiError::internal(
                code,
                anyhow::anyhow!(format!("all backends failed: {detail}")),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::termbridge::{
        TerminalBinding, TerminalBindingId, TerminalCapabilities, TerminalId,
    };
    use crate::ports::termbridge::{
        CapabilityObservation, TermBridgeError, TerminalBindingRepository, TerminalControlPort,
    };
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use futures::TryStreamExt;
    use serde_json::json;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::TempDir;
    use tower::ServiceExt;

    struct FocusAdapter {
        calls: Arc<Mutex<Vec<TerminalBindingId>>>,
    }

    impl FocusAdapter {
        fn new(calls: Arc<Mutex<Vec<TerminalBindingId>>>) -> Self {
            Self { calls }
        }
    }

    #[async_trait]
    impl TerminalControlPort for FocusAdapter {
        fn terminal_id(&self) -> TerminalId {
            TerminalId::new("wezterm")
        }

        async fn detect(&self) -> CapabilityObservation {
            CapabilityObservation::new(
                "WezTerm",
                TerminalCapabilities::builder()
                    .spawn(true)
                    .focus(true)
                    .send_text(true)
                    .build(),
                false,
                Vec::new(),
            )
        }

        async fn focus(&self, binding: &TerminalBinding) -> Result<(), TermBridgeError> {
            self.calls.lock().unwrap().push(binding.id.clone());
            Ok(())
        }
    }
    #[test]
    fn negotiate_prefers_offered_supported() {
        let mut caps = HashMap::new();
        caps.insert("keyboard".to_string(), json!(["kitty", "legacy"]));
        let req = HandshakeRequest {
            version: Some(1),
            persona: Some("nova".into()),
            capabilities: Some(caps),
        };
        let resp = negotiate(&req);
        assert_eq!(resp.persona, "nova");
        let keyboard = resp
            .accepted
            .iter()
            .find(|cap| cap.name == "keyboard")
            .unwrap();
        assert!(keyboard.value.as_array().unwrap().contains(&json!("kitty")));
    }

    #[tokio::test]
    async fn exec_writes_event() {
        let temp = TempDir::new().unwrap();
        let state = AppState::new(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            SocketAddr::from(([127, 0, 0, 1], 0)),
            temp.path().to_path_buf(),
            CipherPolicy::Balanced,
            None,
            None,
        )
        .unwrap();
        let packet = AckPacket {
            id: None,
            persona: Some("core".into()),
            command: "agent.exec".into(),
            args: Some(json!({"cmd": "echo hello"})),
            spectral_tag: None,
        };
        let Json(response) = agent_exec(State(state.clone()), Json(packet))
            .await
            .expect("exec response");
        assert!(response.stdout.contains("hello"));
        let mut attempts = 0;
        loop {
            let journal = tokio::fs::read_to_string(state.journal_path())
                .await
                .expect("journal exists");
            if journal.contains("\"kind\":\"exec\"") {
                break;
            }
            attempts += 1;
            assert!(
                attempts < 10,
                "journal missing exec event after retries: {}",
                journal
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn journal_endpoint_appends_event() {
        let temp = TempDir::new().unwrap();
        let state = AppState::new(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            SocketAddr::from(([127, 0, 0, 1], 0)),
            temp.path().to_path_buf(),
            CipherPolicy::Balanced,
            None,
            None,
        )
        .unwrap();
        let req = JournalRequest {
            kind: "cli.event".to_string(),
            persona: Some("core".into()),
            payload: json!({"message": "hello"}),
            spectral_tag: Some("cli::event".into()),
            bytes: Some(5),
        };
        let Json(event) = journal_event(State(state.clone()), Json(req))
            .await
            .expect("journal response");
        assert_eq!(event.kind, "cli.event");
        let mut attempt = 0;
        loop {
            let journal = tokio::fs::read_to_string(state.journal_path())
                .await
                .expect("journal exists");
            if journal.contains("\"cli.event\"") {
                break;
            }
            attempt += 1;
            assert!(
                attempt < 10,
                "journal missing cli.event after retries: {}",
                journal
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn termbridge_cwd_endpoint_updates_binding_and_journal() {
        use crate::adapters::termbridge::{
            FileTermBridgeStateRepository, InMemoryTermBridgeBindingRepository,
        };

        let temp = TempDir::new().unwrap();
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let state_repo = Arc::new(FileTermBridgeStateRepository::new(
            temp.path().join("termbridge_state.json"),
        ));
        let termbridge_service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo.clone(),
            Vec::new(),
            None,
        ));

        let state =
            AppState::for_termbridge_test(termbridge_service.clone(), temp.path().to_path_buf())
                .expect("test state");

        let mut labels = HashMap::new();
        labels.insert("pane_id".to_string(), "pane-1".to_string());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "pane-1",
            labels,
            Some("wezterm://pane/pane-1".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding).await.unwrap();

        let app = Router::new()
            .route("/termbridge/cwd", post(termbridge_update_cwd))
            .with_state(state.clone());

        let payload = json!({
            "binding_id": binding_id.to_string(),
            "cwd": "/workspace/project"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/termbridge/cwd")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response
            .into_body()
            .into_data_stream()
            .try_fold(Vec::new(), |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            })
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["labels"]["cwd"], "/workspace/project");

        let stored = termbridge_service
            .get_binding(&binding_id)
            .await
            .unwrap()
            .expect("binding persisted");
        assert_eq!(
            stored.labels.get("cwd"),
            Some(&"/workspace/project".to_string())
        );

        let journal_path = state.state_dir().join("journal").join("continuum.log");
        let mut attempts = 0;
        loop {
            if journal_path.exists() {
                let journal = tokio::fs::read_to_string(&journal_path).await.unwrap();
                if journal.contains("termbridge.cwd_update") {
                    assert!(journal.contains("\"previous_cwd\":null"));
                    break;
                }
            }
            attempts += 1;
            assert!(attempts < 20, "journal entry not observed");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn termbridge_cwd_endpoint_returns_not_found() {
        use crate::adapters::termbridge::{
            FileTermBridgeStateRepository, InMemoryTermBridgeBindingRepository,
        };

        let temp = TempDir::new().unwrap();
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let state_repo = Arc::new(FileTermBridgeStateRepository::new(
            temp.path().join("termbridge_state.json"),
        ));
        let termbridge_service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo,
            Vec::new(),
            None,
        ));

        let state = AppState::for_termbridge_test(termbridge_service, temp.path().to_path_buf())
            .expect("test state");

        let app = Router::new()
            .route("/termbridge/cwd", post(termbridge_update_cwd))
            .with_state(state);

        let payload = json!({
            "binding_id": TerminalBindingId::new().to_string(),
            "cwd": "/missing"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/termbridge/cwd")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn termbridge_focus_endpoint_focuses_binding_and_journal() {
        use crate::adapters::termbridge::{
            FileTermBridgeStateRepository, InMemoryTermBridgeBindingRepository,
        };

        let temp = TempDir::new().unwrap();
        let binding_repo = Arc::new(InMemoryTermBridgeBindingRepository::default());
        let state_repo = Arc::new(FileTermBridgeStateRepository::new(
            temp.path().join("termbridge_state.json"),
        ));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let adapter = Arc::new(FocusAdapter::new(calls.clone()));

        let termbridge_service = Arc::new(TermBridgeService::new(
            state_repo,
            binding_repo.clone(),
            vec![adapter],
            None,
        ));

        let state =
            AppState::for_termbridge_test(termbridge_service.clone(), temp.path().to_path_buf())
                .expect("test state");

        let mut labels = HashMap::new();
        labels.insert("pane_id".to_string(), "pane-focus".to_string());
        let binding = TerminalBinding::new(
            TerminalId::new("wezterm"),
            "token-focus",
            labels,
            Some("wezterm://pane/pane-focus".into()),
        );
        let binding_id = binding.id.clone();
        binding_repo.save(binding).await.unwrap();

        let app = Router::new()
            .route("/termbridge/focus", post(termbridge_focus))
            .with_state(state.clone());

        let payload = json!({
            "binding_id": binding_id.to_string(),
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/termbridge/focus")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response
            .into_body()
            .into_data_stream()
            .try_fold(Vec::new(), |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            })
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "accepted");

        let recorded_calls = calls.lock().unwrap();
        assert_eq!(recorded_calls.len(), 1);
        assert_eq!(recorded_calls[0], binding_id);
        drop(recorded_calls);

        let journal_path = state.state_dir().join("journal").join("continuum.log");
        let mut attempts = 0;
        loop {
            if journal_path.exists() {
                let journal = tokio::fs::read_to_string(&journal_path).await.unwrap();
                if journal.contains("termbridge.focus") {
                    break;
                }
            }
            attempts += 1;
            assert!(attempts < 20, "journal entry not observed");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[derive(Debug, Deserialize)]
struct UndoArgs {
    snapshot_id: String,
}

#[derive(Debug, Serialize)]
struct UndoResponse {
    status: &'static str,
    snapshot_id: String,
    restored_events: usize,
    duration_ms: i64,
}

async fn agent_undo(
    State(state): State<AppState>,
    Json(packet): Json<AckPacket>,
) -> Result<Json<UndoResponse>, ApiError> {
    if packet.command != "agent.undo" {
        return Err(ApiError::unsupported(
            "unsupported_command",
            "expected agent.undo",
        ));
    }
    let args_value = packet
        .args
        .clone()
        .ok_or_else(|| ApiError::invalid("missing_args", "agent.undo requires args"))?;
    let undo_args: UndoArgs = serde_json::from_value(args_value)
        .map_err(|err| ApiError::invalid("invalid_args", err.to_string()))?;

    let undo_request = UndoRequest {
        persona: packet.persona.clone(),
        snapshot_id: undo_args.snapshot_id,
        spectral_tag: packet.spectral_tag.clone(),
    };

    let undo_result = state
        .ack()
        .undo(undo_request)
        .await
        .map_err(|err| ack_error_to_api("agent.undo", err))?;

    info!(
        "Restored {} events from snapshot {} in {}ms",
        undo_result.restored_events, undo_result.snapshot_id, undo_result.duration_ms as i64
    );

    Ok(Json(UndoResponse {
        status: "ok",
        snapshot_id: undo_result.snapshot_id,
        restored_events: undo_result.restored_events,
        duration_ms: undo_result.duration_ms as i64,
    }))
}

async fn mcp_ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(err) = handle_mcp_socket(socket, state).await {
            warn!("MCP session terminated: {err:#}");
        }
    })
}

async fn handle_mcp_socket(mut socket: WebSocket, state: AppState) -> AnyResult<()> {
    let bridge = state.mcp();
    let mut session: Option<McpSession> = None;

    while let Some(message) = socket.recv().await {
        let message = match message {
            Ok(msg) => msg,
            Err(err) => {
                warn!("WebSocket receive error: {err}");
                break;
            }
        };

        match message {
            Message::Text(text) => {
                let request: JsonRpcRequest = match serde_json::from_str(&text) {
                    Ok(req) => req,
                    Err(err) => {
                        warn!("Invalid JSON-RPC payload: {err}");
                        continue;
                    }
                };

                if request.jsonrpc != "2.0" {
                    if let Some(id) = request.id.clone() {
                        let error = JsonRpcError {
                            code: -32600,
                            message: "Invalid JSON-RPC version".to_string(),
                            data: None,
                        };
                        let response = JsonRpcResponse {
                            jsonrpc: "2.0",
                            id,
                            result: None,
                            error: Some(error),
                        };
                        let payload = serde_json::to_string(&response)?;
                        socket.send(Message::Text(payload)).await?;
                    }
                    continue;
                }

                match request.method.as_str() {
                    "initialize" => {
                        let id = match request.id.clone() {
                            Some(id) => id,
                            None => continue,
                        };
                        let params: InitializeParams = request
                            .params
                            .clone()
                            .and_then(|value| serde_json::from_value(value).ok())
                            .unwrap_or_default();
                        let protocol_version = params.protocol_version.ok_or_else(|| {
                            McpBridgeError::Protocol("protocolVersion is required".into())
                        })?;
                        let persona = params.identity.and_then(|ident| ident.persona);
                        let capabilities = extract_capabilities(params.client_capabilities.clone());
                        let new_session = bridge
                            .initialize_session(persona, protocol_version.clone(), capabilities)
                            .await;
                        match new_session {
                            Ok(session_obj) => {
                                let session_id = session_obj.id().to_string();
                                let result = json!({
                                    "protocolVersion": protocol_version,
                                    "sessionId": session_id,
                                    "serverCapabilities": {
                                        "tools": {
                                            "listChanged": false
                                        }
                                    }
                                });
                                session = Some(session_obj);
                                send_json_response(&mut socket, id, Ok(result)).await?;
                            }
                            Err(err) => {
                                send_json_response(&mut socket, id, Err(err)).await?;
                            }
                        }
                    }
                    "tools/list" => {
                        if let Some(id) = request.id.clone() {
                            let result = bridge.list_tools().await;
                            send_json_response(&mut socket, id, Ok(result)).await?;
                        }
                    }
                    "tools/call" => {
                        let id = match request.id.clone() {
                            Some(id) => id,
                            None => continue,
                        };
                        let params_value = request.params.clone().unwrap_or(Value::Null);
                        let params: ToolCallParams =
                            serde_json::from_value(params_value).map_err(|err| anyhow!(err))?;
                        let mut current_session = match session.take() {
                            Some(sess) => sess,
                            None => {
                                send_json_response(
                                    &mut socket,
                                    id,
                                    Err(McpBridgeError::Protocol("session not initialized".into())),
                                )
                                .await?;
                                continue;
                            }
                        };
                        if let Some(requested) = params.session_id.as_ref() {
                            if &current_session.id().to_string() != requested {
                                let error = McpBridgeError::Protocol("session mismatch".into());
                                send_json_response(&mut socket, id, Err(error)).await?;
                                session = Some(current_session);
                                continue;
                            }
                        }
                        let outcome = bridge
                            .call_tool(&mut current_session, &params.name, params.arguments)
                            .await
                            .map(|exec| {
                                json!({
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": exec.stdout,
                                        }
                                    ],
                                    "isError": exec.exit_code != 0,
                                    "metadata": {
                                        "exitCode": exec.exit_code,
                                        "stderr": exec.stderr,
                                        "eventId": exec.event_id,
                                        "spectralTag": exec.spectral_tag,
                                        "durationMs": exec.duration_ms,
                                    }
                                })
                            });
                        send_json_response(&mut socket, id, outcome).await?;
                        session = Some(current_session);
                    }
                    "ping" => {
                        if let Some(id) = request.id.clone() {
                            send_json_response(&mut socket, id, Ok(json!({}))).await?;
                        }
                    }
                    "notifications/heartbeat" | "session/heartbeat" => {
                        if let Some(mut current_session) = session.take() {
                            if let Err(err) = bridge.record_heartbeat(&mut current_session).await {
                                warn!("heartbeat failed: {err}");
                            }
                            session = Some(current_session);
                        }
                    }
                    _ => {
                        if let Some(id) = request.id.clone() {
                            send_json_response(
                                &mut socket,
                                id,
                                Err(McpBridgeError::UnsupportedTool(request.method)),
                            )
                            .await?;
                        }
                    }
                }
            }
            Message::Binary(_) => {}
            Message::Ping(payload) => {
                socket.send(Message::Pong(payload)).await.ok();
            }
            Message::Pong(_) => continue,
            Message::Close(_) => break,
        }
    }

    if let Some(mut session) = session {
        let _ = bridge
            .close_session(&mut session, Some("socket closed".into()))
            .await;
    }

    Ok(())
}

fn extract_capabilities(capabilities: Option<Value>) -> Vec<String> {
    match capabilities {
        Some(Value::Object(map)) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

async fn send_json_response(
    socket: &mut WebSocket,
    id: Value,
    outcome: Result<Value, McpBridgeError>,
) -> AnyResult<()> {
    let response = match outcome {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        },
        Err(err) => {
            let json_error = map_mcp_error(err);
            JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json_error),
            }
        }
    };
    let payload = serde_json::to_string(&response)?;
    socket.send(Message::Text(payload)).await?;
    Ok(())
}

fn map_mcp_error(err: McpBridgeError) -> JsonRpcError {
    match err {
        McpBridgeError::Protocol(message) => JsonRpcError {
            code: -32602,
            message,
            data: None,
        },
        McpBridgeError::UnsupportedTool(tool) => JsonRpcError {
            code: -32601,
            message: format!("Unsupported method/tool: {tool}"),
            data: None,
        },
        McpBridgeError::ToolFailure(message) => JsonRpcError {
            code: -32000,
            message,
            data: None,
        },
        McpBridgeError::Internal(message) => JsonRpcError {
            code: -32603,
            message,
            data: None,
        },
    }
}
