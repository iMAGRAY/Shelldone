use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Granted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub command: String,
    pub persona: Option<String>,
    pub reason: String,
    pub requested_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub spectral_tag: Option<String>,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone)]
pub struct NewApprovalRequest {
    pub command: String,
    pub persona: Option<String>,
    pub reason: String,
    pub spectral_tag: Option<String>,
}

#[derive(Debug)]
pub struct ApprovalRegistry {
    path: PathBuf,
    inner: Mutex<HashMap<String, PendingApproval>>,
}

impl ApprovalRegistry {
    pub fn new(state_dir: &Path) -> anyhow::Result<Self> {
        let approvals_dir = state_dir.join("approvals");
        if !approvals_dir.exists() {
            fs::create_dir_all(&approvals_dir)?;
        }
        let path = approvals_dir.join("pending.json");
        let inner = if path.exists() {
            let data = fs::read(&path)?;
            if data.is_empty() {
                HashMap::new()
            } else {
                serde_json::from_slice::<Vec<PendingApproval>>(&data)?
                    .into_iter()
                    .map(|approval| (approval.id.clone(), approval))
                    .collect()
            }
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            inner: Mutex::new(inner),
        })
    }

    pub fn record_request(&self, request: NewApprovalRequest) -> anyhow::Result<PendingApproval> {
        let mut guard = self.inner.lock().expect("approval registry poisoned");
        if let Some(existing) = guard.values().find(|approval| {
            approval.status == ApprovalStatus::Pending
                && approval.command == request.command
                && approval.persona == request.persona
                && approval.reason == request.reason
        }) {
            return Ok(existing.clone());
        }

        let approval = PendingApproval {
            id: uuid::Uuid::new_v4().to_string(),
            command: request.command,
            persona: request.persona,
            reason: request.reason,
            requested_at: Utc::now(),
            resolved_at: None,
            spectral_tag: request.spectral_tag,
            status: ApprovalStatus::Pending,
        };

        guard.insert(approval.id.clone(), approval.clone());
        self.persist_locked(&guard)?;
        Ok(approval)
    }

    pub fn mark_granted(&self, approval_id: &str) -> anyhow::Result<Option<PendingApproval>> {
        let mut guard = self.inner.lock().expect("approval registry poisoned");
        let result = guard.get_mut(approval_id).map(|approval| {
            approval.status = ApprovalStatus::Granted;
            approval.resolved_at = Some(Utc::now());
            approval.clone()
        });
        if result.is_some() {
            self.persist_locked(&guard)?;
        }
        Ok(result)
    }

    pub fn list_pending(&self) -> Vec<PendingApproval> {
        let guard = self.inner.lock().expect("approval registry poisoned");
        let mut approvals: Vec<_> = guard
            .values()
            .filter(|approval| approval.status == ApprovalStatus::Pending)
            .cloned()
            .collect();
        approvals.sort_by_key(|approval| approval.requested_at);
        approvals
    }

    fn persist_locked(&self, approvals: &HashMap<String, PendingApproval>) -> anyhow::Result<()> {
        let mut records: Vec<_> = approvals.values().cloned().collect();
        records.sort_by_key(|approval| approval.requested_at);
        let data = serde_json::to_vec_pretty(&records)?;
        fs::write(&self.path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn records_and_grants_approvals() {
        let temp = tempdir().unwrap();
        let registry = ApprovalRegistry::new(temp.path()).unwrap();

        let req = NewApprovalRequest {
            command: "agent.guard".to_string(),
            persona: Some("nova".to_string()),
            reason: "Approval required for agent.guard".to_string(),
            spectral_tag: None,
        };
        let approval = registry.record_request(req).unwrap();
        assert_eq!(registry.list_pending().len(), 1);

        let granted = registry.mark_granted(&approval.id).unwrap().unwrap();
        assert_eq!(granted.status, ApprovalStatus::Granted);
        assert_eq!(registry.list_pending().len(), 0);
    }
}
