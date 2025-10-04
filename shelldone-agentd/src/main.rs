use clap::Parser;
use shelldone_agentd::{run, Settings};
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
        state_dir: cli.state_dir,
        policy_path,
        otlp_endpoint: cli.otlp_endpoint,
    };

    run(settings).await
}
