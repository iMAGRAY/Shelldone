use anyhow::{Context, Result};
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::Resource;
use std::time::Duration;
use tracing::info;

/// Prism telemetry metrics
pub struct PrismMetrics {
    // ACK command latency
    pub exec_latency: Histogram<f64>,
    pub undo_latency: Histogram<f64>,

    // Policy enforcement
    pub policy_denials: Counter<u64>,
    pub policy_evaluations: Counter<u64>,

    // Sigma proxy violations
    pub sigma_guard_events: Counter<u64>,

    // Persona hints (Wave 2)
    #[allow(dead_code)]
    pub persona_hints: Counter<u64>,

    // Continuum operations (Wave 2)
    #[allow(dead_code)]
    pub snapshot_created: Counter<u64>,
    pub events_restored: Counter<u64>,

    // TLS hot reload observability
    #[allow(dead_code)]
    pub tls_reloads: Counter<u64>,
    #[allow(dead_code)]
    pub tls_reload_errors: Counter<u64>,
    #[allow(dead_code)]
    pub tls_reload_latency: Histogram<f64>,
    #[allow(dead_code)]
    pub termbridge_actions: Counter<u64>,
    #[allow(dead_code)]
    pub termbridge_errors: Counter<u64>,
    #[allow(dead_code)]
    pub termbridge_latency: Histogram<f64>,
    #[allow(dead_code)]
    pub termbridge_clipboard_bytes: Counter<u64>,
}

impl PrismMetrics {
    /// Initialize Prism metrics
    pub fn new(meter: &Meter) -> Self {
        let exec_latency = meter
            .f64_histogram("shelldone.ack.exec.latency_ms")
            .with_description("ACK agent.exec latency in milliseconds")
            .build();

        let undo_latency = meter
            .f64_histogram("shelldone.ack.undo.latency_ms")
            .with_description("ACK agent.undo latency in milliseconds")
            .build();

        let policy_denials = meter
            .u64_counter("shelldone.policy.denials")
            .with_description("Total policy denials")
            .build();

        let policy_evaluations = meter
            .u64_counter("shelldone.policy.evaluations")
            .with_description("Total policy evaluations")
            .build();

        let sigma_guard_events = meter
            .u64_counter("shelldone.sigma.guard.events")
            .with_description("Σ-pty guard violations reported")
            .build();

        let persona_hints = meter
            .u64_counter("shelldone.persona.hints")
            .with_description("Persona hints shown")
            .build();

        let snapshot_created = meter
            .u64_counter("shelldone.continuum.snapshots_created")
            .with_description("Continuum snapshots created")
            .build();

        let events_restored = meter
            .u64_counter("shelldone.continuum.events_restored")
            .with_description("Events restored from snapshots")
            .build();

        let tls_reloads = meter
            .u64_counter("agent.tls.reloads")
            .with_description("Number of TLS reloads applied")
            .build();

        let tls_reload_errors = meter
            .u64_counter("agent.tls.reload_errors")
            .with_description("Number of TLS reload failures")
            .build();

        let tls_reload_latency = meter
            .f64_histogram("agent.tls.reload_latency_ms")
            .with_description("Latency of TLS reload pipeline in milliseconds")
            .build();

        let termbridge_actions = meter
            .u64_counter("termbridge.actions")
            .with_description("TermBridge actions executed")
            .build();

        let termbridge_errors = meter
            .u64_counter("termbridge.errors")
            .with_description("TermBridge errors")
            .build();

        let termbridge_latency = meter
            .f64_histogram("termbridge.latency_ms")
            .with_description("Latency of TermBridge operations in milliseconds")
            .build();

        let termbridge_clipboard_bytes = meter
            .u64_counter("termbridge.clipboard.bytes")
            .with_description("Bytes transferred through clipboard bridge")
            .build();

        Self {
            exec_latency,
            undo_latency,
            policy_denials,
            policy_evaluations,
            sigma_guard_events,
            persona_hints,
            snapshot_created,
            events_restored,
            tls_reloads,
            tls_reload_errors,
            tls_reload_latency,
            termbridge_actions,
            termbridge_errors,
            termbridge_latency,
            termbridge_clipboard_bytes,
        }
    }

    /// Record exec latency
    pub fn record_exec_latency(&self, duration_ms: f64, persona: Option<&str>) {
        let attrs = if let Some(p) = persona {
            vec![KeyValue::new("persona", p.to_string())]
        } else {
            vec![]
        };
        self.exec_latency.record(duration_ms, &attrs);
    }

    /// Record undo latency
    pub fn record_undo_latency(&self, duration_ms: f64, snapshot_id: &str) {
        self.undo_latency.record(
            duration_ms,
            &[KeyValue::new("snapshot_id", snapshot_id.to_string())],
        );
    }

    /// Record policy denial
    pub fn record_policy_denial(&self, command: &str, persona: Option<&str>) {
        let mut attrs = vec![KeyValue::new("command", command.to_string())];
        if let Some(p) = persona {
            attrs.push(KeyValue::new("persona", p.to_string()));
        }
        self.policy_denials.add(1, &attrs);
    }

    /// Record policy evaluation
    pub fn record_policy_evaluation(&self, allowed: bool) {
        self.policy_evaluations
            .add(1, &[KeyValue::new("allowed", allowed.to_string())]);
    }

