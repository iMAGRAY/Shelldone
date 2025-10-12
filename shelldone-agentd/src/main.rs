use clap::Parser;
use shelldone_agentd::{export_termbridge_snapshot, run, CipherPolicy, Settings};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
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

    #[arg(
        long,
        value_name = "PATH",
        help = "Export TermBridge capability snapshot to JSON and exit"
    )]
    termbridge_export: Option<PathBuf>,

    #[arg(
        long,
        default_value_t = 2000,
        help = "Timeout in milliseconds for TermBridge discovery export"
    )]
    termbridge_export_timeout_ms: u64,

    #[arg(
        long,
        help = "Emit OTLP telemetry while exporting TermBridge snapshot",
        requires = "termbridge_export"
    )]
    termbridge_export_emit_otlp: bool,
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

    if let Some(output_path) = cli.termbridge_export {
        let timeout = Duration::from_millis(cli.termbridge_export_timeout_ms);
        let snapshot = export_termbridge_snapshot(
            settings,
            output_path.clone(),
            timeout,
            cli.termbridge_export_emit_otlp,
        )
        .await?;
        println!(
            "TermBridge snapshot written to {} (terminals: {}, discovery: {:.1} ms)",
            output_path.display(),
            snapshot.totals.terminals,
            snapshot.discovery_ms
        );
        return Ok(());
    }

    run(settings).await
}

fn parse_cipher_policy(value: &str) -> Result<CipherPolicy, String> {
    value.parse()
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn cli_defaults_termbridge_flags() {
        let cli = Cli::try_parse_from(["shelldone-agentd"]).expect("default parse");
        assert!(cli.termbridge_export.is_none());
        assert_eq!(cli.termbridge_export_timeout_ms, 2000);
        assert!(!cli.termbridge_export_emit_otlp);
    }

    #[test]
    fn cli_parses_termbridge_export_flags() {
        let cli = Cli::try_parse_from([
            "shelldone-agentd",
            "--termbridge-export",
            "snapshot.json",
            "--termbridge-export-timeout-ms",
            "4500",
            "--termbridge-export-emit-otlp",
        ])
        .expect("termbridge export flags parse");
        assert_eq!(cli.termbridge_export, Some(PathBuf::from("snapshot.json")));
        assert_eq!(cli.termbridge_export_timeout_ms, 4500);
        assert!(cli.termbridge_export_emit_otlp);
    }

    #[test]
    fn cli_rejects_invalid_timeout_value() {
        let result = Cli::try_parse_from([
            "shelldone-agentd",
            "--termbridge-export",
            "snapshot.json",
            "--termbridge-export-timeout-ms",
            "not-a-number",
        ]);
        assert!(result.is_err());
    }
}
