use crate::app::ack::model::{ExecArgs, ExecRequest, ExecResult};
use crate::app::ack::service::{AckError, AckPort};
use crate::domain::mcp::{
    CapabilityName, McpEventEnvelope, McpSession, PersonaProfile, SessionId, ToolName,
};
use crate::ports::mcp::repo_port::McpSessionRepository;
use serde_json::{json, Value};
use std::sync::Arc;
use thiserror::Error;

pub struct McpBridgeService<A, R>
where
    A: AckPort,
    R: McpSessionRepository,
{
    sessions: Arc<R>,
    ack: Arc<A>,
}

#[derive(Debug, Error)]
pub enum McpBridgeError {
    #[error("protocol violation: {0}")]
    Protocol(String),
    #[error("unsupported tool: {0}")]
    UnsupportedTool(String),
    #[error("tool execution failed: {0}")]
    ToolFailure(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl<A, R> McpBridgeService<A, R>
where
    A: AckPort,
    R: McpSessionRepository,
{
    pub fn new(sessions: Arc<R>, ack: Arc<A>) -> Self {
        Self { sessions, ack }
    }

    pub async fn initialize_session(
        &self,
        persona: Option<String>,
        protocol_version: String,
        capabilities: Vec<String>,
    ) -> Result<McpSession, McpBridgeError> {
        let persona_value = persona.unwrap_or_else(|| "core".to_string());
        let persona_profile: PersonaProfile =
            persona_value.parse().map_err(McpBridgeError::Protocol)?;
        let mut session = McpSession::new(persona_profile);
        let capability_names = capabilities
            .into_iter()
            .map(CapabilityName::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(McpBridgeError::Protocol)?;
        let envelope = session
            .complete_handshake(protocol_version, capability_names)
            .map_err(McpBridgeError::Protocol)?;
        self.sessions.insert(session.clone()).await;
        self.log_event(&session, envelope).await?;
        Ok(session)
    }

    pub async fn list_tools(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "agent.exec",
                    "description": "Execute a shell command inside the Shelldone terminal zone",
                    "inputSchema": {
                        "type": "object",
                        "required": ["cmd"],
                        "properties": {
                            "cmd": {"type": "string", "description": "Command to execute"},
                            "cwd": {"type": "string", "description": "Working directory"},
                            "env": {
                                "type": "object",
                                "description": "Environment variables",
                                "additionalProperties": {"type": "string"}
                            },
                            "shell": {"type": "string", "description": "Override shell binary"}
                        }
                    }
                }
            ]
        })
    }

    pub async fn record_heartbeat(&self, session: &mut McpSession) -> Result<(), McpBridgeError> {
        let envelope = session.heartbeat().map_err(McpBridgeError::Protocol)?;
        self.sessions.update(session.clone()).await;
        self.log_event(session, envelope).await
    }

    pub async fn close_session(
        &self,
        session: &mut McpSession,
        reason: Option<String>,
    ) -> Result<(), McpBridgeError> {
        let envelope = session.close(reason).map_err(McpBridgeError::Protocol)?;
        self.sessions.update(session.clone()).await;
        self.log_event(session, envelope).await
    }

    pub async fn call_tool(
        &self,
        session: &mut McpSession,
        tool_name: &str,
        arguments: Value,
    ) -> Result<ExecResult, McpBridgeError> {
        if tool_name != "agent.exec" {
            return Err(McpBridgeError::UnsupportedTool(tool_name.to_string()));
        }
        let exec_args = parse_exec_args(arguments)?;
        let persona = Some(session.persona().name().to_string());
        let spectral_tag = Some(format!("mcp::{}", tool_name));
        let request = ExecRequest {
            command_id: None,
            persona,
            args: exec_args,
            spectral_tag,
        };
        let exec_result = self.ack.exec(request).await.map_err(McpBridgeError::from)?;
        let envelope = session
            .record_tool_invocation(ToolName::new(tool_name)?)
            .map_err(McpBridgeError::Protocol)?;
        self.sessions.update(session.clone()).await;
        self.log_event(session, envelope).await?;
        Ok(exec_result)
    }

    #[allow(dead_code)]
    pub async fn get_session(&self, id: &SessionId) -> Option<McpSession> {
        self.sessions.get(id).await
    }

    pub async fn list_sessions(&self) -> Vec<McpSession> {
        self.sessions.list().await
    }

    async fn log_event(
        &self,
        session: &McpSession,
        envelope: McpEventEnvelope,
    ) -> Result<(), McpBridgeError> {
        let (kind, payload) = match envelope.event {
            crate::domain::mcp::McpDomainEvent::SessionEstablished {
                protocol_version,
                capabilities,
                ..
            } => (
                "mcp.session.established".to_string(),
                json!({
                    "session_id": envelope.session_id.to_string(),
                    "protocol_version": protocol_version,
                    "capabilities": capabilities
                        .into_iter()
                        .map(|c| c.as_str().to_string())
                        .collect::<Vec<_>>()
                }),
            ),
            crate::domain::mcp::McpDomainEvent::Heartbeat => (
                "mcp.session.heartbeat".to_string(),
                json!({
                    "session_id": envelope.session_id.to_string(),
                    "last_active_at": envelope.occurred_at.to_rfc3339(),
                }),
            ),
            crate::domain::mcp::McpDomainEvent::ToolInvoked { tool } => (
                "mcp.tool.invoked".to_string(),
                json!({
                    "session_id": envelope.session_id.to_string(),
                    "tool": tool.as_str(),
                    "occurred_at": envelope.occurred_at.to_rfc3339(),
                }),
            ),
            crate::domain::mcp::McpDomainEvent::SessionClosed { reason } => (
                "mcp.session.closed".to_string(),
                json!({
                    "session_id": envelope.session_id.to_string(),
                    "reason": reason,
                    "occurred_at": envelope.occurred_at.to_rfc3339(),
                }),
            ),
        };
        self.ack
            .journal_custom(
                kind,
                Some(session.persona().name().to_string()),
                payload,
                Some("mcp".to_string()),
                None,
            )
            .await
            .map_err(McpBridgeError::from)?;
        Ok(())
    }
}

