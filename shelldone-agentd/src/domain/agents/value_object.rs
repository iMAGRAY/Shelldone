use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AgentBindingId(Uuid);

impl AgentBindingId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}

impl fmt::Display for AgentBindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for AgentBindingId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for AgentBindingId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let uuid = Uuid::parse_str(&value).map_err(serde::de::Error::custom)?;
        Ok(Self::from_uuid(uuid))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentProvider {
    OpenAi,
    Claude,
    Microsoft,
    Custom(String),
}

impl AgentProvider {
    pub fn slug(&self) -> &str {
        match self {
            AgentProvider::OpenAi => "openai",
            AgentProvider::Claude => "claude",
            AgentProvider::Microsoft => "microsoft",
            AgentProvider::Custom(name) => name.as_str(),
        }
    }
}

impl fmt::Display for AgentProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug())
    }
}

impl FromStr for AgentProvider {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "openai" => Ok(AgentProvider::OpenAi),
            "claude" => Ok(AgentProvider::Claude),
            "microsoft" | "azure" => Ok(AgentProvider::Microsoft),
            other if !other.trim().is_empty() => Ok(AgentProvider::Custom(other.to_string())),
            _ => Err("provider must be non-empty".into()),
        }
    }
}

impl Serialize for AgentProvider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.slug())
    }
}

impl<'de> Deserialize<'de> for AgentProvider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SdkVersion(String);

impl SdkVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if !is_semver_like(&value) {
            return Err("sdk version must follow semver-like pattern".into());
        }
        Ok(Self(value))
    }
}

fn is_semver_like(value: &str) -> bool {
    static SEMVER_RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"^(0|[1-9]\d*)(\.(0|[1-9]\d*))?(\.(0|[1-9]\d*))?(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$")
            .expect("valid semver regex")
    });
    SEMVER_RE.is_match(value)
}

impl fmt::Display for SdkVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for SdkVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SdkVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        SdkVersion::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SdkChannel {
    Stable,
    Preview,
    Custom(String),
}

impl SdkChannel {
    pub fn label(&self) -> &str {
        match self {
            SdkChannel::Stable => "stable",
            SdkChannel::Preview => "preview",
            SdkChannel::Custom(label) => label.as_str(),
        }
    }
}

impl fmt::Display for SdkChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl FromStr for SdkChannel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "stable" => Ok(SdkChannel::Stable),
            "preview" | "beta" => Ok(SdkChannel::Preview),
            other if !other.trim().is_empty() => Ok(SdkChannel::Custom(other.to_string())),
            _ => Err("channel must be non-empty".into()),
        }
    }
}

impl Serialize for SdkChannel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.label())
    }
}

impl<'de> Deserialize<'de> for SdkChannel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CapabilityName(String);

impl CapabilityName {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("capability name cannot be empty".into());
        }
        Ok(Self(value))
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
        CapabilityName::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_from_str_variants() {
        assert_eq!(AgentProvider::OpenAi, "OpenAI".parse().unwrap());
        assert_eq!(AgentProvider::Claude, "claude".parse().unwrap());
        assert_eq!(AgentProvider::Microsoft, "azure".parse().unwrap());
        assert!(matches!(
            "custom".parse::<AgentProvider>().unwrap(),
            AgentProvider::Custom(_)
        ));
    }

    #[test]
    fn sdk_version_validation() {
        assert!(SdkVersion::new("1.2.3").is_ok());
        assert!(SdkVersion::new("1.0.0-beta.1").is_ok());
        assert!(SdkVersion::new("abc").is_err());
    }

    #[test]
    fn channel_from_str() {
        assert_eq!(SdkChannel::Stable, "stable".parse().unwrap());
        assert_eq!(SdkChannel::Preview, "beta".parse().unwrap());
        assert!(matches!(
            "nightly".parse::<SdkChannel>().unwrap(),
            SdkChannel::Custom(_)
        ));
    }

    #[test]
    fn capability_name_rejects_empty() {
        assert!(CapabilityName::new("").is_err());
        assert!(CapabilityName::new("fs").is_ok());
    }
}
