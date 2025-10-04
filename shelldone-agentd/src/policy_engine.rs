use anyhow::Result;
use std::path::Path;
use tracing::{debug, info, warn};

/// Policy evaluation result
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub deny_reasons: Vec<String>,
}

impl PolicyDecision {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            deny_reasons: vec![],
        }
    }

    pub fn deny(reasons: Vec<String>) -> Self {
        Self {
            allowed: false,
            deny_reasons: reasons,
        }
    }

    pub fn is_allowed(&self) -> bool {
        self.allowed
    }
}

/// Policy engine stub (Rego integration TODO)
pub struct PolicyEngine {
    enabled: bool,
}

impl PolicyEngine {
    pub fn new(policy_path: Option<&Path>) -> Result<Self> {
        if let Some(path) = policy_path {
            if path.exists() {
                warn!(
                    "Policy file found at {}, but Rego integration is not yet complete. Allowing all operations.",
                    path.display()
                );
            }
        }
        
        info!("Policy engine stub active (TODO: integrate Rego runtime)");
        Ok(Self { enabled: false })
    }

    pub fn evaluate_ack(&mut self, _input: &AckPolicyInput) -> Result<PolicyDecision> {
        // TODO: Integrate Rego evaluation
        // For now, allow all but log
        debug!("Policy check (stub): allowing all ACK commands");
        Ok(PolicyDecision::allow())
    }

    pub fn evaluate_osc(&mut self, osc_code: u32, operation: &str) -> Result<PolicyDecision> {
        // Hardcoded safe list as fallback
        let allowed_codes = [0, 2, 4, 8, 52, 133, 1337];
        if allowed_codes.contains(&osc_code) {
            Ok(PolicyDecision::allow())
        } else {
            Ok(PolicyDecision::deny(vec![format!(
                "OSC {} {} not in hardcoded allowlist",
                osc_code, operation
            )]))
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AckPolicyInput {
    pub command: String,
    pub persona: Option<String>,
    pub spectral_tag: Option<String>,
    pub approval_granted: bool,
}

impl AckPolicyInput {
    pub fn new(command: String, persona: Option<String>, spectral_tag: Option<String>) -> Self {
        Self {
            command,
            persona,
            spectral_tag,
            approval_granted: false,
        }
    }

    pub fn with_approval(mut self, granted: bool) -> Self {
        self.approval_granted = granted;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_allows_all_ack() {
        let mut engine = PolicyEngine::new(None).unwrap();
        let input = AckPolicyInput::new(
            "agent.exec".to_string(),
            Some("core".to_string()),
            None,
        );
        let decision = engine.evaluate_ack(&input).unwrap();
        assert!(decision.is_allowed());
    }

    #[test]
    fn stub_filters_unsafe_osc() {
        let mut engine = PolicyEngine::new(None).unwrap();
        let decision = engine.evaluate_osc(999, "test").unwrap();
        assert!(!decision.is_allowed());
    }
}
