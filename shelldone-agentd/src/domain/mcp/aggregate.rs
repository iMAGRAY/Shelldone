use super::events::{McpDomainEvent, McpEventEnvelope};
use super::value_object::{CapabilityName, PersonaProfile, SessionId, ToolName};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Negotiating,
    Active,
    Closed,
}

/// Aggregate capturing the lifecycle of an MCP session.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct McpSession {
    id: SessionId,
    persona: PersonaProfile,
    protocol_version: Option<String>,
    capabilities: HashSet<CapabilityName>,
    status: SessionStatus,
    created_at: DateTime<Utc>,
    last_active_at: DateTime<Utc>,
}

#[allow(dead_code)]
impl McpSession {
    pub fn new(persona: PersonaProfile) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            persona,
            protocol_version: None,
            capabilities: HashSet::new(),
            status: SessionStatus::Negotiating,
            created_at: now,
            last_active_at: now,
        }
    }

    pub fn id(&self) -> SessionId {
        self.id.clone()
    }

    pub fn status(&self) -> &SessionStatus {
        &self.status
    }

    pub fn persona(&self) -> &PersonaProfile {
        &self.persona
    }

    pub fn protocol_version(&self) -> Option<&String> {
        self.protocol_version.as_ref()
    }

    pub fn last_active_at(&self) -> DateTime<Utc> {
        self.last_active_at
    }

    pub fn capability_names(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .map(|cap| cap.as_str().to_string())
            .collect()
    }

    pub fn to_snapshot(&self) -> McpSessionSnapshot {
        McpSessionSnapshot {
            id: self.id.clone(),
            persona: self.persona.clone(),
            protocol_version: self.protocol_version.clone(),
            capabilities: self.capabilities.iter().cloned().collect(),
            status: self.status.clone(),
            created_at: self.created_at,
            last_active_at: self.last_active_at,
        }
    }

    pub fn from_snapshot(snapshot: McpSessionSnapshot) -> Result<Self, String> {
        let McpSessionSnapshot {
            id,
            persona,
            protocol_version,
            capabilities,
            status,
            created_at,
            last_active_at,
        } = snapshot;

        if matches!(status, SessionStatus::Negotiating) && protocol_version.is_some() {
            return Err("negotiating session cannot have protocol version".into());
        }

        let mut session = Self {
            id,
            persona,
            protocol_version,
            capabilities: capabilities.into_iter().collect(),
            status,
            created_at,
            last_active_at,
        };

        if session.last_active_at < session.created_at {
            session.last_active_at = session.created_at;
        }

        Ok(session)
    }

    /// Completes the handshake and transitions session into active state.
    pub fn complete_handshake(
        &mut self,
        protocol_version: String,
        capabilities: impl IntoIterator<Item = CapabilityName>,
    ) -> Result<McpEventEnvelope, String> {
        if matches!(self.status, SessionStatus::Closed) {
            return Err("handshake attempted on closed session".into());
        }
        if self.protocol_version.is_some() {
            return Err("handshake already completed".into());
        }
        if protocol_version.trim().is_empty() {
            return Err("protocol version cannot be empty".into());
        }
        self.protocol_version = Some(protocol_version.clone());
        self.capabilities = capabilities.into_iter().collect();
        self.status = SessionStatus::Active;
        self.last_active_at = Utc::now();
        Ok(McpEventEnvelope::new(
            self.id.clone(),
            McpDomainEvent::SessionEstablished {
                persona: self.persona.clone(),
                protocol_version,
                capabilities: self.capabilities.iter().cloned().collect(),
            },
        ))
    }

    /// Records a heartbeat coming from the MCP client.
    pub fn heartbeat(&mut self) -> Result<McpEventEnvelope, String> {
        if !matches!(self.status, SessionStatus::Active) {
            return Err("heartbeat allowed only for active sessions".into());
        }
        self.last_active_at = Utc::now();
        Ok(McpEventEnvelope::new(
            self.id.clone(),
            McpDomainEvent::Heartbeat,
        ))
    }

    /// Records invocation of a tool, enforcing active session state.
    pub fn record_tool_invocation(&mut self, tool: ToolName) -> Result<McpEventEnvelope, String> {
        if !matches!(self.status, SessionStatus::Active) {
            return Err("tools can be invoked only on active sessions".into());
        }
        self.last_active_at = Utc::now();
        Ok(McpEventEnvelope::new(
            self.id.clone(),
            McpDomainEvent::ToolInvoked { tool },
        ))
    }

    pub fn close(&mut self, reason: Option<String>) -> Result<McpEventEnvelope, String> {
        if matches!(self.status, SessionStatus::Closed) {
            return Err("session already closed".into());
        }
        self.status = SessionStatus::Closed;
        self.last_active_at = Utc::now();
        Ok(McpEventEnvelope::new(
            self.id.clone(),
            McpDomainEvent::SessionClosed { reason },
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpSessionSnapshot {
    pub id: SessionId,
    pub persona: PersonaProfile,
    pub protocol_version: Option<String>,
    pub capabilities: Vec<CapabilityName>,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_session() -> McpSession {
        McpSession::new(PersonaProfile::Core)
    }

    #[test]
    fn handshake_sets_state_once() {
        let mut session = mk_session();
        let event = session
            .complete_handshake("1.0".to_string(), vec![CapabilityName::new("fs").unwrap()])
            .unwrap();
        assert!(matches!(session.status(), SessionStatus::Active));
        assert_eq!(
            event.event,
            McpDomainEvent::SessionEstablished {
                persona: PersonaProfile::Core,
                protocol_version: "1.0".into(),
                capabilities: vec![CapabilityName::new("fs").unwrap()],
            }
        );
        let err = session
            .complete_handshake("1.0".to_string(), vec![])
            .unwrap_err();
        assert!(err.contains("already completed"));
    }

    #[test]
    fn heartbeat_requires_active_state() {
        let mut session = mk_session();
        assert!(session.heartbeat().is_err());
        session
            .complete_handshake("1.0".to_string(), Vec::<CapabilityName>::new())
            .unwrap();
        assert!(session.heartbeat().is_ok());
    }

    #[test]
    fn tool_invocation_enforces_state() {
        let mut session = mk_session();
        assert!(session
            .record_tool_invocation(ToolName::new("agent.exec").unwrap())
            .is_err());
        session
            .complete_handshake("1.0".to_string(), Vec::<CapabilityName>::new())
            .unwrap();
        let event = session
            .record_tool_invocation(ToolName::new("agent.exec").unwrap())
            .unwrap();
        assert!(matches!(event.event, McpDomainEvent::ToolInvoked { .. }));
    }

    #[test]
    fn close_transitions_and_emits_event() {
        let mut session = mk_session();
        session
            .complete_handshake("1.0".to_string(), Vec::<CapabilityName>::new())
            .unwrap();
        let event = session.close(Some("client shutdown".into())).unwrap();
        assert!(matches!(session.status(), SessionStatus::Closed));
        assert!(matches!(
            event.event,
            McpDomainEvent::SessionClosed { reason: Some(_) }
        ));
        assert!(session.close(None).is_err());
    }
}
