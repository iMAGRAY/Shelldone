use super::value_object::{CapabilityName, PersonaProfile, SessionId, ToolName};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq)]
pub enum McpDomainEvent {
    SessionEstablished {
        persona: PersonaProfile,
        protocol_version: String,
        capabilities: Vec<CapabilityName>,
    },
    Heartbeat,
    ToolInvoked {
        tool: ToolName,
    },
    SessionClosed {
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct McpEventEnvelope {
    pub session_id: SessionId,
    pub occurred_at: DateTime<Utc>,
    pub event: McpDomainEvent,
}

impl McpEventEnvelope {
    pub fn new(session_id: SessionId, event: McpDomainEvent) -> Self {
        Self {
            session_id,
            occurred_at: Utc::now(),
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_records_timestamp() {
        let session = SessionId::new();
        let envelope = McpEventEnvelope::new(session.clone(), McpDomainEvent::Heartbeat);
        assert_eq!(session, envelope.session_id);
        assert!(envelope.occurred_at <= Utc::now());
    }
}
