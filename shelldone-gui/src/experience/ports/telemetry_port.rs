use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFrameStatus {
    Active,
    Registered,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFrame {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub channel: Option<String>,
    pub status: AgentFrameStatus,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub registered_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalFrame {
    pub id: String,
    pub command: String,
    pub persona: Option<String>,
    pub reason: String,
    pub requested_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonaFrame {
    pub name: String,
    pub intent_hint: Option<String>,
    pub tone_hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ApprovalSource {
    Http,
    Local,
    #[default]
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetrySnapshot {
    pub generated_at: Option<DateTime<Utc>>,
    pub persona: Option<PersonaFrame>,
    pub agents: Vec<AgentFrame>,
    pub approvals: Vec<ApprovalFrame>,
    pub telemetry_ready: bool,
    pub approvals_source: ApprovalSource,
    pub termbridge_delta: Option<TermBridgeDeltaSnapshot>,
    pub state_snapshots: Vec<StateSnapshotFrame>,
}

impl Default for TelemetrySnapshot {
    fn default() -> Self {
        Self {
            generated_at: None,
            persona: None,
            agents: Vec::new(),
            approvals: Vec::new(),
            telemetry_ready: false,
            approvals_source: ApprovalSource::None,
            termbridge_delta: None,
            state_snapshots: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateSnapshotFrame {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub path: String,
    pub tags: Vec<String>,
}

pub trait ExperienceTelemetryPort: Send + Sync {
    fn snapshot(&self) -> anyhow::Result<TelemetrySnapshot>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TermBridgeDeltaSnapshot {
    pub changed: bool,
    pub added: Vec<TermBridgeTerminalChange>,
    pub updated: Vec<TermBridgeTerminalChange>,
    pub removed: Vec<TermBridgeTerminalChange>,
    pub terminals: Vec<TermBridgeTerminalChange>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TermBridgeTerminalChange {
    pub terminal: String,
    pub requires_opt_in: bool,
    pub source: Option<String>,
    pub capabilities: TermBridgeTerminalCapabilities,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TermBridgeTerminalCapabilities {
    pub spawn: bool,
    pub split: bool,
    pub focus: bool,
    pub duplicate: bool,
    pub close: bool,
    pub send_text: bool,
}
