use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Serializable event entry persisted in the Continuum journal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub event_id: String,
    pub kind: String,
    pub timestamp: String,
    pub persona: Option<String>,
    pub payload: Value,
    pub spectral_tag: Option<String>,
    pub bytes: Option<usize>,
}

impl EventRecord {
    pub fn new(
        kind: &str,
        persona: Option<String>,
        payload: Value,
        event_id: Option<String>,
        spectral_tag: Option<String>,
        bytes: Option<usize>,
    ) -> Self {
        Self {
            event_id: event_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            kind: kind.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            persona,
            payload,
            spectral_tag,
            bytes,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExecArgs {
    pub cmd: String,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub shell: Option<String>,
}

impl ExecArgs {
    pub fn try_new(
        cmd: String,
        cwd: Option<PathBuf>,
        env: Option<HashMap<String, String>>,
        shell: Option<String>,
    ) -> Result<Self, String> {
        if cmd.trim().is_empty() {
            return Err("command cannot be empty".into());
        }
        Ok(Self {
            cmd,
            cwd,
            env: env.unwrap_or_default(),
            shell,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ExecRequest {
    pub command_id: Option<String>,
    pub persona: Option<String>,
    pub args: ExecArgs,
    pub spectral_tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExecResult {
    pub event_id: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub spectral_tag: String,
    pub duration_ms: f64,
}

#[derive(Clone, Debug)]
pub struct UndoRequest {
    pub persona: Option<String>,
    pub snapshot_id: String,
    pub spectral_tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UndoResult {
    pub snapshot_id: String,
    pub restored_events: usize,
    pub duration_ms: f64,
}
