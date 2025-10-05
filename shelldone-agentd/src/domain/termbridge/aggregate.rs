use super::value_object::{
    CurrentWorkingDirectory, TerminalBindingId, TerminalCapabilities, TerminalId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecord {
    pub terminal: TerminalId,
    pub display_name: String,
    pub requires_opt_in: bool,
    pub capabilities: TerminalCapabilities,
    pub notes: Vec<String>,
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
        }
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

    pub fn update_capabilities(&mut self, records: Vec<CapabilityRecord>) {
        self.records = records;
        self.discovered_at = Some(Utc::now());
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
        state.update_capabilities(records.clone());
        assert!(state.discovered_at().is_some());
        assert_eq!(state.capabilities(), records);
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
