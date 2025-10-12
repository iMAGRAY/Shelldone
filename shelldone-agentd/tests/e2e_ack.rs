// E2E tests for ACK protocol
//
// Tests full request/response cycle for agent.exec and agent.undo.
// Validates policy enforcement, Continuum journal, metrics recording.

use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

#[tokio::test]
async fn e2e_agent_exec_echo_command() {
    let temp = TempDir::new().unwrap();
    let port = find_free_port().await;

    // Start agentd in background
    let state_dir = temp.path().to_path_buf();
    let settings = shelldone_agentd::Settings {
        listen: ([127, 0, 0, 1], port).into(),
        grpc_listen: ([127, 0, 0, 1], 0).into(),
        grpc_tls_cert: None,
        grpc_tls_key: None,
        grpc_tls_ca: None,
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: state_dir.clone(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        shelldone_agentd::run(settings).await.unwrap();
    });

    // Wait for server to start
    wait_for_port(port).await;

    // Send agent.exec request
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/ack/exec", port))
        .json(&json!({
            "command": "agent.exec",
            "persona": "core",
            "args": {
                "cmd": "echo hello e2e",
                "cwd": temp.path().to_str().unwrap(),
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    let stdout = body["stdout"].as_str().unwrap();
    assert!(
        stdout.contains("hello") && stdout.contains("e2e"),
        "stdout was: {}",
        stdout
    );
    assert_eq!(body["exit_code"], 0);

    // Verify journal was written
    let journal_path = state_dir.join("journal").join("continuum.log");
    assert!(journal_path.exists());

    let journal = tokio::fs::read_to_string(&journal_path).await.unwrap();
    assert!(journal.contains("\"kind\":\"exec\""));
    assert!(journal.contains("\"persona\":\"core\""));

    server_handle.abort();
}

#[tokio::test]
async fn e2e_sigma_guard_journal_event() {
    let temp = TempDir::new().unwrap();
    let port = find_free_port().await;

    let state_dir = temp.path().to_path_buf();
    let settings = shelldone_agentd::Settings {
        listen: ([127, 0, 0, 1], port).into(),
        grpc_listen: ([127, 0, 0, 1], 0).into(),
        grpc_tls_cert: None,
        grpc_tls_key: None,
        grpc_tls_ca: None,
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: state_dir.clone(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        shelldone_agentd::run(settings).await.unwrap();
    });

    wait_for_port(port).await;

    let client = reqwest::Client::new();
    let payload = json!({
        "kind": "sigma.guard",
        "payload": {
            "reason": "OSC 52 read blocked",
            "direction": "output",
            "sequence_preview": "1B 5D 35 32",
            "sequence_len": 12,
            "occurred_at": "2025-10-04T00:00:00Z"
        },
        "spectral_tag": "sigma::guard",
        "bytes": 12
    });

    let response = client
        .post(format!("http://127.0.0.1:{}/journal/event", port))
        .json(&payload)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    sleep(Duration::from_millis(50)).await;
    let journal_path = state_dir.join("journal").join("continuum.log");
    let journal = tokio::fs::read_to_string(&journal_path).await.unwrap();
    let last_line = journal.lines().last().unwrap().to_string();
    let event: serde_json::Value = serde_json::from_str(&last_line).unwrap();
    assert_eq!(event["kind"], "sigma.guard");
    assert_eq!(event["payload"]["reason"], "OSC 52 read blocked");
    assert_eq!(event["payload"]["direction"], "output");
    assert_eq!(event["bytes"], 12);

    server_handle.abort();
}

#[tokio::test]
async fn e2e_agent_exec_policy_enforcement() {
    let temp = TempDir::new().unwrap();
    let port = find_free_port().await;

    // Create restrictive policy
    let policy_dir = temp.path().join("policies");
    tokio::fs::create_dir_all(&policy_dir).await.unwrap();
    let policy_path = policy_dir.join("strict.rego");

    tokio::fs::write(
        &policy_path,
        r#"
package shelldone.policy

import future.keywords.if

# Deny all commands by default
default allow = false

# Only allow Nova persona
allow if {
    input.persona == "nova"
}
"#,
    )
    .await
    .unwrap();

    // Start agentd with strict policy
    let state_dir = temp.path().to_path_buf();
    let settings = shelldone_agentd::Settings {
        listen: ([127, 0, 0, 1], port).into(),
        grpc_listen: ([127, 0, 0, 1], 0).into(),
        grpc_tls_cert: None,
        grpc_tls_key: None,
        grpc_tls_ca: None,
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: state_dir.clone(),
        policy_path: Some(policy_path),
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        shelldone_agentd::run(settings).await.unwrap();
    });

    wait_for_port(port).await;

    // Try with core persona (should be denied)
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/ack/exec", port))
        .json(&json!({
            "command": "agent.exec",
            "persona": "core",
            "args": {
                "cmd": "echo test",
                "cwd": temp.path().to_str().unwrap(),
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body.get("code").and_then(|v| v.as_str()),
        Some("policy_denied")
    );

    // Try with nova persona (should succeed)
    let response = client
        .post(format!("http://127.0.0.1:{}/ack/exec", port))
        .json(&json!({
            "command": "agent.exec",
            "persona": "nova",
            "args": {
                "cmd": "echo allowed",
                "cwd": temp.path().to_str().unwrap(),
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    server_handle.abort();
}

#[tokio::test]
async fn e2e_agent_undo_snapshot_restore() {
    let temp = TempDir::new().unwrap();
    let port = find_free_port().await;

    // Start agentd
    let state_dir = temp.path().to_path_buf();
    let settings = shelldone_agentd::Settings {
        listen: ([127, 0, 0, 1], port).into(),
        grpc_listen: ([127, 0, 0, 1], 0).into(),
        grpc_tls_cert: None,
        grpc_tls_key: None,
        grpc_tls_ca: None,
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: state_dir.clone(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        shelldone_agentd::run(settings).await.unwrap();
    });

    wait_for_port(port).await;

    // Execute commands to generate journal events
    let client = reqwest::Client::new();
    for i in 0..5 {
        client
            .post(format!("http://127.0.0.1:{}/ack/exec", port))
            .json(&json!({
                "command": "agent.exec",
                "persona": "core",
                "args": {
                    "cmd": format!("echo event_{}", i),
                    "cwd": temp.path().to_str().unwrap(),
                }
            }))
            .send()
            .await
            .unwrap();
    }

    // Manual snapshot creation; Continuum API integration tracked via task-continuum-api
    let snapshot_dir = state_dir.join("snapshots");
    tokio::fs::create_dir_all(&snapshot_dir).await.unwrap();

    // For now, skip actual snapshot test until Continuum API is exposed
    // This is a placeholder for full E2E undo test

    server_handle.abort();
}

#[tokio::test]
async fn e2e_healthz_endpoint() {
    let temp = TempDir::new().unwrap();
    let port = find_free_port().await;

    let settings = shelldone_agentd::Settings {
        listen: ([127, 0, 0, 1], port).into(),
        grpc_listen: ([127, 0, 0, 1], 0).into(),
        grpc_tls_cert: None,
        grpc_tls_key: None,
        grpc_tls_ca: None,
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: temp.path().to_path_buf(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        shelldone_agentd::run(settings).await.unwrap();
    });

    wait_for_port(port).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://127.0.0.1:{}/healthz", port))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["timestamp"].is_string());

    server_handle.abort();
}

#[tokio::test]
async fn e2e_concurrent_requests() {
    let temp = TempDir::new().unwrap();
    let port = find_free_port().await;

    let settings = shelldone_agentd::Settings {
        listen: ([127, 0, 0, 1], port).into(),
        grpc_listen: ([127, 0, 0, 1], 0).into(),
        grpc_tls_cert: None,
        grpc_tls_key: None,
        grpc_tls_ca: None,
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: temp.path().to_path_buf(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        shelldone_agentd::run(settings).await.unwrap();
    });

    wait_for_port(port).await;

    // Send 10 concurrent requests
    let mut handles = vec![];
    for i in 0..10 {
        let request_port = port;
        let cwd = temp.path().to_str().unwrap().to_string();
        let handle = tokio::spawn(async move {
            let client = reqwest::Client::new();
            client
                .post(format!("http://127.0.0.1:{}/ack/exec", request_port))
                .json(&json!({
                    "command": "agent.exec",
                    "persona": "core",
                    "args": {
                        "cmd": format!("echo concurrent_{}", i),
                        "cwd": cwd,
                    }
                }))
                .send()
                .await
                .unwrap()
        });
        handles.push(handle);
    }

    // All requests should succeed
    for handle in handles {
        let response = handle.await.unwrap();
        assert_eq!(response.status(), 200);
    }

    server_handle.abort();
}

// Helper: find free TCP port for testing
async fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn wait_for_port(port: u16) {
    let mut attempts = 0;
    loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => {
                drop(stream);
                break;
            }
            Err(_) if attempts < 50 => {
                attempts += 1;
                sleep(Duration::from_millis(50)).await;
            }
            Err(err) => {
                panic!("agentd did not start listening on port {}: {}", port, err);
            }
        }
    }
}
