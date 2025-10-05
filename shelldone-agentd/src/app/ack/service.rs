use super::approvals::{ApprovalRegistry, NewApprovalRequest, PendingApproval};
use super::model::{EventRecord, ExecArgs, ExecRequest, ExecResult, UndoRequest, UndoResult};
use crate::continuum::{ContinuumSnapshot, ContinuumStore};
use crate::policy_engine::{AckPolicyInput, PolicyDecision, PolicyEngine};
use crate::ports::ack::command_runner::CommandRunner;
use crate::telemetry::PrismMetrics;
use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{error, warn};

#[derive(thiserror::Error, Debug)]
pub enum AckError {
    #[error("policy denied: {reason}")]
    PolicyDenied { reason: String },
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AckResult<T> = Result<T, AckError>;

#[async_trait]
pub trait AckPort: Send + Sync {
    async fn exec(&self, request: ExecRequest) -> AckResult<ExecResult>;

    async fn journal_custom(
        &self,
        kind: String,
        persona: Option<String>,
        payload: serde_json::Value,
        spectral_tag: Option<String>,
        bytes: Option<usize>,
    ) -> AckResult<EventRecord>;
}

/// Application service orchestrating ACK command execution, journaling, and undo.
pub struct AckService<R: CommandRunner> {
    policy_engine: Arc<Mutex<PolicyEngine>>,
    continuum_store: Arc<tokio::sync::Mutex<ContinuumStore>>,
    journal_path: Arc<PathBuf>,
    command_runner: Arc<R>,
    metrics: Option<Arc<PrismMetrics>>,
    approvals: Arc<ApprovalRegistry>,
}

impl<R: CommandRunner> AckService<R> {
    pub fn new(
        policy_engine: Arc<Mutex<PolicyEngine>>,
        continuum_store: Arc<tokio::sync::Mutex<ContinuumStore>>,
        journal_path: PathBuf,
        command_runner: Arc<R>,
        metrics: Option<Arc<PrismMetrics>>,
        approvals: Arc<ApprovalRegistry>,
    ) -> Self {
        Self {
            policy_engine,
            continuum_store,
            journal_path: Arc::new(journal_path),
            command_runner,
            metrics,
            approvals,
        }
    }

    pub fn journal_path(&self) -> &Path {
        self.journal_path.as_path()
    }

    pub async fn exec(&self, request: ExecRequest) -> AckResult<ExecResult> {
        let policy_input = AckPolicyInput::new(
            "agent.exec".to_string(),
            request.persona.clone(),
            request.spectral_tag.clone(),
        );
        let decision = self.evaluate_policy(&policy_input)?;
        if !decision.is_allowed() {
            self.record_policy_metrics("agent.exec", false, request.persona.as_deref());
            if Self::requires_approval(&decision.deny_reasons) {
                if let Err(err) = self
                    .record_approval_request("agent.exec", &request, &decision.deny_reasons)
                    .await
                {
                    warn!("Failed to record approval request: {err:#}");
                }
            }
            let reason = decision.deny_reasons.join("; ");
            self.log_policy_denial("agent.exec", &reason);
            return Err(AckError::PolicyDenied { reason });
        }
        self.record_policy_metrics("agent.exec", true, request.persona.as_deref());

        let start = chrono::Utc::now();
        let output = self
            .command_runner
            .run(&request.args)
            .await
            .map_err(|err| AckError::Internal(err.to_string()))?;
        let duration_ms = (chrono::Utc::now() - start).num_milliseconds() as f64;

        if let Some(metrics) = &self.metrics {
            metrics.record_exec_latency(duration_ms, request.persona.as_deref());
        }

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let spectral_tag = request
            .spectral_tag
            .clone()
            .unwrap_or_else(|| "exec::default".to_string());
        let event_id = request
            .command_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let event = EventRecord::new(
            "exec",
            request.persona.clone(),
            json!({
                "command": request.args.cmd,
                "cwd": request.args.cwd.as_ref().map(|c| c.display().to_string()),
                "env_keys": request.args.env.keys().collect::<Vec<_>>(),
                "exit_code": exit_code,
                "stdout_len": stdout.len(),
                "stderr_len": stderr.len(),
                "duration_ms": duration_ms as i64,
            }),
            Some(event_id.clone()),
            Some(spectral_tag.clone()),
            Some(stdout.len() + stderr.len()),
        );

        self.append_event(&event)
            .await
            .map_err(|err| AckError::Internal(err.to_string()))?;

        Ok(ExecResult {
            event_id,
            exit_code,
            stdout,
            stderr,
            spectral_tag,
            duration_ms,
        })
    }

