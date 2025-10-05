use crate::app::ack::service::AckPort;
use crate::app::mcp::service::{McpBridgeError, McpBridgeService};
use crate::domain::mcp::SessionId;
use crate::ports::mcp::repo_port::McpSessionRepository;
use prost_types::Timestamp;
use serde_json::Value;
use std::sync::Arc;
use std::time::SystemTime;
use tonic::{async_trait, Response, Status};
use uuid::Uuid;

pub mod proto {
    tonic::include_proto!("shelldone.mcp");
}

use proto::mcp_bridge_server::{McpBridge, McpBridgeServer};
use proto::{
    CallToolRequest, CallToolResponse, HeartbeatRequest, HeartbeatResponse, InitializeRequest,
    InitializeResponse, ListToolsRequest, ListToolsResponse, ToolDescriptor,
};

#[derive(Clone)]
pub struct GrpcBridge<A, R>
where
    A: AckPort + 'static,
    R: McpSessionRepository + 'static,
{
    bridge: Arc<McpBridgeService<A, R>>,
}

impl<A, R> GrpcBridge<A, R>
where
    A: AckPort + 'static,
    R: McpSessionRepository + 'static,
{
    pub fn new(bridge: Arc<McpBridgeService<A, R>>) -> Self {
        Self { bridge }
    }

    pub fn into_server(self) -> McpBridgeServer<Self> {
        McpBridgeServer::new(self)
    }
}

#[async_trait]
impl<A, R> McpBridge for GrpcBridge<A, R>
where
    A: AckPort + Send + Sync + 'static,
    R: McpSessionRepository + Send + Sync + 'static,
{
    async fn initialize(
        &self,
        request: tonic::Request<InitializeRequest>,
    ) -> Result<Response<InitializeResponse>, Status> {
        let payload = request.into_inner();
        if payload.protocol_version.trim().is_empty() {
            return Err(Status::invalid_argument("protocol_version is required"));
        }

        let session = self
            .bridge
            .initialize_session(
                optional_string(payload.persona),
                payload.protocol_version.clone(),
                payload.capabilities.clone(),
            )
            .await
            .map_err(map_bridge_error)?;

        let response = InitializeResponse {
            session_id: session.id().to_string(),
            protocol_version: payload.protocol_version,
            capabilities: session.capability_names().into_iter().collect(),
        };

        Ok(Response::new(response))
    }

    async fn list_tools(
        &self,
        _request: tonic::Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        let tools_json = self.bridge.list_tools().await;
        let mut tools = Vec::new();
        if let Some(array) = tools_json.get("tools").and_then(Value::as_array) {
            for tool in array {
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let input_schema_json = tool
                    .get("inputSchema")
                    .map(|schema| schema.to_string())
                    .unwrap_or_else(|| "{}".into());
                tools.push(ToolDescriptor {
                    name,
                    description,
                    input_schema_json,
                });
            }
        }
        Ok(Response::new(ListToolsResponse { tools }))
    }

    async fn call_tool(
        &self,
        request: tonic::Request<CallToolRequest>,
    ) -> Result<Response<CallToolResponse>, Status> {
        let payload = request.into_inner();
        let session_id = parse_session_id(&payload.session_id)?;
        let arguments = parse_json(&payload.arguments_json)?;

        let mut session = self
            .bridge
            .get_session(&session_id)
            .await
            .ok_or_else(|| Status::not_found("session not found"))?;

        let exec = self
            .bridge
            .call_tool(&mut session, &payload.tool_name, arguments)
            .await
            .map_err(map_bridge_error)?;

        let response = CallToolResponse {
            exit_code: exec.exit_code,
            stdout: exec.stdout,
            stderr: exec.stderr,
            event_id: exec.event_id,
            spectral_tag: exec.spectral_tag,
            duration_ms: exec.duration_ms,
            is_error: exec.exit_code != 0,
        };

        Ok(Response::new(response))
    }

    async fn heartbeat(
        &self,
        request: tonic::Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let payload = request.into_inner();
        let session_id = parse_session_id(&payload.session_id)?;
        let mut session = self
            .bridge
            .get_session(&session_id)
            .await
            .ok_or_else(|| Status::not_found("session not found"))?;
        self.bridge
            .record_heartbeat(&mut session)
            .await
            .map_err(map_bridge_error)?;
        let timestamp = Timestamp::from(SystemTime::now());
        Ok(Response::new(HeartbeatResponse {
            occurred_at: Some(timestamp),
        }))
    }
}

#[allow(clippy::result_large_err)]
fn parse_session_id(raw: &str) -> Result<SessionId, Status> {
    let uuid = Uuid::parse_str(raw).map_err(|_| Status::invalid_argument("invalid session_id"))?;
    Ok(SessionId::from_uuid(uuid))
}

#[allow(clippy::result_large_err)]
fn parse_json(raw: &str) -> Result<Value, Status> {
    if raw.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(raw).map_err(|err| Status::invalid_argument(err.to_string()))
}

fn optional_string(input: String) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn map_bridge_error(err: McpBridgeError) -> Status {
    match err {
        McpBridgeError::Protocol(reason) => Status::invalid_argument(reason),
        McpBridgeError::UnsupportedTool(tool) => {
            Status::unimplemented(format!("unsupported tool: {tool}"))
        }
        McpBridgeError::ToolFailure(reason) => Status::failed_precondition(reason),
        McpBridgeError::Internal(reason) => Status::internal(reason),
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
    use tempfile::tempdir;
    use tonic::Request;

    type TestBridge =
        McpBridgeService<AckService<ShellCommandRunner>, InMemoryMcpSessionRepository>;

    fn build_bridge() -> Arc<TestBridge> {
        let tmp = tempdir().unwrap();
        let journal_path = tmp.path().join("journal.jsonl");
        let policy = PolicyEngine::new(None).unwrap();
        let continuum = Arc::new(tokio::sync::Mutex::new(ContinuumStore::new(
            journal_path.clone(),
            tmp.path().join("snapshots"),
        )));
        let ack = Arc::new(AckService::new(
            Arc::new(Mutex::new(policy)),
            continuum,
            journal_path,
            Arc::new(ShellCommandRunner::new()),
            None,
        ));
        Arc::new(McpBridgeService::new(
            Arc::new(InMemoryMcpSessionRepository::new()),
            ack,
        ))
    }

    #[tokio::test]
    async fn initialize_returns_session() {
        let bridge = build_bridge();
        let grpc = GrpcBridge::new(bridge);
        let request = InitializeRequest {
            persona: "core".into(),
            protocol_version: "1.0".into(),
            capabilities: vec!["fs".into()],
        };
        let response = grpc
            .initialize(Request::new(request))
            .await
            .expect("initialize response")
            .into_inner();
        assert!(!response.session_id.is_empty());
        assert_eq!(response.protocol_version, "1.0");
    }

    #[tokio::test]
    async fn call_tool_executes_command() {
        let bridge = build_bridge();
        let grpc = GrpcBridge::new(bridge.clone());
        let init = InitializeRequest {
            persona: "core".into(),
            protocol_version: "1.0".into(),
            capabilities: vec![],
        };
        let session_id = grpc
            .initialize(Request::new(init))
            .await
            .unwrap()
            .into_inner()
            .session_id;

        let call = CallToolRequest {
            session_id,
            tool_name: "agent.exec".into(),
            arguments_json: "{\"cmd\":\"echo grpc\"}".into(),
        };

        let response = grpc
            .call_tool(Request::new(call))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.exit_code, 0);
        assert!(response.stdout.contains("grpc"));
        assert!(!response.event_id.is_empty());
    }
}
