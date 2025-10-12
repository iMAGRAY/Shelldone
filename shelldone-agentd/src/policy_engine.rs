use anyhow::{Context, Result};
use lru::LruCache;
use regorus::Engine;
use serde::Serialize;
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

        let mut engine = self
            .engine
            .write()
            .map_err(|e| anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e))?;

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
        let mut cache = self
            .cache
            .lock()
            .map_err(|e| anyhow::anyhow!("failed to acquire cache lock: {}", e))?;
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
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| anyhow::anyhow!("failed to acquire cache lock: {}", e))?;

            if let Some(cached) = cache.get(&cache_key) {
                debug!("Policy cache hit for {}", input.command);
                return Ok(cached.clone());
            }
        }

        debug!("Policy cache miss, evaluating policy for {}", input.command);

        let input_json = serde_json::to_string(input).context("serializing ACK policy input")?;

        let mut engine = self
            .engine
            .write()
            .map_err(|e| anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e))?;

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
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| anyhow::anyhow!("failed to acquire cache lock: {}", e))?;
            cache.put(cache_key, decision.clone());
        }

        Ok(decision)
    }

    /// Evaluate TermBridge action against policy (clipboard, spawn, send_text)
    pub fn evaluate_termbridge(&self, input: &TermBridgePolicyInput) -> Result<PolicyDecision> {
        if !self.enabled {
            debug!("Policy engine disabled, allowing termbridge action by default");
            return Ok(PolicyDecision::allow());
        }

        let cache_key = PolicyCacheKey {
            command: input.action.clone(),
            persona: input.persona.clone(),
            spectral_tag: None,
        };

        {
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| anyhow::anyhow!("failed to acquire cache lock: {}", e))?;
            if let Some(cached) = cache.get(&cache_key) {
                debug!("TermBridge policy cache hit for {}", input.action);
                return Ok(cached.clone());
            }
        }

        let input_json =
            serde_json::to_string(input).context("serializing TermBridge policy input")?;

        let mut engine = self
            .engine
            .write()
            .map_err(|e| anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e))?;

        engine
            .set_input_json(&input_json)
            .context("setting termbridge policy input")?;

        let result = engine
            .eval_query("data.shelldone.policy.termbridge_allow".to_string(), false)
            .context("evaluating policy query data.shelldone.policy.termbridge_allow")?;

        let mut allowed = true;
        if let Some(res) = result.result.first() {
            if let Some(expr) = res.expressions.first() {
                if let Ok(value) = expr.value.as_bool() {
                    allowed = *value;
                }
            }
        }

        let decision = if allowed {
            PolicyDecision::allow()
        } else {
            let reasons = self.extract_termbridge_deny_reasons_internal(&mut engine)?;
            PolicyDecision::deny(reasons)
        };

        let mut cache = self
            .cache
            .lock()
            .map_err(|e| anyhow::anyhow!("failed to acquire cache lock: {}", e))?;
        cache.put(cache_key, decision.clone());

        Ok(decision)
    }

    /// Evaluate TLS configuration against policy (hot-path on startup and reload)
    pub fn evaluate_tls(&self, input: &TlsPolicyInput) -> Result<PolicyDecision> {
        if !self.enabled {
            return Ok(PolicyDecision::allow());
        }

        let input_json = serde_json::to_string(input).context("serializing TLS policy input")?;

        let mut engine = self
            .engine
            .write()
            .map_err(|e| anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e))?;

        engine
            .set_input_json(&input_json)
            .context("setting TLS policy input")?;

        let result = engine
            .eval_query("data.shelldone.policy.tls_allow".to_string(), false)
            .context("evaluating TLS policy")?;

        if result.result.is_empty() {
            let reasons = self.extract_tls_deny_reasons_internal(&mut engine)?;
            return Ok(PolicyDecision::deny(reasons));
        }

        let allowed = result
            .result
            .first()
            .and_then(|r| r.expressions.first())
            .and_then(|e| e.value.as_bool().ok().copied())
            .unwrap_or(false);

        if !allowed {
            let reasons = self.extract_tls_deny_reasons_internal(&mut engine)?;
            return Ok(PolicyDecision::deny(reasons));
        }

        Ok(PolicyDecision::allow())
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

        let mut engine = self
            .engine
            .write()
            .map_err(|e| anyhow::anyhow!("failed to acquire write lock on policy engine: {}", e))?;

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
        self.extract_reasons_internal(
            engine,
            "data.shelldone.policy.deny_reason",
            "Policy denied without specific reason",
        )
    }

    fn extract_tls_deny_reasons_internal(&self, engine: &mut Engine) -> Result<Vec<String>> {
        self.extract_reasons_internal(
            engine,
            "data.shelldone.policy.tls_deny_reason",
            "TLS configuration denied without specific reason",
        )
    }

    fn extract_termbridge_deny_reasons_internal(&self, engine: &mut Engine) -> Result<Vec<String>> {
        self.extract_reasons_internal(
            engine,
            "data.shelldone.policy.termbridge_deny_reason",
            "TermBridge action denied without specific reason",
        )
    }

    fn extract_reasons_internal(
        &self,
        engine: &mut Engine,
        query: &str,
        default_reason: &str,
    ) -> Result<Vec<String>> {
        let result = engine
            .eval_query(query.to_string(), false)
            .context("extracting deny reasons")?;

        let mut reasons = Vec::new();

        for res in result.result {
            for expr in res.expressions {
                if let Ok(set) = expr.value.as_set() {
                    for item in set {
                        if let Ok(s) = item.as_string() {
                            reasons.push(s.to_string());
                        }
                    }
                } else if let Ok(s) = expr.value.as_string() {
                    reasons.push(s.to_string());
                } else if let Ok(arr) = expr.value.as_array() {
                    for item in arr {
                        if let Ok(s) = item.as_string() {
                            reasons.push(s.to_string());
                        }
                    }
                }
            }
        }

        if reasons.is_empty() {
            reasons.push(default_reason.to_string());
        }

        Ok(reasons)
    }
}

