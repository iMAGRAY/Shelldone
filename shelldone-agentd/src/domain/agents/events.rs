use super::aggregate::BindingStatus;
use super::value_object::{AgentBindingId, AgentProvider, CapabilityName, SdkChannel, SdkVersion};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentDomainEvent {
    BindingRegistered {
        provider: AgentProvider,
        sdk_version: SdkVersion,
        channel: SdkChannel,
        capabilities: Vec<CapabilityName>,
    },
    StatusChanged {
        status: BindingStatus,
    },
    CapabilitiesUpdated {
        capabilities: Vec<CapabilityName>,
    },
    HeartbeatObserved {
        at: DateTime<Utc>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentEventEnvelope {
    pub binding_id: AgentBindingId,
    pub occurred_at: DateTime<Utc>,
    pub event: AgentDomainEvent,
}

impl AgentEventEnvelope {
    pub fn new(binding_id: AgentBindingId, event: AgentDomainEvent) -> Self {
        Self {
            binding_id,
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
        let id = AgentBindingId::new();
        let envelope = AgentEventEnvelope::new(
            id.clone(),
            AgentDomainEvent::StatusChanged {
                status: BindingStatus::Registered,
            },
        );
        assert_eq!(id, envelope.binding_id);
        assert!(envelope.occurred_at <= Utc::now());
    }
}
