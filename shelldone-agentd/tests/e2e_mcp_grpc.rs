use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType, IsCa};
use serde_json::json;
use shelldone_agentd::{run, Settings};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};
use tonic::transport::{
    Certificate as ClientCertificate, Channel, ClientTlsConfig, Endpoint,
    Identity as ClientIdentity,
};
use tonic::Code;

async fn atomic_replace(path: &std::path::Path, contents: &[u8]) {
    let tmp_path = path.with_extension("tmp");
    tokio::fs::write(&tmp_path, contents).await.unwrap();
    tokio::fs::rename(&tmp_path, path).await.unwrap();
}

mod mcp_proto {
    tonic::include_proto!("shelldone.mcp");
}

use mcp_proto::mcp_bridge_client::McpBridgeClient;
use mcp_proto::{CallToolRequest, InitializeRequest, ListToolsRequest};

#[tokio::test]
async fn grpc_mutual_tls_allows_authorized_client() {
    let http_port = find_free_port().await;
    let grpc_port = find_free_port().await;
    let temp = TempDir::new().unwrap();

    let (ca_pem, server_cert_pem, server_key_pem, client_cert_pem, client_key_pem) =
        generate_certificates();
    let ca_path = temp.path().join("ca.pem");
    tokio::fs::write(&ca_path, &ca_pem).await.unwrap();
    let server_cert_path = temp.path().join("server-cert.pem");
    let server_key_path = temp.path().join("server-key.pem");
    tokio::fs::write(&server_cert_path, &server_cert_pem)
        .await
        .unwrap();
    tokio::fs::write(&server_key_path, &server_key_pem)
        .await
        .unwrap();

    let state_dir = temp.path().join("state");

    let settings = Settings {
        listen: ([127, 0, 0, 1], http_port).into(),
        grpc_listen: ([127, 0, 0, 1], grpc_port).into(),
        grpc_tls_cert: Some(server_cert_path.clone()),
        grpc_tls_key: Some(server_key_path.clone()),
        grpc_tls_ca: Some(ca_path.clone()),
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: state_dir.clone(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        run(settings).await.unwrap();
    });

    sleep(Duration::from_millis(500)).await;

    let client = build_client(grpc_port, &ca_pem, &client_cert_pem, &client_key_pem).await;
    exercise_bridge(client).await;

    server_handle.abort();
}

#[tokio::test]
async fn grpc_mutual_tls_blocks_client_without_certificate() {
    let http_port = find_free_port().await;
    let grpc_port = find_free_port().await;
    let temp = TempDir::new().unwrap();

    let (ca_pem, server_cert_pem, server_key_pem, _client_cert_pem, _client_key_pem) =
        generate_certificates();
    let ca_path = temp.path().join("ca.pem");
    tokio::fs::write(&ca_path, &ca_pem).await.unwrap();
    let server_cert_path = temp.path().join("server-cert.pem");
    let server_key_path = temp.path().join("server-key.pem");
    tokio::fs::write(&server_cert_path, &server_cert_pem)
        .await
        .unwrap();
    tokio::fs::write(&server_key_path, &server_key_pem)
        .await
        .unwrap();

    let state_dir = temp.path().join("state");

    let settings = Settings {
        listen: ([127, 0, 0, 1], http_port).into(),
        grpc_listen: ([127, 0, 0, 1], grpc_port).into(),
        grpc_tls_cert: Some(server_cert_path.clone()),
        grpc_tls_key: Some(server_key_path.clone()),
        grpc_tls_ca: Some(ca_path.clone()),
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: state_dir.clone(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        run(settings).await.unwrap();
    });

    sleep(Duration::from_millis(200)).await;

    let endpoint = Endpoint::from_shared(format!("https://localhost:{grpc_port}")).unwrap();
    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ClientCertificate::from_pem(ca_pem.clone().into_bytes()));
    let channel = endpoint.tls_config(tls).unwrap().connect().await.unwrap();
    let mut client = McpBridgeClient::new(channel);
    let err = client
        .initialize(tonic::Request::new(InitializeRequest {
            persona: "core".into(),
            protocol_version: "1.0".into(),
            capabilities: vec![],
        }))
        .await
        .unwrap_err();
    assert!(
        matches!(err.code(), Code::Unknown | Code::Unauthenticated),
        "expected TLS/mTLS failure, got {:?}",
        err
    );

    server_handle.abort();
}

