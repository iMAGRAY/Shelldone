use super::super::ports::{
    AgentFrame, AgentFrameStatus, ApprovalFrame, ExperienceTelemetryPort, PersonaFrame,
    TelemetrySnapshot,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dirs_next::config_dir;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::convert::{TryFrom, TryInto};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const DISCOVERY_ENV_KEY: &str = "SHELLDONE_AGENTD_DISCOVERY";
const DISCOVERY_RELATIVE_PATH: &str = "shelldone/agentd.json";
const PENDING_APPROVALS_FILE: &str = "approvals/pending.json";

#[derive(Debug)]
pub struct AgentdTelemetryPort {
    discovery_override: Option<PathBuf>,
}

impl AgentdTelemetryPort {
    pub fn new() -> Self {
        let override_path = env::var_os(DISCOVERY_ENV_KEY).map(PathBuf::from);
        Self {
            discovery_override: override_path,
        }
    }

    fn discovery_path(&self) -> Option<PathBuf> {
        if let Some(path) = &self.discovery_override {
            return Some(path.clone());
        }
        let mut path = config_dir()?;
        path.push(DISCOVERY_RELATIVE_PATH);
        Some(path)
    }

    fn read_discovery(&self) -> Result<Option<DiscoveryDocument>> {
        let Some(path) = self.discovery_path() else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path)
            .with_context(|| format!("reading agentd discovery file at {}", path.display()))?;
        let doc: DiscoveryDocument = serde_json::from_slice(&data)
            .with_context(|| format!("parsing agentd discovery file at {}", path.display()))?;
        Ok(Some(doc))
    }

    fn read_pending_approvals(&self, state_dir: &Path) -> Vec<ApprovalFrame> {
        let path = state_dir.join(PENDING_APPROVALS_FILE);
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(_) => return Vec::new(),
        };
        if data.is_empty() {
            return Vec::new();
        }
        let approvals: Vec<PendingApprovalRecord> = match serde_json::from_slice(&data) {
            Ok(records) => records,
            Err(err) => {
                log::warn!(
                    "experience.telemetry: failed to parse approvals file {}: {err}",
                    path.display()
                );
                return Vec::new();
            }
        };
        approvals
            .into_iter()
            .filter_map(|record| record.try_into().ok())
            .collect()
    }

    fn map_agents(&self, discovery: &DiscoveryDocument) -> Vec<AgentFrame> {
        discovery
            .agents
            .iter()
            .filter_map(|agent| {
                agent
                    .clone()
                    .try_into()
                    .map_err(|err| {
                        log::warn!(
                            "experience.telemetry: skipping agent {} due to {err}",
                            agent.id
                        );
                    })
                    .ok()
            })
            .collect()
    }
}

impl Default for AgentdTelemetryPort {
    fn default() -> Self {
        Self::new()
    }
}

impl ExperienceTelemetryPort for AgentdTelemetryPort {
    fn snapshot(&self) -> Result<TelemetrySnapshot> {
        let Some(discovery) = self.read_discovery()? else {
            return Ok(TelemetrySnapshot::default());
        };

        let generated_at = parse_datetime(&discovery.generated_at);
        let agents = self.map_agents(&discovery);
        let persona = discovery.persona_env.as_ref().map(|name| PersonaFrame {
            name: name.clone(),
            intent_hint: discovery
                .termbridge
                .as_ref()
                .and_then(|status| status.intent_hint()),
            tone_hint: None,
        });

        let approvals = discovery
            .paths
            .as_ref()
            .and_then(|paths| PathBuf::from(&paths.state_dir).canonicalize().ok())
            .map(|state_dir| self.read_pending_approvals(&state_dir))
            .unwrap_or_default();

        Ok(TelemetrySnapshot {
            generated_at,
            persona,
            agents,
            approvals,
            telemetry_ready: discovery.telemetry_ready.unwrap_or(false),
        })
    }
}