/// Input structure for ACK policy evaluation
#[derive(Debug, Clone, Serialize)]
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

/// Input structure for TLS policy evaluation
#[derive(Debug, Clone, Serialize)]
pub struct TlsPolicyInput {
    pub listener: String,
    pub cipher_policy: String,
    pub tls_versions: Vec<String>,
    pub client_auth_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_fingerprint_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_fingerprint_sha256: Option<String>,
}

/// Input for TermBridge policy evaluation
#[derive(Debug, Clone, Serialize)]
pub struct TermBridgePolicyInput {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_opt_in: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent_granted: Option<bool>,
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

termbridge_allowed_actions := {{
    "spawn",
    "send_text",
    "focus",
    "duplicate",
    "close",
    "clipboard.write",
    "clipboard.read",
    "cwd.update",
}}

 default termbridge_allow := false

 termbridge_require_consent if {{
     input.requires_opt_in == true
     not input.consent_granted == true
 }}

 termbridge_allow if {{
     input.action in {{"spawn", "send_text", "focus", "duplicate", "close"}}
     not termbridge_require_consent
 }}

termbridge_allow if {{
    input.action == "clipboard.write"
    not termbridge_clipboard_exceeds_limit
}}

termbridge_allow if {{
    input.action == "clipboard.read"
}}

