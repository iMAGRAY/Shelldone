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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TelemetrySnapshot {
    pub generated_at: Option<DateTime<Utc>>,
    pub persona: Option<PersonaFrame>,
    pub agents: Vec<AgentFrame>,
    pub approvals: Vec<ApprovalFrame>,
    pub telemetry_ready: bool,
}

pub trait ExperienceTelemetryPort: Send + Sync {
    fn snapshot(&self) -> anyhow::Result<TelemetrySnapshot>;
}