fn parse_datetime(input: &Option<String>) -> Option<DateTime<Utc>> {
    input.as_ref().and_then(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%:z")
                    .map(|dt| dt.with_timezone(&Utc))
            })
            .ok()
    })
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryDocument {
    generated_at: Option<String>,
    persona_env: Option<String>,
    #[serde(default)]
    telemetry_ready: Option<bool>,
    agents: Vec<DiscoveryAgent>,
    #[serde(default)]
    termbridge: Option<DiscoveryTermBridge>,
    #[serde(default)]
    paths: Option<DiscoveryPaths>,
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryPaths {
    state_dir: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryTermBridge {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    terminals: Vec<DiscoveryTermBridgeTerminal>,
}

impl DiscoveryTermBridge {
    fn intent_hint(&self) -> Option<String> {
        if let Some(enabled) = self.enabled {
            if !enabled {
                return Some("recover".to_string());
            }
        }
        if self
            .terminals
            .iter()
            .any(|terminal| terminal.requires_opt_in.unwrap_or(false))
        {
            Some("focus".to_string())
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryTermBridgeTerminal {
    #[serde(default)]
    requires_opt_in: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct DiscoveryAgent {
    id: String,
    provider: String,
    version: String,
    channel: String,
    status: String,
    capabilities: Vec<String>,
    #[serde(default)]
    last_heartbeat_at: Option<String>,
    registered_at: String,
}

impl TryFrom<DiscoveryAgent> for AgentFrame {
    type Error = anyhow::Error;

    fn try_from(value: DiscoveryAgent) -> Result<Self> {
        let status = match value.status.as_str() {
            "active" => AgentFrameStatus::Active,
            "registered" => AgentFrameStatus::Registered,
            "disabled" => AgentFrameStatus::Disabled,
            other => anyhow::bail!("unknown agent status {other}"),
        };
        let registered_at = DateTime::parse_from_rfc3339(&value.registered_at)
            .map(|dt| dt.with_timezone(&Utc))
            .with_context(|| format!("parsing registered_at for agent {}", value.id))?;
        let last_heartbeat_at = parse_datetime(&value.last_heartbeat_at);
        let label = format_provider_label(
            &value.provider,
            &value.channel,
            &value.version,
            value.capabilities.len(),
        );
        Ok(Self {
            id: value.id,
            label,
            provider: value.provider,
            channel: Some(value.channel),
            status,
            last_heartbeat_at,
            registered_at,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct PendingApprovalRecord {
    id: String,
    command: String,
    persona: Option<String>,
    reason: String,
    requested_at: String,
    #[allow(dead_code)]
    spectral_tag: Option<String>,
    #[allow(dead_code)]
    status: Option<String>,
}

impl TryFrom<PendingApprovalRecord> for ApprovalFrame {
    type Error = anyhow::Error;

    fn try_from(value: PendingApprovalRecord) -> Result<Self> {
        let requested_at = DateTime::parse_from_rfc3339(&value.requested_at)
            .map(|dt| dt.with_timezone(&Utc))
            .with_context(|| format!("parsing requested_at for approval {}", value.id))?;
        Ok(Self {
            id: value.id,
            command: value.command,
            persona: value.persona,
            reason: value.reason,
            requested_at,
        })
    }
}

fn format_provider_label(
    provider: &str,
    channel: &str,
    version: &str,
    capability_count: usize,
) -> String {
    lazy_static! {
        static ref ALNUM: Regex = Regex::new(r"[^A-Za-z0-9]+").expect("regex compile");
    }
    let pretty_provider = if provider.is_empty() {
        "agent".to_string()
    } else {
        let cleaned = ALNUM.replace_all(provider, " ");
        cleaned
            .split_whitespace()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!("{pretty_provider} {channel} ({version}, {capability_count} caps)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn snapshot_reads_agents_and_approvals() {
        let temp = tempdir().unwrap();
        let discovery_path = temp.path().join("agentd.json");
        let state_dir = temp.path().join("state");
        std::fs::create_dir_all(state_dir.join("approvals")).unwrap();
        let approval_json = serde_json::json!([
            {
                "id": "approval-1",
                "command": "agent.exec",
                "persona": "Nova",
                "reason": "dangerous",
                "requested_at": "2025-10-05T03:10:00Z",
                "status": "pending"
            }
        ]);
        std::fs::write(
            state_dir.join("approvals").join("pending.json"),
            serde_json::to_vec_pretty(&approval_json).unwrap(),
        )
        .unwrap();

        let discovery = serde_json::json!({
            "generated_at": "2025-10-05T03:12:23Z",
            "persona_env": "Nova",
            "telemetry_ready": true,
            "paths": {"state_dir": state_dir.display().to_string()},
            "agents": [
                {
                    "id": "agent-1",
                    "provider": "openai",
                    "version": "1.2.0",
                    "channel": "stable",
                    "status": "active",
                    "capabilities": ["agent.exec"],
                    "last_heartbeat_at": "2025-10-05T03:11:10Z",
                    "registered_at": "2025-10-05T03:00:00Z"
                }
            ]
        });
        std::fs::write(
            &discovery_path,
            serde_json::to_vec_pretty(&discovery).unwrap(),
        )
        .unwrap();

        std::env::set_var("SHELLDONE_AGENTD_DISCOVERY", &discovery_path);
        let port = AgentdTelemetryPort::new();
        let snapshot = port.snapshot().unwrap();
        assert_eq!(snapshot.agents.len(), 1);
        assert_eq!(snapshot.approvals.len(), 1);
        assert!(snapshot.telemetry_ready);
        let agent = &snapshot.agents[0];
        assert_eq!(agent.provider, "openai");
        assert_eq!(agent.channel.as_deref(), Some("stable"));
        assert_eq!(agent.status, AgentFrameStatus::Active);
        assert!(agent.last_heartbeat_at.is_some());
        let approval = &snapshot.approvals[0];
        assert_eq!(approval.command, "agent.exec");
        assert_eq!(approval.persona.as_deref(), Some("Nova"));
        assert_eq!(approval.reason, "dangerous");
        // Clean up override
        std::env::remove_var("SHELLDONE_AGENTD_DISCOVERY");
    }
}
