use super::aggregate::CapabilityRecord;
use super::value_object::{TerminalBindingId, TerminalId};
use chrono::{DateTime, Utc};

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum TermBridgeDomainEvent {
    CapabilityRecordUpdated {
        record: CapabilityRecord,
    },
    BindingRegistered {
        binding_id: TerminalBindingId,
        terminal: TerminalId,
    },
    BindingTouched {
        binding_id: TerminalBindingId,
        timestamp: DateTime<Utc>,
    },
    BindingRemoved {
        binding_id: TerminalBindingId,
        terminal: TerminalId,
    },
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct TermBridgeEventEnvelope {
    pub occurred_at: DateTime<Utc>,
    pub event: TermBridgeDomainEvent,
}

#[allow(dead_code)]
impl TermBridgeEventEnvelope {
    pub fn new(event: TermBridgeDomainEvent) -> Self {
        Self {
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
        let id = TerminalBindingId::new();
        let envelope = TermBridgeEventEnvelope::new(TermBridgeDomainEvent::BindingRemoved {
            binding_id: id,
            terminal: TerminalId::new("kitty"),
        });
        assert!(envelope.occurred_at <= Utc::now());
    }
}