    /// Record σ-pty guard violation event
    pub fn record_sigma_guard_event(&self, direction: &str, reason: &str) {
        self.sigma_guard_events.add(
            1,
            &[
                KeyValue::new("direction", direction.to_string()),
                KeyValue::new("reason", reason.to_string()),
            ],
        );
    }

    /// Record persona hint (Wave 2: Persona Engine)
    #[allow(dead_code)]
    pub fn record_persona_hint(&self, persona: &str, hint_type: &str) {
        self.persona_hints.add(
            1,
            &[
                KeyValue::new("persona", persona.to_string()),
                KeyValue::new("hint_type", hint_type.to_string()),
            ],
        );
    }

    /// Record snapshot creation (Wave 2: Continuum API)
    #[allow(dead_code)]
    pub fn record_snapshot_created(&self, _event_count: u64) {
        self.snapshot_created.add(1, &[]);
        // Could add event_count as attribute if needed
    }

    /// Record events restored
    pub fn record_events_restored(&self, count: u64) {
        self.events_restored.add(count, &[]);
    }

    /// Record TLS reload success
    #[allow(dead_code)]
    pub fn record_tls_reload_success(
        &self,
        policy: &str,
        fingerprint: Option<&str>,
        latency_ms: f64,
    ) {
        let mut attrs = vec![KeyValue::new("policy", policy.to_string())];
        if let Some(fp) = fingerprint {
            attrs.push(KeyValue::new("fingerprint", fp.to_string()));
        }
        self.tls_reloads.add(1, &attrs);
        self.tls_reload_latency.record(latency_ms, &attrs);
    }

    /// Record TLS reload failure
    #[allow(dead_code)]
    pub fn record_tls_reload_error(&self, policy: &str, reason: &str) {
        self.tls_reload_errors.add(
            1,
            &[
                KeyValue::new("policy", policy.to_string()),
                KeyValue::new("reason", reason.to_string()),
            ],
        );
    }

    #[allow(dead_code)]
    pub fn record_termbridge_action(
        &self,
        action: &str,
        terminal: &str,
        latency_ms: f64,
        outcome: &str,
    ) {
        let attrs = [
            KeyValue::new("action", action.to_string()),
            KeyValue::new("terminal", terminal.to_string()),
            KeyValue::new("outcome", outcome.to_string()),
        ];
        self.termbridge_actions.add(1, &attrs);
        self.termbridge_latency.record(latency_ms, &attrs);
    }

    #[allow(dead_code)]
    pub fn record_termbridge_error(&self, action: &str, terminal: &str, reason: &str) {
        self.termbridge_errors.add(
            1,
            &[
                KeyValue::new("action", action.to_string()),
                KeyValue::new("terminal", terminal.to_string()),
                KeyValue::new("reason", reason.to_string()),
            ],
        );
    }

    #[allow(dead_code)]
    pub fn record_termbridge_clipboard(
        &self,
        action: &str,
        backend: &str,
        bytes: u64,
        latency_ms: f64,
        outcome: &str,
    ) {
        let attrs = [
            KeyValue::new("action", action.to_string()),
            KeyValue::new("backend", backend.to_string()),
            KeyValue::new("outcome", outcome.to_string()),
        ];
        self.termbridge_actions.add(1, &attrs);
        self.termbridge_latency.record(latency_ms, &attrs);
        self.termbridge_clipboard_bytes.add(bytes, &attrs);
    }
}

/// Initialize Prism OTLP telemetry
pub fn init_prism(
    endpoint: Option<String>,
    service_name: &str,
) -> Result<(SdkMeterProvider, PrismMetrics)> {
    let endpoint = endpoint.unwrap_or_else(|| "http://localhost:4318".to_string());

    info!("Initializing Prism OTLP telemetry: endpoint={}", endpoint);

    let export_config = opentelemetry_otlp::ExportConfig {
        endpoint: Some(endpoint.clone()),
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_export_config(export_config)
        .build()
        .context("building OTLP metric exporter")?;

    let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(
        exporter,
        opentelemetry_sdk::runtime::Tokio,
    )
    .with_interval(Duration::from_secs(30))
    .build();

    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.to_string()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    global::set_meter_provider(provider.clone());

    let meter = global::meter("shelldone-agentd");
    let metrics = PrismMetrics::new(&meter);

    info!("Prism OTLP telemetry initialized successfully");

    Ok((provider, metrics))
}

/// Graceful shutdown of telemetry
pub fn shutdown_prism(provider: SdkMeterProvider) -> Result<()> {
    info!("Shutting down Prism telemetry");
    provider
        .shutdown()
        .context("shutting down meter provider")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::metrics::MeterProvider;

    #[test]
    fn test_metrics_initialization() {
        let provider = SdkMeterProvider::builder().build();
        let meter = provider.meter("test");
        let metrics = PrismMetrics::new(&meter);

        // Record some test metrics
        metrics.record_exec_latency(100.0, Some("core"));
        metrics.record_policy_denial("agent.exec", Some("nova"));
        metrics.record_policy_evaluation(true);
        metrics.record_snapshot_created(100);
        metrics.record_events_restored(50);
    }
}
