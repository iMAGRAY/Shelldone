mod continuum;
mod policy_engine;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use policy_engine::{AckPolicyInput, PolicyEngine};
use continuum::{ContinuumSnapshot, ContinuumStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::signal::ctrl_c;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    journal_path: Arc<PathBuf>,
    policy_engine: Arc<Mutex<PolicyEngine>>,
    continuum_store: Arc<tokio::sync::Mutex<ContinuumStore>>,
}

impl AppState {
    fn new(state_dir: PathBuf, policy_path: Option<PathBuf>) -> Self {
        let journal_path = state_dir.join("journal").join("continuum.log");
        let policy_engine = PolicyEngine::new(policy_path.as_deref())
            .unwrap_or_else(|e| {
                warn!("Failed to load policy engine: {e}. Policy enforcement disabled.");
                PolicyEngine::new(None).expect("creating disabled policy engine")
            });

        Self {
            journal_path: Arc::new(journal_path),
            policy_engine: Arc::new(Mutex::new(policy_engine)),
            continuum_store: Arc::new(tokio::sync::Mutex::new({
                let snapshot_dir = state_dir.join("snapshots");
                ContinuumStore::new(state_dir.join("journal").join("continuum.log"), snapshot_dir)
            })),
        }
    }

    fn journal_path(&self) -> &Path {
        &self.journal_path
    }

    async fn append_event(&self, event: &EventRecord) -> anyhow::Result<()> {
        let dir = self
            .journal_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        tokio::fs::create_dir_all(&dir).await?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.journal_path())
            .await?;
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        file.write_all(&line).await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub listen: SocketAddr,
    pub state_dir: PathBuf,
    pub policy_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 17717)),
            state_dir: PathBuf::from("state"),
            policy_path: Some(PathBuf::from("policies/default.rego")),
        }
    }
}

