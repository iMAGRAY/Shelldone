use criterion::{black_box, criterion_group, criterion_main, Criterion};
use shelldone_agentd::policy_engine::{AckPolicyInput, PolicyEngine};
use std::io::Write;
use tempfile::NamedTempFile;

fn create_test_policy() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
package shelldone.policy
import rego.v1

default allow := false

# Allow specific personas
allow if {{
    input.persona in {{"core", "nova", "flux"}}
    input.command in {{"agent.exec", "agent.plan", "agent.undo"}}
}}

# Deny dangerous commands
deny_reasons contains "dangerous_command" if {{
    input.command == "agent.destroy"
}}
"#
    )
    .unwrap();
    file.flush().unwrap();
    file
}

fn policy_eval_cold_cache(c: &mut Criterion) {
    let policy_file = create_test_policy();
    let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

    c.bench_function("policy_eval_cold_cache", |b| {
        b.iter(|| {
            // Create new policy engine for each iteration (no cache)
            let fresh_engine = PolicyEngine::new(Some(policy_file.path())).unwrap();
            let input = AckPolicyInput::new(
                black_box("agent.exec".to_string()),
                black_box(Some("core".to_string())),
                None,
            );
            fresh_engine.evaluate_ack(&input).unwrap()
        })
    });
}

fn policy_eval_hot_cache(c: &mut Criterion) {
    let policy_file = create_test_policy();
    let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

    // Warm up cache
    let input = AckPolicyInput::new(
        "agent.exec".to_string(),
        Some("core".to_string()),
        None,
    );
    engine.evaluate_ack(&input).unwrap();

    c.bench_function("policy_eval_hot_cache", |b| {
        b.iter(|| {
            let input = AckPolicyInput::new(
                black_box("agent.exec".to_string()),
                black_box(Some("core".to_string())),
                None,
            );
            engine.evaluate_ack(&input).unwrap()
        })
    });
}

fn policy_eval_mixed_keys(c: &mut Criterion) {
    let policy_file = create_test_policy();
    let engine = PolicyEngine::new(Some(policy_file.path())).unwrap();

    let personas = vec!["core", "nova", "flux"];
    let commands = vec!["agent.exec", "agent.plan", "agent.undo"];

    c.bench_function("policy_eval_mixed_keys", |b| {
        let mut i = 0;
        b.iter(|| {
            let persona = personas[i % personas.len()];
            let command = commands[i % commands.len()];
            i += 1;

            let input = AckPolicyInput::new(
                black_box(command.to_string()),
                black_box(Some(persona.to_string())),
                None,
            );
            engine.evaluate_ack(&input).unwrap()
        })
    });
}

criterion_group!(
    benches,
    policy_eval_cold_cache,
    policy_eval_hot_cache,
    policy_eval_mixed_keys
);
criterion_main!(benches);
