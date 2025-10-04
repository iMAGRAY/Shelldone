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

    let settings = Settings {
        listen: cli.listen,
        state_dir: cli.state_dir,
    };

    run(settings).await
}
