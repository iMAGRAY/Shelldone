use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use std::fmt;

/// Identifies a surface within the experience bounded context.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExperienceSurfaceId(String);

impl ExperienceSurfaceId {
    pub fn new(id: impl Into<String>) -> anyhow::Result<Self> {
        let value = id.into().trim().to_string();
        if value.is_empty() {
            anyhow::bail!("surface id must not be empty");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExperienceSurfaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Visual tier for a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExperienceLayer {
    Foundation,
    Primary,
    Overlay,
    HeadsUp,
}

/// Functional role a surface fulfils in the hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExperienceSurfaceRole {
    Workspace,
    Persona,
    AgentFeed,
    Metrics,
    CommandPalette,
    StateSync,
}

/// Intent that drives persona framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExperienceIntent {
    Focus,
    Explore,
    Automate,
    Recover,
}

/// Describes the persona currently guiding the UI framing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExperiencePersona {
    name: String,
    intent: ExperienceIntent,
    tone: String,
}

impl ExperiencePersona {
    pub fn new(name: impl Into<String>, intent: ExperienceIntent, tone: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            intent,
            tone: tone.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn intent(&self) -> ExperienceIntent {
        self.intent
    }

    pub fn tone(&self) -> &str {
        &self.tone
    }
}

/// State of an automation/AI agent rendered in the hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExperienceAgentState {
    Idle,
    Running,
    WaitingApproval,
    Error,
}

/// Value object describing an agent badge.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperienceAgentStatus {
    agent_id: ExperienceSurfaceId,
    label: String,
    state: ExperienceAgentState,
    confidence: NotNan<f32>,
}

impl ExperienceAgentStatus {
    pub fn new(
        agent_id: ExperienceSurfaceId,
        label: impl Into<String>,
        state: ExperienceAgentState,
        confidence: f32,
    ) -> anyhow::Result<Self> {
        if !(0.0..=1.0).contains(&confidence) {
            anyhow::bail!("confidence must be between 0 and 1 inclusive");
        }
        let label = label.into();
        if label.trim().is_empty() {
            anyhow::bail!("agent label must not be empty");
        }
        Ok(Self {
            agent_id,
            label,
            state,
            confidence: NotNan::new(confidence).expect("finite confidence"),
        })
    }

    pub fn state(&self) -> ExperienceAgentState {
        self.state
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn confidence(&self) -> f32 {
        self.confidence.into_inner()
    }

    pub fn agent_id(&self) -> &ExperienceSurfaceId {
        &self.agent_id
    }
}

/// Immutable description of a UI surface rendered inside the hub.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperienceSurface {
    id: ExperienceSurfaceId,
    title: String,
    layer: ExperienceLayer,
    role: ExperienceSurfaceRole,
    highlight_ratio: NotNan<f32>,
    persona: Option<ExperiencePersona>,
    agents: Vec<ExperienceAgentStatus>,
    approvals: Vec<ExperienceApproval>,
    snapshots: Vec<ExperienceSnapshot>,
    active: bool,
}