 termbridge_allow if {{
     input.action == "cwd.update"
     input.cwd
     count(input.cwd) <= 4096
     not termbridge_require_consent
 }}

termbridge_deny_reason contains msg if {{
    not termbridge_allow
    input.action == "cwd.update"
    msg := sprintf(
        "TermBridge action %v denied (cwd=%v)",
        [input.action, input.cwd]
    )
}}

termbridge_clipboard_exceeds_limit if {{
    input.action == "clipboard.write"
    input.bytes
    input.bytes > 4096
}}

termbridge_deny_reason contains msg if {{
    not termbridge_allow
    msg := sprintf(
        "TermBridge action %v denied (bytes=%v, cwd=%v)",
        [input.action, input.bytes, input.cwd]
    )
}}

default allow_osc := false

allow_osc if {{
    input.osc_code in {{0, 2, 4, 8, 133, 1337}}
}}

allow_osc if {{
    input.osc_code == 52
    input.operation == "write"
}}

default tls_allow := false

tls_allow if {{
    input.cipher_policy == "strict"
}}

tls_allow if {{
    input.cipher_policy == "balanced"
    input.client_auth_required == true
}}

tls_deny_reason contains msg if {{
    not tls_allow
    msg := sprintf("TLS policy rejected (%v, mTLS=%v)", [input.cipher_policy, input.client_auth_required])
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

        let input = AckPolicyInput::new("agent.guard".to_string(), Some("core".to_string()), None);

        let decision = engine.evaluate_ack(&input).unwrap();
        assert!(!decision.is_allowed(), "Expected guard to be denied");
        assert!(!decision.deny_reasons.is_empty());
        assert!(decision.deny_reasons[0].contains("guard"));
    }

    #[test]
    fn policy_termbridge_allows_clipboard_write() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "clipboard.write".to_string(),
            persona: Some("core".to_string()),
            terminal: None,
            command: None,
            backend: Some("wl-copy".to_string()),
            channel: Some("clipboard".to_string()),
            bytes: Some(1024),
            cwd: None,
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(decision.is_allowed(), "Clipboard write should be allowed");
    }

