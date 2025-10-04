use anyhow::{Context, Result};
use lru::LruCache;
use regorus::Engine;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Mutex, RwLock};
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

/// Cache key for policy evaluation results
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct PolicyCacheKey {
    command: String,
    persona: Option<String>,
    spectral_tag: Option<String>,
}

/// Production-grade Rego policy engine with LRU cache
pub struct PolicyEngine {
    engine: RwLock<Engine>,
    enabled: bool,
    #[allow(dead_code)] // Stored for future reload capability
    policy_path: Option<std::path::PathBuf>,
    /// LRU cache for policy evaluation results (256 entries)
    cache: Mutex<LruCache<PolicyCacheKey, PolicyDecision>>,
}

impl PolicyEngine {
    /// Create a new policy engine and load policy from file
    pub fn new(policy_path: Option<&Path>) -> Result<Self> {
        let cache_size = NonZeroUsize::new(256).unwrap();
        let cache = Mutex::new(LruCache::new(cache_size));

        let Some(path) = policy_path else {
            info!("Policy engine disabled (no policy file specified)");
            return Ok(Self {
                engine: RwLock::new(Engine::new()),
                enabled: false,
                policy_path: None,
                cache,
            });
        };

        if !path.exists() {
            warn!(
                "Policy file not found: {}. Policy enforcement disabled.",
                path.display()
            );
            return Ok(Self {
                engine: RwLock::new(Engine::new()),
                enabled: false,
                policy_path: None,
                cache,
            });
        }

        let mut engine = Engine::new();
        let policy_text = std::fs::read_to_string(path)
            .with_context(|| format!("reading policy file {}", path.display()))?;

        engine
            .add_policy(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("policy.rego")
                    .to_string(),
                policy_text,
            )
            .context("adding policy to Rego engine")?;

        info!(
            "Policy engine loaded from {} ({} bytes)",
            path.display(),
            path.metadata()?.len()
        );

        Ok(Self {
            engine: RwLock::new(engine),
            enabled: true,
            policy_path: Some(path.to_path_buf()),
            cache,
        })
    }

    /// Reload policy from disk (hot-reload support)
    /// Wave 2: Will be used for dynamic policy updates
    #[allow(dead_code)]
    pub fn reload(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let Some(path) = &self.policy_path else {
            return Ok(());
        };

        let policy_text = std::fs::read_to_string(path)
            .with_context(|| format!("reloading policy from {}", path.display()))?;

        let mut engine = self.engine.write().map_err(|e| {
            anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e)
        })?;

        // Create new engine to avoid stale state
        let mut new_engine = Engine::new();
        new_engine
            .add_policy(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("policy.rego")
                    .to_string(),
                policy_text,
            )
            .context("reloading policy")?;

        *engine = new_engine;

        // Clear cache on policy reload (invalidate all cached decisions)
        let mut cache = self.cache.lock().map_err(|e| {
            anyhow::anyhow!("failed to acquire cache lock: {}", e)
        })?;
        cache.clear();

        info!("Policy reloaded from {} (cache cleared)", path.display());