#[tokio::test]
async fn tls_hot_reload_accepts_rotated_certificate() {
    let http_port = find_free_port().await;
    let grpc_port = find_free_port().await;
    let temp = TempDir::new().unwrap();

    let (ca1_pem, server1_cert_pem, server1_key_pem, client1_cert_pem, client1_key_pem) =
        generate_certificates();
    let (ca2_pem, server2_cert_pem, server2_key_pem, client2_cert_pem, client2_key_pem) =
        generate_certificates();

    let ca_path = temp.path().join("ca.pem");
    let server_cert_path = temp.path().join("server-cert.pem");
    let server_key_path = temp.path().join("server-key.pem");

    tokio::fs::write(&ca_path, &ca1_pem).await.unwrap();
    tokio::fs::write(&server_cert_path, &server1_cert_pem)
        .await
        .unwrap();
    tokio::fs::write(&server_key_path, &server1_key_pem)
        .await
        .unwrap();

    let state_dir = temp.path().join("state");

    let settings = Settings {
        listen: ([127, 0, 0, 1], http_port).into(),
        grpc_listen: ([127, 0, 0, 1], grpc_port).into(),
        grpc_tls_cert: Some(server_cert_path.clone()),
        grpc_tls_key: Some(server_key_path.clone()),
        grpc_tls_ca: Some(ca_path.clone()),
        grpc_tls_policy: shelldone_agentd::CipherPolicy::Balanced,
        state_dir: state_dir.clone(),
        policy_path: None,
        otlp_endpoint: None,
    };

    let server_handle = tokio::spawn(async move {
        run(settings).await.unwrap();
    });

    sleep(Duration::from_millis(500)).await;

    // Initial connection works with CA #1
    let mut client = build_client(grpc_port, &ca1_pem, &client1_cert_pem, &client1_key_pem).await;
    client
        .initialize(tonic::Request::new(InitializeRequest {
            persona: "core".into(),
            protocol_version: "1.0".into(),
            capabilities: vec![],
        }))
        .await
        .unwrap();

    // Rotate PEM files to second set
    atomic_replace(&ca_path, ca2_pem.as_bytes()).await;
    atomic_replace(&server_cert_path, server2_cert_pem.as_bytes()).await;
    atomic_replace(&server_key_path, server2_key_pem.as_bytes()).await;

    // Wait for hot reload (notify debounce + filesystem latency under heavy IO)
    sleep(Duration::from_secs(8)).await;

    // Old CA should now fail
    let result_old =
        try_build_client(grpc_port, &ca1_pem, &client1_cert_pem, &client1_key_pem).await;
    assert!(
        result_old.is_err(),
        "old CA should be rejected after rotation"
    );

    // Retry new CA until success (allowing watcher latency)
    let mut attempts = 0;
    let mut connected = None;
    while attempts < 40 {
        match try_build_client(grpc_port, &ca2_pem, &client2_cert_pem, &client2_key_pem).await {
            Ok(mut client) => {
                client
                    .initialize(tonic::Request::new(InitializeRequest {
                        persona: "nova".into(),
                        protocol_version: "1.0".into(),
                        capabilities: vec!["fs".into()],
                    }))
                    .await
                    .unwrap();
                connected = Some(client);
                break;
            }
            Err(_) => {
                attempts += 1;
                sleep(Duration::from_millis(200)).await;
            }
        }
    }

    assert!(
        connected.is_some(),
        "expected TLS reload to accept rotated certificate"
    );

    server_handle.abort();
}

fn generate_certificates() -> (String, String, String, String, String) {
    let mut ca_params = CertificateParams::default();
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Shelldone Root CA");
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca_cert = Certificate::from_params(ca_params).unwrap();

    let (server_cert_pem, server_key_pem) = generate_signed_cert(&ca_cert, "localhost");
    let (client_cert_pem, client_key_pem) = generate_signed_cert(&ca_cert, "mcp-client");

    (
        ca_cert.serialize_pem().unwrap(),
        server_cert_pem,
        server_key_pem,
        client_cert_pem,
        client_key_pem,
    )
}

fn generate_signed_cert(ca: &Certificate, common_name: &str) -> (String, String) {
    let mut params = CertificateParams::new(vec![common_name.to_string()]);
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, common_name);
    let cert = Certificate::from_params(params).unwrap();
    let cert_pem = cert.serialize_pem_with_signer(ca).unwrap();
    let key_pem = cert.serialize_private_key_pem();
    (cert_pem, key_pem)
}

async fn build_client(
    port: u16,
    ca_pem: &str,
    client_cert_pem: &str,
    client_key_pem: &str,
) -> McpBridgeClient<Channel> {
    try_build_client(port, ca_pem, client_cert_pem, client_key_pem)
        .await
        .unwrap()
}

async fn exercise_bridge(mut client: McpBridgeClient<Channel>) {
    let response = client
        .initialize(tonic::Request::new(InitializeRequest {
            persona: "core".into(),
            protocol_version: "1.0".into(),
            capabilities: vec!["fs".into()],
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!response.session_id.is_empty());

    let tools = client
        .list_tools(tonic::Request::new(ListToolsRequest {
            session_id: response.session_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!tools.tools.is_empty());

    let exec = client
        .call_tool(tonic::Request::new(CallToolRequest {
            session_id: response.session_id,
            tool_name: "agent.exec".into(),
            arguments_json: json!({
                "cmd": "echo grpc mTLS"
            })
            .to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(exec.exit_code, 0);
    assert!(exec.stdout.contains("grpc"));
}

async fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn try_build_client(
    port: u16,
    ca_pem: &str,
    client_cert_pem: &str,
    client_key_pem: &str,
) -> Result<McpBridgeClient<Channel>, tonic::transport::Error> {
    let endpoint = Endpoint::from_shared(format!("https://localhost:{port}"))?;
    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ClientCertificate::from_pem(ca_pem.as_bytes().to_vec()))
        .identity(ClientIdentity::from_pem(
            client_cert_pem.as_bytes().to_vec(),
            client_key_pem.as_bytes().to_vec(),
        ));

    let channel = endpoint.tls_config(tls)?.connect().await?;
    Ok(McpBridgeClient::new(channel))
}
