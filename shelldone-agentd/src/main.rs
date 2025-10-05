use clap::Parser;
use shelldone_agentd::{run, CipherPolicy, Settings};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(author, version, about = "Shelldone agent control plane daemon", long_about = None)]
struct Cli {
    #[arg(
        long,
        default_value = "127.0.0.1:17717",
        help = "Listen address for Σ-json endpoints"
    )]
    listen: SocketAddr,

    #[arg(
        long,
        default_value = "127.0.0.1:17718",
        help = "Listen address for MCP gRPC bridge"
    )]
    grpc_listen: SocketAddr,

    #[arg(
        long,
        value_name = "PATH",
        help = "PEM certificate file enabling TLS for the MCP gRPC bridge"
    )]
    grpc_tls_cert: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        help = "PEM private key file enabling TLS for the MCP gRPC bridge"
    )]
    grpc_tls_key: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        help = "PEM CA bundle enforcing mutual TLS for the MCP gRPC bridge"
    )]
    grpc_tls_ca: Option<PathBuf>,

    #[arg(
        long,
        default_value = "balanced",
        value_parser = parse_cipher_policy,
        help = "Cipher policy for gRPC TLS (strict|balanced|legacy)"
    )]
    grpc_tls_policy: CipherPolicy,

    #[arg(
        long,
        default_value = "state",
        help = "Directory for Continuum journal and artifacts"
    )]
    state_dir: PathBuf,

    #[arg(
        long,
        help = "Path to Rego policy file (defaults to policies/default.rego if exists)"
    )]
    policy: Option<PathBuf>,

    #[arg(
        long,
        help = "OTLP endpoint for Prism telemetry (e.g., http://localhost:4318)"
    )]
    otlp_endpoint: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let policy_path = cli.policy.or_else(|| {
        let default_path = PathBuf::from("policies/default.rego");
        if default_path.exists() {
            Some(default_path)
        } else {
            None
        }
    });

    let settings = Settings {
        listen: cli.listen,
        grpc_listen: cli.grpc_listen,
        grpc_tls_cert: cli.grpc_tls_cert,
        grpc_tls_key: cli.grpc_tls_key,
        grpc_tls_ca: cli.grpc_tls_ca,
        grpc_tls_policy: cli.grpc_tls_policy,
        state_dir: cli.state_dir,
        policy_path,
        otlp_endpoint: cli.otlp_endpoint,
    };

    run(settings).await
}

fn parse_cipher_policy(value: &str) -> Result<CipherPolicy, String> {
    value.parse()
}