        Ok(())
    }

    /// Evaluate ACK command against policy
    pub fn evaluate_ack(&self, input: &AckPolicyInput) -> Result<PolicyDecision> {
        if !self.enabled {
            debug!("Policy engine disabled, allowing by default");
            return Ok(PolicyDecision::allow());
        }

        // Check cache first (hot path optimization)
        let cache_key = PolicyCacheKey {
            command: input.command.clone(),
            persona: input.persona.clone(),
            spectral_tag: input.spectral_tag.clone(),
        };

        {
            let mut cache = self.cache.lock().map_err(|e| {
                anyhow::anyhow!("failed to acquire cache lock: {}", e)
            })?;

            if let Some(cached) = cache.get(&cache_key) {
                debug!("Policy cache hit for {}", input.command);
                return Ok(cached.clone());
            }
        }

        debug!("Policy cache miss, evaluating policy for {}", input.command);

        let input_json =
            serde_json::to_string(input).context("serializing ACK policy input")?;

        let mut engine = self.engine.write().map_err(|e| {
            anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e)
        })?;

        engine
            .set_input_json(&input_json)
            .context("setting policy input")?;

        let result = engine
            .eval_query("data.shelldone.policy.allow".to_string(), false)
            .context("evaluating policy query data.shelldone.policy.allow")?;

        // Empty result means false (policy didn't match)
        let decision = if result.result.is_empty() {
            let deny_reasons = self.extract_deny_reasons_internal(&mut engine)?;
            debug!("Policy denied ACK command: {:?}", deny_reasons);
            PolicyDecision::deny(deny_reasons)
        } else {
            let allowed = result
                .result
                .first()
                .and_then(|r| r.expressions.first())
                .and_then(|e| e.value.as_bool().ok().copied())
                .unwrap_or(false);

            if !allowed {
                let deny_reasons = self.extract_deny_reasons_internal(&mut engine)?;
                debug!("Policy denied ACK command: {:?}", deny_reasons);
                PolicyDecision::deny(deny_reasons)
            } else {
                debug!("Policy allowed ACK command: {}", input.command);
                PolicyDecision::allow()
            }
        };

        // Cache the result for future lookups
        {
            let mut cache = self.cache.lock().map_err(|e| {
                anyhow::anyhow!("failed to acquire cache lock: {}", e)
            })?;
            cache.put(cache_key, decision.clone());
        }

        Ok(decision)
    }

    /// Evaluate OSC escape sequence against policy
    /// Wave 2: OSC filtering integration with Σ-pty proxy
    #[allow(dead_code)]
    pub fn evaluate_osc(&self, osc_code: u32, operation: &str) -> Result<PolicyDecision> {
        if !self.enabled {
            return Ok(PolicyDecision::allow());
        }

        let input = serde_json::json!({
            "osc_code": osc_code,
            "operation": operation,
        });
        let input_str = serde_json::to_string(&input).context("serializing OSC input")?;

        let mut engine = self.engine.write().map_err(|e| {
            anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e)
        })?;

        engine
            .set_input_json(&input_str)
            .context("setting OSC policy input")?;

        let result = engine
            .eval_query("data.shelldone.policy.allow_osc".to_string(), false)
            .context("evaluating OSC policy")?;

        if result.result.is_empty() {
            return Ok(PolicyDecision::deny(vec![format!(
                "OSC {} {} not allowed by policy (no match)",
                osc_code, operation
            )]));
        }

        let allowed = result
            .result
            .first()
            .and_then(|r| r.expressions.first())
            .and_then(|e| e.value.as_bool().ok().copied())
            .unwrap_or(false);

        if !allowed {
            return Ok(PolicyDecision::deny(vec![format!(
                "OSC {} {} explicitly denied by policy",
                osc_code, operation
            )]));
        }

        Ok(PolicyDecision::allow())
    }

    /// Extract deny reasons from policy evaluation (internal, assumes lock held)
    fn extract_deny_reasons_internal(&self, engine: &mut Engine) -> Result<Vec<String>> {
        let result = engine
            .eval_query("data.shelldone.policy.deny_reason".to_string(), false)
            .context("extracting deny reasons")?;

        let mut reasons = Vec::new();

        for res in result.result {
            for expr in res.expressions {
                // deny_reason is typically a set of strings
                if let Ok(set) = expr.value.as_set() {
                    for item in set {
                        if let Ok(s) = item.as_string() {
                            reasons.push(s.to_string());
                        }
                    }
                }
                // Fallback: check if it's a single string
                else if let Ok(s) = expr.value.as_string() {
                    reasons.push(s.to_string());
                }
                // Fallback: check if it's an array
                else if let Ok(arr) = expr.value.as_array() {
                    for item in arr {
                        if let Ok(s) = item.as_string() {
                            reasons.push(s.to_string());
                        }
                    }
                }
            }
        }

        if reasons.is_empty() {
            reasons.push("Policy denied without specific reason".to_string());
        }

        Ok(reasons)
    }
}

/// Input structure for ACK policy evaluation
#[derive(Debug, Clone, serde::Serialize)]
pub struct AckPolicyInput {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

