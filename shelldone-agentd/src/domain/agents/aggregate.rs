use super::events::{AgentDomainEvent, AgentEventEnvelope};
use super::value_object::{AgentBindingId, AgentProvider, CapabilityName, SdkChannel, SdkVersion};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingStatus {
    Registered,
    Active,
    Disabled,
}

impl BindingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BindingStatus::Registered => "registered",
            BindingStatus::Active => "active",
            BindingStatus::Disabled => "disabled",
        }
    }
}

impl fmt::Display for BindingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilitySet {
    inner: HashSet<CapabilityName>,
}

impl CapabilitySet {
    pub fn new(values: impl IntoIterator<Item = CapabilityName>) -> Result<Self, String> {
        let inner: HashSet<_> = values.into_iter().collect();
        if inner.is_empty() {
            return Err("capability set cannot be empty".into());
        }
        Ok(Self { inner })
    }

    pub fn contains(&self, name: &CapabilityName) -> bool {
        self.inner.contains(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapabilityName> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn to_vec(&self) -> Vec<CapabilityName> {
        self.inner.iter().cloned().collect()
    }
}

#[derive(Clone, Debug)]
pub struct AgentBinding {
    id: AgentBindingId,
    provider: AgentProvider,
    sdk_version: SdkVersion,
    channel: SdkChannel,
    capabilities: CapabilitySet,
    status: BindingStatus,
    registered_at: DateTime<Utc>,
    last_heartbeat_at: Option<DateTime<Utc>>,
}

impl AgentBinding {
    pub fn register(
        provider: AgentProvider,
        sdk_version: SdkVersion,
        channel: SdkChannel,
        capabilities: CapabilitySet,
    ) -> Result<(Self, AgentEventEnvelope), String> {
        let id = AgentBindingId::new();
        let now = Utc::now();
        let binding = Self {
            id: id.clone(),
            provider: provider.clone(),
            sdk_version: sdk_version.clone(),
            channel: channel.clone(),
            capabilities: capabilities.clone(),
            status: BindingStatus::Registered,
            registered_at: now,
            last_heartbeat_at: None,
        };
        let event = AgentEventEnvelope::new(
            id,
            AgentDomainEvent::BindingRegistered {
                provider,
                sdk_version,
                channel,
                capabilities: capabilities.to_vec(),
            },
        );
        Ok((binding, event))
    }

    pub fn activate(&mut self) -> Result<AgentEventEnvelope, String> {
        match self.status {
            BindingStatus::Active => Err("binding already active".into()),
            BindingStatus::Disabled => Err("disabled binding must be re-registered".into()),
            BindingStatus::Registered => {
                self.status = BindingStatus::Active;
                self.last_heartbeat_at = Some(Utc::now());
                Ok(AgentEventEnvelope::new(
                    self.id.clone(),
                    AgentDomainEvent::StatusChanged {
                        status: BindingStatus::Active,
                    },
                ))
            }
        }
    }

    pub fn deactivate(&mut self) -> Result<AgentEventEnvelope, String> {
        match self.status {
            BindingStatus::Registered => Err("binding not active".into()),
            BindingStatus::Disabled => Err("binding already disabled".into()),
            BindingStatus::Active => {
                self.status = BindingStatus::Disabled;
                Ok(AgentEventEnvelope::new(
                    self.id.clone(),
                    AgentDomainEvent::StatusChanged {
                        status: BindingStatus::Disabled,
                    },
                ))
            }
        }
    }

    pub fn update_capabilities(
        &mut self,
        capabilities: CapabilitySet,
    ) -> Result<AgentEventEnvelope, String> {
        if self.capabilities == capabilities {
            return Err("capabilities unchanged".into());
        }
        self.capabilities = capabilities.clone();
        Ok(AgentEventEnvelope::new(
            self.id.clone(),
            AgentDomainEvent::CapabilitiesUpdated {
                capabilities: capabilities.to_vec(),
            },
        ))
    }

    pub fn record_heartbeat(&mut self) -> Result<AgentEventEnvelope, String> {
        if !matches!(self.status, BindingStatus::Active) {
            return Err("heartbeat only allowed in active state".into());
        }
        let now = Utc::now();
        self.last_heartbeat_at = Some(now);
        Ok(AgentEventEnvelope::new(
            self.id.clone(),
            AgentDomainEvent::HeartbeatObserved { at: now },
        ))
    }

    pub fn id(&self) -> AgentBindingId {
        self.id.clone()
    }

    pub fn provider(&self) -> &AgentProvider {
        &self.provider
    }

    pub fn sdk_version(&self) -> &SdkVersion {
        &self.sdk_version
    }

    pub fn channel(&self) -> &SdkChannel {
        &self.channel
    }

    pub fn status(&self) -> &BindingStatus {
        &self.status
    }

    pub fn registered_at(&self) -> DateTime<Utc> {
        self.registered_at
    }

    pub fn last_heartbeat_at(&self) -> Option<DateTime<Utc>> {
        self.last_heartbeat_at
    }

    pub fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_capabilities() -> CapabilitySet {
        CapabilitySet::new(vec![CapabilityName::new("fs.read").unwrap()]).unwrap()
    }

    #[test]
    fn register_emits_event() {
        let (binding, event) = AgentBinding::register(
            AgentProvider::OpenAi,
            SdkVersion::new("1.0.0").unwrap(),
            SdkChannel::Stable,
            mk_capabilities(),
        )
        .unwrap();
        assert!(matches!(binding.status(), BindingStatus::Registered));
        assert!(matches!(
            event.event,
            AgentDomainEvent::BindingRegistered { .. }
        ));
    }

    #[test]
    fn activate_transitions_state() {
        let (mut binding, _) = AgentBinding::register(
            AgentProvider::Claude,
            SdkVersion::new("2.1.0").unwrap(),
            SdkChannel::Preview,
            mk_capabilities(),
        )
        .unwrap();
        let event = binding.activate().unwrap();
        assert!(matches!(binding.status(), BindingStatus::Active));
        assert!(matches!(
            event.event,
            AgentDomainEvent::StatusChanged {
                status: BindingStatus::Active
            }
        ));
        assert!(binding.record_heartbeat().is_ok());
    }

    #[test]
    fn deactivate_requires_active_state() {
        let (mut binding, _) = AgentBinding::register(
            AgentProvider::Microsoft,
            SdkVersion::new("3.0.0").unwrap(),
            SdkChannel::Stable,
            mk_capabilities(),
        )
        .unwrap();
        assert!(binding.deactivate().is_err());
        binding.activate().unwrap();
        binding.deactivate().unwrap();
        assert!(matches!(binding.status(), BindingStatus::Disabled));
        assert!(binding.record_heartbeat().is_err());
    }

    #[test]
    fn update_capabilities_rejects_empty_and_duplicates() {
        let (mut binding, _) = AgentBinding::register(
            AgentProvider::OpenAi,
            SdkVersion::new("1.0.0").unwrap(),
            SdkChannel::Stable,
            mk_capabilities(),
        )
        .unwrap();
        let same = CapabilitySet::new(vec![CapabilityName::new("fs.read").unwrap()]).unwrap();
        assert!(binding.update_capabilities(same).is_err());
        let new_caps = CapabilitySet::new(vec![CapabilityName::new("fs.write").unwrap()]).unwrap();
        binding.update_capabilities(new_caps).unwrap();
    }
}