    pub async fn journal_custom(
        &self,
        kind: String,
        persona: Option<String>,
        payload: serde_json::Value,
        spectral_tag: Option<String>,
        bytes: Option<usize>,
    ) -> AckResult<EventRecord> {
        if kind.trim().is_empty() {
            return Err(AckError::Invalid("journal kind cannot be empty".into()));
        }

        if kind == "sigma.guard" {
            if let Some(metrics) = &self.metrics {
                let direction = payload
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let reason = payload
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unspecified");
                metrics.record_sigma_guard_event(direction, reason);
            }
        }
        let event = EventRecord::new(&kind, persona, payload, None, spectral_tag, bytes);
        self.append_event(&event)
            .await
            .map_err(|err| AckError::Internal(err.to_string()))?;
        Ok(event)
    }

    pub async fn undo(&self, request: UndoRequest) -> AckResult<UndoResult> {
        let policy_input = AckPolicyInput::new(
            "agent.undo".to_string(),
            request.persona.clone(),
            request.spectral_tag.clone(),
        );
        let decision = self.evaluate_policy(&policy_input)?;
        if !decision.is_allowed() {
            self.record_policy_metrics("agent.undo", false, request.persona.as_deref());
            if Self::requires_approval(&decision.deny_reasons) {
                if let Ok(args) = ExecArgs::try_new("agent.undo".to_string(), None, None, None) {
                    let exec_request = ExecRequest {
                        command_id: Some(request.snapshot_id.clone()),
                        persona: request.persona.clone(),
                        args,
                        spectral_tag: request.spectral_tag.clone(),
                    };
                    if let Err(err) = self
                        .record_approval_request(
                            "agent.undo",
                            &exec_request,
                            &decision.deny_reasons,
                        )
                        .await
                    {
                        warn!("Failed to record approval request: {err:#}");
                    }
                }
            }
            let reason = decision.deny_reasons.join("; ");
            self.log_policy_denial("agent.undo", &reason);
            return Err(AckError::PolicyDenied { reason });
        }
        self.record_policy_metrics("agent.undo", true, request.persona.as_deref());

        let start = chrono::Utc::now();
        let store = self.continuum_store.lock().await;
        let snapshots = store
            .list_snapshots()
            .map_err(|e| AckError::Internal(format!("list_snapshots failed: {e}")))?;

        let snapshot_path = snapshots
            .iter()
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.contains(&request.snapshot_id))
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                AckError::Invalid(format!("snapshot {} not found", request.snapshot_id))
            })?;

        let snapshot = ContinuumSnapshot::load(snapshot_path).map_err(|e| {
            AckError::Internal(format!("load_snapshot {}: {e}", snapshot_path.display()))
        })?;

        let events = snapshot
            .restore_events()
            .map_err(|e| AckError::Internal(format!("restore_events failed: {e}")))?;

        let restored_count = events.len();
        let duration_ms = (chrono::Utc::now() - start).num_milliseconds() as f64;

        if let Some(metrics) = &self.metrics {
            metrics.record_undo_latency(duration_ms, &request.snapshot_id);
            metrics.record_events_restored(restored_count as u64);
        }

        let undo_event = EventRecord::new(
            "undo",
            request.persona.clone(),
            json!({
                "snapshot_id": request.snapshot_id,
                "restored_events": restored_count,
                "duration_ms": duration_ms as i64,
            }),
            None,
            request.spectral_tag.clone(),
            None,
        );

        self.append_event(&undo_event)
            .await
            .map_err(|err| AckError::Internal(err.to_string()))?;

        Ok(UndoResult {
            snapshot_id: request.snapshot_id,
            restored_events: restored_count,
            duration_ms,
        })
    }

    pub async fn append_event(&self, event: &EventRecord) -> anyhow::Result<()> {
        let dir = self
            .journal_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        tokio::fs::create_dir_all(&dir).await?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.journal_path())
            .await?;
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        file.write_all(&line).await?;
        Ok(())
    }

    fn evaluate_policy(&self, input: &AckPolicyInput) -> AckResult<PolicyDecision> {
        self.policy_engine
            .lock()
            .map_err(|e| AckError::Internal(format!("policy lock poisoned: {e}")))?
            .evaluate_ack(input)
            .map_err(|e| AckError::Internal(e.to_string()))
    }

    fn record_policy_metrics(&self, command: &str, allowed: bool, persona: Option<&str>) {
        if let Some(metrics) = &self.metrics {
            metrics.record_policy_evaluation(allowed);
            if !allowed {
                metrics.record_policy_denial(command, persona);
            }
        }
    }

    fn log_policy_denial(&self, command: &str, reason: &str) {
        warn!("Policy denied {command}: {reason}");
    }

    fn requires_approval(reasons: &[String]) -> bool {
        reasons
            .iter()
            .any(|reason| reason.to_ascii_lowercase().contains("approval required"))
    }

    async fn record_approval_request(
        &self,
        origin: &str,
        request: &ExecRequest,
        reasons: &[String],
    ) -> anyhow::Result<PendingApproval> {
        let approval = self.approvals.record_request(NewApprovalRequest {
            command: request.args.cmd.clone(),
            persona: request.persona.clone(),
            reason: reasons.join("; "),
            spectral_tag: request.spectral_tag.clone(),
        })?;

        let event = EventRecord::new(
            "approval.requested",
            approval.persona.clone(),
            json!({
                "approval_id": approval.id,
                "command": approval.command,
                "reason": approval.reason,
                "status": "pending",
                "origin_command": origin,
            }),
            Some(approval.id.clone()),
            Some("approval".to_string()),
            None,
        );

        self.append_event(&event).await?;
        Ok(approval)
    }

    pub async fn grant_approval(&self, approval_id: &str) -> AckResult<PendingApproval> {
        let approval = self
            .approvals
            .mark_granted(approval_id)
            .map_err(|err| AckError::Internal(err.to_string()))?
            .ok_or_else(|| AckError::Invalid(format!("approval {approval_id} not found")))?;

        let event = EventRecord::new(
            "approval.granted",
            approval.persona.clone(),
            json!({
                "approval_id": approval.id,
                "command": approval.command,
                "reason": approval.reason,
                "status": "granted",
            }),
            Some(approval.id.clone()),
            Some("approval".to_string()),
            None,
        );

        self.append_event(&event)
            .await
            .map_err(|err| AckError::Internal(err.to_string()))?;

        Ok(approval)
    }
}