    /// Builder method for tests
    #[allow(dead_code)]
    pub fn with_approval(mut self, granted: bool) -> Self {
        self.approval_granted = granted;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_policy() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
package shelldone.policy

import rego.v1

allowed_commands := {{
    "agent.plan",
    "agent.exec",
    "agent.journal",
    "agent.inspect",
}}

approval_required_commands := {{
    "agent.guard",
    "agent.undo",
    "agent.connect",
}}

default allow := false

allow if {{
    input.persona in {{"core", "flux"}}
    input.command in allowed_commands
}}

allow if {{
    input.command in approval_required_commands
    input.approval_granted == true
}}

deny_reason contains msg if {{
    not allow
    msg := sprintf("Command %v denied for persona %v", [input.command, input.persona])
}}

default allow_osc := false

allow_osc if {{
    input.osc_code in {{0, 2, 4, 8, 133, 1337}}
}}

allow_osc if {{
    input.osc_code == 52
    input.operation == "write"
}}
"#
        )
        .unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn policy_allows_core_persona_exec() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = AckPolicyInput::new(
            "agent.exec".to_string(),
            Some("core".to_string()),
            Some("exec::test".to_string()),
        );

        let decision = engine.evaluate_ack(&input).unwrap();
        assert!(decision.is_allowed(), "Expected exec to be allowed");
    }

    #[test]
    fn policy_denies_guard_without_approval() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = AckPolicyInput::new(
            "agent.guard".to_string(),
            Some("core".to_string()),
            None,
        );

        let decision = engine.evaluate_ack(&input).unwrap();
        assert!(!decision.is_allowed(), "Expected guard to be denied");
        assert!(!decision.deny_reasons.is_empty());
        assert!(decision.deny_reasons[0].contains("guard"));
    }

    #[test]
    fn policy_allows_guard_with_approval() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = AckPolicyInput::new(
            "agent.guard".to_string(),
            Some("core".to_string()),
            None,
        )
        .with_approval(true);

        let decision = engine.evaluate_ack(&input).unwrap();
        assert!(
            decision.is_allowed(),
            "Expected guard with approval to be allowed"
        );
    }

    #[test]
    fn policy_allows_safe_osc() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let decision = engine.evaluate_osc(133, "marker").unwrap();
        assert!(decision.is_allowed(), "OSC 133 should be allowed");
    }

    #[test]
    fn policy_allows_osc52_write() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let decision = engine.evaluate_osc(52, "write").unwrap();
        assert!(decision.is_allowed(), "OSC 52 write should be allowed");
    }

    #[test]
    fn policy_denies_osc52_read() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let decision = engine.evaluate_osc(52, "read").unwrap();
        assert!(!decision.is_allowed(), "OSC 52 read should be denied");
    }

    #[test]
    fn policy_denies_unsafe_osc() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let decision = engine.evaluate_osc(999, "test").unwrap();
        assert!(!decision.is_allowed(), "OSC 999 should be denied");
    }

    #[test]
    fn policy_engine_disabled_allows_all() {
        let engine = PolicyEngine::new(None).unwrap();

        let input = AckPolicyInput::new(
            "agent.unknown".to_string(),
            Some("test".to_string()),
            None,
        );

        let decision = engine.evaluate_ack(&input).unwrap();
        assert!(
            decision.is_allowed(),
            "Disabled engine should allow everything"
        );
    }

    #[test]
    fn policy_reload_works() {
        use std::io::Seek;
        
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
package shelldone.policy
import rego.v1
default allow := false
allow if {{ input.command == "test" }}
"#
        )
        .unwrap();
        file.flush().unwrap();

        let engine = PolicyEngine::new(Some(file.path())).unwrap();

        let input = AckPolicyInput::new("test".to_string(), None, None);
        assert!(engine.evaluate_ack(&input).unwrap().is_allowed());

        // Update policy
        file.as_file_mut().seek(std::io::SeekFrom::Start(0)).unwrap();
        file.as_file_mut().set_len(0).unwrap();
        writeln!(
            file,
            r#"
package shelldone.policy
import rego.v1
default allow := false
allow if {{ input.command == "other" }}
"#
        )
        .unwrap();
        file.flush().unwrap();

        engine.reload().unwrap();

        // Old command should now fail
        assert!(!engine.evaluate_ack(&input).unwrap().is_allowed());

        // New command should work
        let new_input = AckPolicyInput::new("other".to_string(), None, None);
        assert!(engine.evaluate_ack(&new_input).unwrap().is_allowed());
    }
}
