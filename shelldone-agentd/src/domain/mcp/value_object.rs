use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Strongly typed identifier for an MCP session.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

#[allow(dead_code)]
impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for SessionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let uuid = Uuid::parse_str(&value).map_err(D::Error::custom)?;
        Ok(SessionId::from_uuid(uuid))
    }
}

/// Persona profile negotiated during handshake.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PersonaProfile {
    Nova,
    Core,
    Flux,
    Custom(String),
}

impl PersonaProfile {
    pub fn name(&self) -> &str {
        match self {
            PersonaProfile::Nova => "nova",
            PersonaProfile::Core => "core",
            PersonaProfile::Flux => "flux",
            PersonaProfile::Custom(name) => name.as_str(),
        }
    }
}

impl FromStr for PersonaProfile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "nova" => Ok(PersonaProfile::Nova),
            "core" => Ok(PersonaProfile::Core),
            "flux" => Ok(PersonaProfile::Flux),
            other if !other.trim().is_empty() => Ok(PersonaProfile::Custom(other.to_string())),
            _ => Err("persona must be a non-empty string".to_string()),
        }
    }
}

impl Serialize for PersonaProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for PersonaProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

/// MCP tool names are case sensitive; wrap them to prevent accidental mixing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ToolName(String);

impl ToolName {
    pub fn new(name: impl Into<String>) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("tool name cannot be empty".to_string());
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ToolName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ToolName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        ToolName::new(value).map_err(D::Error::custom)
    }
}

/// Capability names declared during handshake.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CapabilityName(String);

impl CapabilityName {
    pub fn new(name: impl Into<String>) -> Result<Self, String> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("capability name cannot be empty".to_string());
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CapabilityName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for CapabilityName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CapabilityName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        CapabilityName::new(value).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persona_from_str_variants() {
        assert_eq!(PersonaProfile::Nova, "Nova".parse().unwrap());
        assert_eq!(PersonaProfile::Core, "core".parse().unwrap());
        assert_eq!(PersonaProfile::Flux, "FLUX".parse().unwrap());
        let custom: PersonaProfile = "ops".parse().unwrap();
        assert_eq!("ops", custom.name());
        assert!("".parse::<PersonaProfile>().is_err());
    }

    #[test]
    fn tool_name_validation() {
        assert!(ToolName::new("agent.exec").is_ok());
        assert!(ToolName::new("  ").is_err());
    }

    #[test]
    fn capability_name_validation() {
        assert!(CapabilityName::new("clipboard").is_ok());
        assert!(CapabilityName::new("").is_err());
    }

    #[test]
    fn session_id_wraps_uuid() {
        let uuid = Uuid::new_v4();
        let session = SessionId::from_uuid(uuid);
        assert_eq!(uuid, session.as_uuid());
        assert_eq!(uuid.to_string(), session.to_string());
    }
}