impl ExperienceSurface {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ExperienceSurfaceId,
        title: impl Into<String>,
        layer: ExperienceLayer,
        role: ExperienceSurfaceRole,
        highlight_ratio: f32,
        persona: Option<ExperiencePersona>,
        agents: Vec<ExperienceAgentStatus>,
        approvals: Vec<ExperienceApproval>,
        snapshots: Vec<ExperienceSnapshot>,
        active: bool,
    ) -> anyhow::Result<Self> {
        if !(0.0..=1.0).contains(&highlight_ratio) {
            anyhow::bail!("highlight ratio must be between 0 and 1 inclusive");
        }
        let title = title.into();
        if title.trim().is_empty() {
            anyhow::bail!("surface title must not be empty");
        }
        Ok(Self {
            id,
            title,
            layer,
            role,
            highlight_ratio: NotNan::new(highlight_ratio).expect("finite highlight ratio"),
            persona,
            agents,
            approvals,
            snapshots,
            active,
        })
    }

    pub fn id(&self) -> &ExperienceSurfaceId {
        &self.id
    }

    pub fn layer(&self) -> ExperienceLayer {
        self.layer
    }

    pub fn role(&self) -> ExperienceSurfaceRole {
        self.role
    }

    pub fn highlight_ratio(&self) -> f32 {
        self.highlight_ratio.into_inner()
    }

    pub fn persona(&self) -> Option<&ExperiencePersona> {
        self.persona.as_ref()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn agents(&self) -> &[ExperienceAgentStatus] {
        &self.agents
    }

    pub fn approvals(&self) -> &[ExperienceApproval] {
        &self.approvals
    }

    pub fn snapshots(&self) -> &[ExperienceSnapshot] {
        &self.snapshots
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

/// Approval entry describing pending policy decisions surfaced to the operator.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperienceApproval {
    id: String,
    command: String,
    persona: Option<String>,
    reason: String,
    requested_at: DateTime<Utc>,
}

impl ExperienceApproval {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        command: impl Into<String>,
        persona: Option<String>,
        reason: impl Into<String>,
        requested_at: DateTime<Utc>,
    ) -> anyhow::Result<Self> {
        let id = id.into();
        if id.trim().is_empty() {
            anyhow::bail!("approval id must not be empty");
        }
        let command = command.into();
        if command.trim().is_empty() {
            anyhow::bail!("approval command must not be empty");
        }
        let reason = reason.into();
        if reason.trim().is_empty() {
            anyhow::bail!("approval reason must not be empty");
        }
        Ok(Self {
            id,
            command,
            persona,
            reason,
            requested_at,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn persona(&self) -> Option<&str> {
        self.persona.as_deref()
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }
}

/// Snapshot entry describing stored session state information.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperienceSnapshot {
    id: String,
    label: String,
    created_at: DateTime<Utc>,
    size_bytes: u64,
    path: String,
    tags: Vec<String>,
}

impl ExperienceSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        created_at: DateTime<Utc>,
        size_bytes: u64,
        path: impl Into<String>,
        tags: Vec<String>,
    ) -> anyhow::Result<Self> {
        let id = id.into();
        if id.trim().is_empty() {
            anyhow::bail!("snapshot id must not be empty");
        }
        let label = label.into();
        if label.trim().is_empty() {
            anyhow::bail!("snapshot label must not be empty");
        }
        let path = path.into();
        if path.trim().is_empty() {
            anyhow::bail!("snapshot path must not be empty");
        }
        Ok(Self {
            id,
            label,
            created_at,
            size_bytes,
            path,
            tags,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_id_validation() {
        assert!(ExperienceSurfaceId::new("").is_err());
        assert!(ExperienceSurfaceId::new(" persona ").is_ok());
    }

    #[test]
    fn agent_status_validation() {
        let agent_id = ExperienceSurfaceId::new("agent::core").unwrap();
        assert!(
            ExperienceAgentStatus::new(agent_id.clone(), "", ExperienceAgentState::Idle, 0.4)
                .is_err()
        );
        assert!(ExperienceAgentStatus::new(
            agent_id.clone(),
            "Copilot",
            ExperienceAgentState::Idle,
            -0.1
        )
        .is_err());
        let status =
            ExperienceAgentStatus::new(agent_id, "Copilot", ExperienceAgentState::Running, 0.75)
                .unwrap();
        assert_eq!(status.confidence(), 0.75);
        assert_eq!(status.state(), ExperienceAgentState::Running);
    }

    #[test]
    fn surface_validation() {
        let persona =
            ExperiencePersona::new("Nova", ExperienceIntent::Explore, "playful precision");
        let agent_id = ExperienceSurfaceId::new("agent::core").unwrap();
        let agent =
            ExperienceAgentStatus::new(agent_id, "Copilot", ExperienceAgentState::Idle, 0.5)
                .unwrap();
        let surface = ExperienceSurface::new(
            ExperienceSurfaceId::new("surface::persona").unwrap(),
            "Persona",
            ExperienceLayer::HeadsUp,
            ExperienceSurfaceRole::Persona,
            0.9,
            Some(persona),
            vec![agent],
            vec![],
            vec![],
            true,
        )
        .unwrap();
        assert!(surface.is_active());
        assert_eq!(surface.layer(), ExperienceLayer::HeadsUp);
    }

    #[test]
    fn recover_intent_and_error_state_roundtrip() {
        let persona = ExperiencePersona::new("Flux", ExperienceIntent::Recover, "restorative calm");
        assert_eq!(persona.intent(), ExperienceIntent::Recover);

        let error_agent = ExperienceAgentStatus::new(
            ExperienceSurfaceId::new("agent::failsafe").unwrap(),
            "Failsafe",
            ExperienceAgentState::Error,
            0.4,
        )
        .unwrap();

        let metrics_surface = ExperienceSurface::new(
            ExperienceSurfaceId::new("surface::metrics").unwrap(),
            "Ops Metrics",
            ExperienceLayer::Foundation,
            ExperienceSurfaceRole::Metrics,
            0.5,
            Some(persona.clone()),
            vec![error_agent.clone()],
            vec![],
            vec![],
            false,
        )
        .unwrap();
        assert_eq!(metrics_surface.role(), ExperienceSurfaceRole::Metrics);

        let palette_surface = ExperienceSurface::new(
            ExperienceSurfaceId::new("surface::palette").unwrap(),
            "Command Palette",
            ExperienceLayer::HeadsUp,
            ExperienceSurfaceRole::CommandPalette,
            0.3,
            None,
            vec![],
            vec![],
            vec![],
            false,
        )
        .unwrap();
        assert_eq!(
            palette_surface.role(),
            ExperienceSurfaceRole::CommandPalette
        );

        let agent_id = error_agent.agent_id();
        assert_eq!(agent_id.as_str(), "agent::failsafe");
    }
}