#[async_trait]
impl<R: CommandRunner + 'static> AckPort for AckService<R> {
    async fn exec(&self, request: ExecRequest) -> AckResult<ExecResult> {
        AckService::exec(self, request).await
    }

    async fn journal_custom(
        &self,
        kind: String,
        persona: Option<String>,
        payload: serde_json::Value,
        spectral_tag: Option<String>,
        bytes: Option<usize>,
    ) -> AckResult<EventRecord> {
        AckService::journal_custom(self, kind, persona, payload, spectral_tag, bytes).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ack::command_runner::ShellCommandRunner;
    use crate::app::ack::model::ExecArgs;
    use crate::policy_engine::PolicyEngine;
    use tempfile::tempdir;

    fn build_service() -> AckService<ShellCommandRunner> {
        let tmp_root = tempdir().unwrap();
        #[allow(deprecated)]
        let tmp = tmp_root.into_path();
        let journal_path = tmp.join("journal.jsonl");
        let policy = PolicyEngine::new(None).unwrap();
        let approvals = Arc::new(ApprovalRegistry::new(&tmp).unwrap());
        AckService::new(
            Arc::new(Mutex::new(policy)),
            Arc::new(tokio::sync::Mutex::new(ContinuumStore::new(
                journal_path.clone(),
                tmp.join("snapshots"),
            ))),
            journal_path.as_path().into(),
            Arc::new(ShellCommandRunner::new()),
            None,
            approvals,
        )
    }

    #[tokio::test]
    async fn exec_writes_event() {
        let service = build_service();
        let args = ExecArgs::try_new("echo test".into(), None, None, None).unwrap();
        let request = ExecRequest {
            command_id: None,
            persona: Some("core".into()),
            args,
            spectral_tag: Some("exec::test".into()),
        };
        let result = service.exec(request).await.unwrap();
        assert_eq!(result.exit_code, 0);
        let journal = tokio::fs::read_to_string(service.journal_path())
            .await
            .unwrap();
        assert!(journal.contains("\"exec\""));
    }

    #[tokio::test]
    async fn journal_custom_rejects_empty_kind() {
        let service = build_service();
        let err = service
            .journal_custom("".into(), None, json!({}), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, AckError::Invalid(_)));
    }
}