fn parse_exec_args(value: Value) -> Result<ExecArgs, McpBridgeError> {
    let cmd = value
        .get("cmd")
        .and_then(Value::as_str)
        .ok_or_else(|| McpBridgeError::Protocol("cmd is required and must be a string".into()))?
        .to_string();
    let cwd = value
        .get("cwd")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);
    let env = value.get("env").and_then(Value::as_object).map(|map| {
        map.iter()
            .filter_map(|(k, v)| v.as_str().map(|val| (k.clone(), val.to_string())))
            .collect::<std::collections::HashMap<_, _>>()
    });
    let shell = value
        .get("shell")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    ExecArgs::try_new(cmd, cwd, env, shell).map_err(McpBridgeError::Protocol)
}

impl From<AckError> for McpBridgeError {
    fn from(value: AckError) -> Self {
        match value {
            AckError::PolicyDenied { reason } => McpBridgeError::ToolFailure(reason),
            AckError::Invalid(message) => McpBridgeError::Protocol(message),
            AckError::Internal(message) => McpBridgeError::Internal(message),
        }
    }
}

impl From<String> for McpBridgeError {
    fn from(value: String) -> Self {
        McpBridgeError::Protocol(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ack::command_runner::ShellCommandRunner;
    use crate::adapters::mcp::repo_mem::InMemoryMcpSessionRepository;
    use crate::app::ack::service::AckService;
    use crate::continuum::ContinuumStore;
    use crate::policy_engine::PolicyEngine;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::tempdir;

    fn build_bridge() -> (
        Arc<McpBridgeService<AckService<ShellCommandRunner>, InMemoryMcpSessionRepository>>,
        tempfile::TempDir,
    ) {
        let tmp = tempdir().unwrap();
        let journal_path = tmp.path().join("journal.jsonl");
        let policy = PolicyEngine::new(None).unwrap();
        let policy = Arc::new(Mutex::new(policy));
        let continuum = Arc::new(tokio::sync::Mutex::new(ContinuumStore::new(
            journal_path.clone(),
            tmp.path().join("snapshots"),
        )));
        let ack = Arc::new(AckService::new(
            policy,
            continuum,
            journal_path,
            Arc::new(ShellCommandRunner::new()),
            None,
        ));
        let repo = Arc::new(InMemoryMcpSessionRepository::new());
        let bridge = Arc::new(McpBridgeService::new(repo, ack));
        (bridge, tmp)
    }

    #[tokio::test]
    async fn initialize_creates_active_session() {
        let (bridge, tmp) = build_bridge();
        let session = bridge
            .initialize_session(Some("nova".into()), "1.0".into(), vec!["fs".into()])
            .await
            .expect("handshake");
        assert_eq!(session.persona().name(), "nova");
        let journal_path = tmp.path().join("journal.jsonl");
        let mut attempts = 0;
        loop {
            let journal = tokio::fs::read_to_string(&journal_path).await.unwrap();
            if journal.contains("mcp.session.established") {
                break;
            }
            attempts += 1;
            assert!(
                attempts < 10,
                "journal missing session event after retries: {}",
                journal
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn call_tool_executes_command() {
        let (bridge, tmp) = build_bridge();
        let mut session = bridge
            .initialize_session(None, "1.0".into(), vec![])
            .await
            .unwrap();
        let arguments = json!({"cmd": "echo bridge-test"});
        let result = bridge
            .call_tool(&mut session, "agent.exec", arguments)
            .await
            .expect("tool call");
        assert!(result.stdout.contains("bridge-test"));
        let journal_path = tmp.path().join("journal.jsonl");
        let mut attempts = 0;
        loop {
            let journal = tokio::fs::read_to_string(&journal_path).await.unwrap();
            if journal.contains("mcp.tool.invoked") {
                break;
            }
            attempts += 1;
            assert!(
                attempts < 10,
                "journal missing tool invocation after retries: {}",
                journal
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn heartbeat_updates_session() {
        let (bridge, _) = build_bridge();
        let mut session = bridge
            .initialize_session(None, "1.0".into(), vec![])
            .await
            .unwrap();
        bridge
            .record_heartbeat(&mut session)
            .await
            .expect("heartbeat");
    }

    #[test]
    fn parse_exec_args_validates_input() {
        let args = json!({"cmd": "ls", "shell": "/bin/bash"});
        let parsed = parse_exec_args(args).unwrap();
        assert_eq!(parsed.cmd, "ls");
        assert_eq!(parsed.shell.as_deref(), Some("/bin/bash"));
        assert!(parse_exec_args(json!({})).is_err());
    }
}
