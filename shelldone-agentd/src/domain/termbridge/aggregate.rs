use super::value_object::{
    CurrentWorkingDirectory, TerminalBindingId, TerminalCapabilities, TerminalId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    #[default]
    Local,
    Mcp,
    Bootstrap,
    #[serde(other)]
    External,
}

impl CapabilitySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilitySource::Local => "local",
            CapabilitySource::Mcp => "mcp",
            CapabilitySource::Bootstrap => "bootstrap",
            CapabilitySource::External => "external",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecord {
    pub terminal: TerminalId,
    pub display_name: String,
    pub requires_opt_in: bool,
    pub capabilities: TerminalCapabilities,
    pub notes: Vec<String>,
    #[serde(default)]
    pub source: CapabilitySource,
}

impl CapabilityRecord {
    pub fn new(
        terminal: TerminalId,
        display_name: impl Into<String>,
        requires_opt_in: bool,
        capabilities: TerminalCapabilities,
        notes: Vec<String>,
    ) -> Self {
        Self {
            terminal,
            display_name: display_name.into(),
            requires_opt_in,
            capabilities,
            notes,
            source: CapabilitySource::Local,
        }
    }

    pub fn with_source(mut self, source: CapabilitySource) -> Self {
        self.source = source;
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalBinding {
    pub id: TerminalBindingId,
    pub terminal: TerminalId,
    pub token: String,
    pub labels: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub ipc_endpoint: Option<String>,
}

impl TerminalBinding {
    pub fn new(
        terminal: TerminalId,
        token: impl Into<String>,
        labels: HashMap<String, String>,
        ipc_endpoint: Option<String>,
    ) -> Self {
        Self {
            id: TerminalBindingId::new(),
            terminal,
            token: token.into(),
            labels,
            created_at: Utc::now(),
            ipc_endpoint,
        }
    }

    pub fn cwd(&self) -> Option<&str> {
        self.labels.get("cwd").map(|value| value.as_str())
    }

    pub fn set_cwd(&mut self, cwd: &CurrentWorkingDirectory) {
        self.labels.insert("cwd".to_string(), String::from(cwd));
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TermBridgeState {
    discovered_at: Option<DateTime<Utc>>,
    records: Vec<CapabilityRecord>,
}

impl TermBridgeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn discovered_at(&self) -> Option<DateTime<Utc>> {
        self.discovered_at
    }

    pub fn capabilities(&self) -> Vec<CapabilityRecord> {
        self.records.clone()
    }

    pub fn update_capabilities(&mut self, records: Vec<CapabilityRecord>) -> bool {
        use std::collections::BTreeMap;

        let mut dedup_map: BTreeMap<String, CapabilityRecord> = BTreeMap::new();
        for record in records {
            dedup_map.insert(record.terminal.as_str().to_string(), record);
        }
        let mut sanitized: Vec<CapabilityRecord> = dedup_map.into_values().collect();
        sanitized.sort_by(|a, b| a.terminal.as_str().cmp(b.terminal.as_str()));
        let changed = sanitized != self.records;
        self.records = sanitized;
        self.discovered_at = Some(Utc::now());
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_record_constructor() {
        let record = CapabilityRecord::new(
            TerminalId::new("kitty"),
            "Kitty",
            false,
            TerminalCapabilities::builder().send_text(true).build(),
            vec!["note".to_string()],
        );
        assert_eq!(record.display_name, "Kitty");
        assert!(record.capabilities.send_text);
    }

    #[test]
    fn state_updates_timestamp() {
        let mut state = TermBridgeState::new();
        assert!(state.discovered_at().is_none());
        let records = vec![CapabilityRecord::new(
            TerminalId::new("wezterm"),
            "WezTerm",
            false,
            TerminalCapabilities::builder().spawn(true).build(),
            Vec::new(),
        )];
        assert!(state.update_capabilities(records.clone()));
        assert!(state.discovered_at().is_some());
        assert_eq!(state.capabilities(), records);
    }

    #[test]
    fn update_capabilities_deduplicates_and_reports_changes() {
        let mut state = TermBridgeState::new();
        let wezterm = CapabilityRecord::new(
            TerminalId::new("wezterm"),
            "WezTerm",
            false,
            TerminalCapabilities::builder().send_text(true).build(),
            Vec::new(),
        );
        let kitty = CapabilityRecord::new(
            TerminalId::new("kitty"),
            "Kitty",
            false,
            TerminalCapabilities::builder().spawn(true).build(),
            Vec::new(),
        );
        assert!(state.update_capabilities(vec![wezterm.clone(), kitty.clone(), wezterm.clone()]));
        let caps = state.capabilities();
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].terminal.as_str(), "kitty");
        assert_eq!(caps[1].terminal.as_str(), "wezterm");
        assert!(!state.update_capabilities(vec![kitty.clone(), wezterm.clone()]));
        assert!(state.update_capabilities(vec![kitty]));
    }

    #[test]
    fn binding_builder_sets_defaults() {
        let binding =
            TerminalBinding::new(TerminalId::new("wezterm"), "token", HashMap::new(), None);
        assert_eq!(binding.terminal.as_str(), "wezterm");
        assert_eq!(binding.token, "token");
    }

    #[test]
    fn binding_set_cwd_updates_label() {
        let mut binding =
            TerminalBinding::new(TerminalId::new("wezterm"), "token", HashMap::new(), None);
        let cwd = CurrentWorkingDirectory::new("/work").unwrap();
        binding.set_cwd(&cwd);
        assert_eq!(binding.cwd(), Some("/work"));
    }
}