    #[test]
    fn policy_termbridge_denies_large_clipboard() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "clipboard.write".to_string(),
            persona: Some("core".to_string()),
            terminal: None,
            command: None,
            backend: Some("wl-copy".to_string()),
            channel: Some("clipboard".to_string()),
            bytes: Some(10_000),
            cwd: None,
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(
            !decision.is_allowed(),
            "Large clipboard payload should be denied"
        );
        assert!(decision
            .deny_reasons
            .iter()
            .any(|reason| reason.contains("TermBridge action")));
    }

    #[test]
    fn policy_termbridge_allows_send_text() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "send_text".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("wezterm".to_string()),
            command: Some("ls".to_string()),
            backend: None,
            channel: None,
            bytes: Some(128),
            cwd: None,
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(decision.is_allowed(), "send_text should be allowed");
    }

    #[test]
    fn policy_termbridge_allows_focus() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "focus".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("wezterm".to_string()),
            command: None,
            backend: None,
            channel: None,
            bytes: None,
            cwd: None,
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(decision.is_allowed(), "focus should be allowed");
    }

    #[test]
    fn policy_termbridge_allows_duplicate() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "duplicate".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("wezterm".to_string()),
            command: Some("htop".to_string()),
            backend: None,
            channel: None,
            bytes: None,
            cwd: Some("/workspace".to_string()),
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(decision.is_allowed(), "duplicate should be allowed");
    }

    #[test]
    fn policy_termbridge_allows_close() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "close".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("wezterm".to_string()),
            command: None,
            backend: None,
            channel: None,
            bytes: None,
            cwd: None,
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(decision.is_allowed(), "close should be allowed");
    }

    #[test]
    fn policy_termbridge_allows_cwd_update() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "cwd.update".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("wezterm".to_string()),
            command: None,
            backend: None,
            channel: None,
            bytes: None,
            cwd: Some("/workspace".to_string()),
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(decision.is_allowed(), "cwd.update should be allowed");
    }

    #[test]
    fn policy_termbridge_denies_oversized_cwd_update() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let oversized = "a".repeat(5000);
        let input = TermBridgePolicyInput {
            action: "cwd.update".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("wezterm".to_string()),
            command: None,
            backend: None,
            channel: None,
            bytes: None,
            cwd: Some(oversized),
            requires_opt_in: Some(false),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(
            !decision.is_allowed(),
            "oversized cwd.update should be denied by policy"
        );
        assert!(
            decision
                .deny_reasons
                .iter()
                .any(|reason| reason.contains("cwd")),
            "expected deny reason to mention cwd, got {:?}",
            decision.deny_reasons
        );
    }

    #[test]
    fn policy_termbridge_denies_without_consent_when_required() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "spawn".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("iterm2".to_string()),
            command: Some("bash".to_string()),
            backend: None,
            channel: None,
            bytes: None,
            cwd: None,
            requires_opt_in: Some(true),
            consent_granted: Some(false),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(
            !decision.is_allowed(),
            "spawn must be denied when consent is required and not granted"
        );
    }

    #[test]
    fn policy_termbridge_allows_with_consent_when_required() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TermBridgePolicyInput {
            action: "send_text".to_string(),
            persona: Some("core".to_string()),
            terminal: Some("iterm2".to_string()),
            command: None,
            backend: None,
            channel: None,
            bytes: Some(5),
            cwd: None,
            requires_opt_in: Some(true),
            consent_granted: Some(true),
        };

        let decision = engine.evaluate_termbridge(&input).unwrap();
        assert!(
            decision.is_allowed(),
            "send_text should be allowed with consent"
        );
    }

    #[test]
    fn policy_allows_guard_with_approval() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = AckPolicyInput::new("agent.guard".to_string(), Some("core".to_string()), None)
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
    fn policy_tls_allows_strict_cipher() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TlsPolicyInput {
            listener: "127.0.0.1:17718".to_string(),
            cipher_policy: "strict".to_string(),
            tls_versions: vec!["TLS1.3".to_string()],
            client_auth_required: false,
            certificate_fingerprint_sha256: Some("cafef00d".to_string()),
            ca_fingerprint_sha256: None,
        };

        let decision = engine.evaluate_tls(&input).unwrap();
        assert!(decision.is_allowed(), "Strict TLS should be allowed");
    }

    #[test]
    fn policy_tls_denies_legacy_without_mtls() {
        let policy_file = create_test_policy();
        let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

        let input = TlsPolicyInput {
            listener: "127.0.0.1:17718".to_string(),
            cipher_policy: "legacy".to_string(),
            tls_versions: vec!["TLS1.2".to_string()],
            client_auth_required: false,
            certificate_fingerprint_sha256: Some("deadbeef".to_string()),
            ca_fingerprint_sha256: None,
        };

        let decision = engine.evaluate_tls(&input).unwrap();
        assert!(
            !decision.is_allowed(),
            "Legacy TLS without mTLS should be denied"
        );
        assert!(decision
            .deny_reasons
            .iter()
            .any(|reason| reason.contains("legacy")));
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

        let input =
            AckPolicyInput::new("agent.unknown".to_string(), Some("test".to_string()), None);

        let decision = engine.evaluate_ack(&input).unwrap();
        assert!(
            decision.is_allowed(),
            "Disabled engine should allow everything"
        );
    }

    #[test]
    fn policy_tls_allows_when_disabled() {
        let engine = PolicyEngine::new(None).unwrap();

        let input = TlsPolicyInput {
            listener: "127.0.0.1:17718".to_string(),
            cipher_policy: "legacy".to_string(),
            tls_versions: vec!["TLS1.2".to_string()],
            client_auth_required: false,
            certificate_fingerprint_sha256: None,
            ca_fingerprint_sha256: None,
        };

        let decision = engine.evaluate_tls(&input).unwrap();
        assert!(decision.is_allowed(), "Disabled engine should allow TLS");
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
        file.as_file_mut()
            .seek(std::io::SeekFrom::Start(0))
            .unwrap();
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