pub async fn run(settings: Settings) -> anyhow::Result<()> {
    let state = AppState::new(settings.state_dir.clone(), settings.policy_path.clone());
    tokio::fs::create_dir_all(
        state
            .journal_path()
            .parent()
            .unwrap_or_else(|| Path::new(".")),
    )
    .await?;

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/sigma/handshake", post(handshake))
        .route("/ack/exec", post(agent_exec))
        .route("/journal/event", post(journal_event))
        .route("/ack/undo", post(agent_undo))
        .with_state(state.clone());

    let listener = TcpListener::bind(settings.listen).await?;
    info!("listening" = %settings.listen, "state_dir" = %settings.state_dir.display(), "msg" = "shelldone-agentd started");

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
struct ExecArgs {
    cmd: String,
    cwd: Option<PathBuf>,
    env: Option<HashMap<String, String>>,
    shell: Option<String>,
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

    // Policy check
    let policy_input = AckPolicyInput::new(
        packet.command.clone(),
        packet.persona.clone(),
        packet.spectral_tag.clone(),
    );

    let policy_decision = state
        .policy_engine
        .lock()
        .unwrap()
        .evaluate_ack(&policy_input)
        .map_err(|e| ApiError::internal("policy_evaluation", e))?;

    if !policy_decision.is_allowed() {
        let reason = policy_decision.deny_reasons.join("; ");
        warn!("Policy denied agent.exec: {}", reason);

        // Log policy denial event
        let event = EventRecord::new(
            "policy_denied",
            packet.persona.clone(),
            json!({
                "command": packet.command,
                "deny_reasons": policy_decision.deny_reasons,
            }),
            None,
            packet.spectral_tag.clone(),
            None,
        );
        let _ = state.append_event(&event).await;

        return Err(ApiError::forbidden("policy_denied", reason));
    }

    let args_value = packet
        .args
        .clone()
        .ok_or_else(|| ApiError::invalid("missing_args", "agent.exec requires args"))?;
    let exec_args: ExecArgs = serde_json::from_value(args_value.clone())
        .map_err(|err| ApiError::invalid("invalid_args", err.to_string()))?;

    let event_id = packet
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let persona = packet.persona.clone();
    let spectral_tag = packet
        .spectral_tag
        .clone()
        .unwrap_or_else(|| "exec::default".to_string());

    let start_time = Utc::now();

    let output = spawn_command(&exec_args)
        .await
        .map_err(|err| ApiError::internal("exec_failed", err))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let event = EventRecord::new(
        "exec",
        persona.clone(),
        json!({
            "command": packet.command,
            "args": args_value,
            "exit_code": exit_code,
            "stdout_len": stdout.len(),
            "stderr_len": stderr.len(),
            "duration_ms": (Utc::now() - start_time).num_milliseconds(),
        }),
        Some(event_id.clone()),
        Some(spectral_tag.clone()),
        Some(stdout.len() + stderr.len()),
    );

    if let Err(err) = state.append_event(&event).await {
        error!(%err, "failed to append exec event");
        return Err(ApiError::internal("journal_write", err));
    }

    Ok(Json(ExecResponse {
        status: "ok",
        event_id,
        exit_code,
        stdout,
        stderr,
        spectral_tag,
    }))
}

async fn journal_event(
    State(state): State<AppState>,
    Json(req): Json<JournalRequest>,
) -> Result<Json<EventRecord>, ApiError> {
    let event = EventRecord::new(
        &req.kind,
        req.persona.clone(),
        req.payload,
        None,
        req.spectral_tag.clone(),
        req.bytes,
    );

    state
        .append_event(&event)
        .await
        .map_err(|err| ApiError::internal("journal_write", err))?;

    Ok(Json(event))
}

async fn spawn_command(args: &ExecArgs) -> anyhow::Result<std::process::Output> {
    #[cfg(windows)]
    let shell = args.shell.as_deref().unwrap_or("cmd.exe");
    #[cfg(not(windows))]
    let shell = args.shell.as_deref().unwrap_or("sh");

    #[cfg(windows)]
    let mut command = {
        let mut cmd = Command::new(shell);
        cmd.arg("/C").arg(&args.cmd);
        cmd
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut cmd = Command::new(shell);
        cmd.arg("-c").arg(&args.cmd);
        cmd
    };

    if let Some(cwd) = &args.cwd {
        command.current_dir(cwd);
    }
    if let Some(env) = &args.env {
        command.envs(env.clone());
    }

    command.output().await.map_err(anyhow::Error::from)
}

#[derive(Serialize)]
struct EventRecord {
    event_id: String,
    kind: String,
    timestamp: String,
    persona: Option<String>,
    payload: Value,
    spectral_tag: Option<String>,
    bytes: Option<usize>,
}

impl EventRecord {
    fn new(
        kind: &str,
        persona: Option<String>,
        payload: Value,
        event_id: Option<String>,
        spectral_tag: Option<String>,
        bytes: Option<usize>,
    ) -> Self {
        Self {
            event_id: event_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            kind: kind.to_string(),
            timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            persona,
            payload,
            spectral_tag,
            bytes,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::TempDir;

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
        let state = AppState::new(temp.path().to_path_buf(), None);
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
        let journal = tokio::fs::read_to_string(state.journal_path())
            .await
            .expect("journal exists");
        assert!(journal.contains("\"kind\":\"exec\""));
    }

    #[tokio::test]
    async fn journal_endpoint_appends_event() {
        let temp = TempDir::new().unwrap();
        let state = AppState::new(temp.path().to_path_buf(), None);
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
        let journal = tokio::fs::read_to_string(state.journal_path())
            .await
            .expect("journal exists");
        assert!(journal.contains("\"cli.event\""));
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

    let policy_input = AckPolicyInput::new(
        packet.command.clone(),
        packet.persona.clone(),
        packet.spectral_tag.clone(),
    );

    let policy_decision = state
        .policy_engine
        .lock()
        .unwrap()
        .evaluate_ack(&policy_input)
        .map_err(|e| ApiError::internal("policy_evaluation", e))?;

    if !policy_decision.is_allowed() {
        let reason = policy_decision.deny_reasons.join("; ");
        warn!("Policy denied agent.undo: {}", reason);
        return Err(ApiError::forbidden("policy_denied", reason));
    }

    let args_value = packet
        .args
        .clone()
        .ok_or_else(|| ApiError::invalid("missing_args", "agent.undo requires args"))?;
    let undo_args: UndoArgs = serde_json::from_value(args_value)
        .map_err(|err| ApiError::invalid("invalid_args", err.to_string()))?;

    let start_time = Utc::now();

    let mut store = state.continuum_store.lock().await;

    let snapshots = store
        .list_snapshots()
        .map_err(|e| ApiError::internal("list_snapshots", e))?;

    let snapshot_path = snapshots
        .iter()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.contains(&undo_args.snapshot_id))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            ApiError::invalid(
                "snapshot_not_found",
                format!("Snapshot {} not found", undo_args.snapshot_id),
            )
        })?;

    let snapshot = ContinuumSnapshot::load(snapshot_path)
        .map_err(|e| ApiError::internal("load_snapshot", e))?;

    let events = snapshot
        .restore_events()
        .map_err(|e| ApiError::internal("restore_events", e))?;

    let restored_count = events.len();
    let duration = (Utc::now() - start_time).num_milliseconds();

    let undo_event = EventRecord::new(
        "undo",
        packet.persona.clone(),
        json!({
            "snapshot_id": undo_args.snapshot_id,
            "restored_events": restored_count,
            "duration_ms": duration,
        }),
        None,
        packet.spectral_tag.clone(),
        None,
    );

    if let Err(err) = state.append_event(&undo_event).await {
        error!(%err, "failed to log undo event");
    }

    info!(
        "Restored {} events from snapshot {} in {}ms",
        restored_count, undo_args.snapshot_id, duration
    );

    Ok(Json(UndoResponse {
        status: "ok",
        snapshot_id: undo_args.snapshot_id,
        restored_events: restored_count,
        duration_ms: duration,
    }))
}

