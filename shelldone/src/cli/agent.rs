use anyhow::{anyhow, Context, Result};
use clap::{ArgGroup, Parser, Subcommand};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::task;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:17717";
const DEFAULT_PERSONA: &str = "core";

#[derive(Debug, Parser, Clone)]
#[command(about = "Interact with the Shelldone agent control plane (UTIF-Σ)")]
pub struct AgentCommand {
    /// Base endpoint (protocol://host:port) of the running shelldone-agentd service.
    #[arg(long, default_value = DEFAULT_ENDPOINT)]
    endpoint: String,

    #[command(subcommand)]
    action: AgentAction,
}

#[derive(Debug, Subcommand, Clone)]
pub enum AgentAction {
    /// Negotiate capabilities via Σ-cap handshake.
    Handshake(HandshakeArgs),

    /// Execute a command through ACK `agent.exec`.
    Exec(ExecArgs),

    /// Append a custom event to Continuum journal via agentd.
    Journal(JournalArgs),
}

#[derive(Debug, Parser, Clone)]
pub struct HandshakeArgs {
    /// Persona to declare during handshake (nova|core|flux).
    #[arg(long, default_value = DEFAULT_PERSONA)]
    pub persona: String,

    /// Keyboard capabilities (comma-separated).
    #[arg(long, value_delimiter = ',', num_args = 1.., default_value = "kitty,legacy")]
    pub keyboard: Vec<String>,

    /// Graphics capabilities (comma-separated).
    #[arg(long, value_delimiter = ',', num_args = 1.., default_value = "kitty,minimal")]
    pub graphics: Vec<String>,

    /// Additional capability key=value pairs.
    #[arg(long = "cap", value_parser = parse_key_value, value_name = "KEY=JSON", num_args = 0..)]
    pub custom_caps: Vec<(String, Value)>,
}

#[derive(Debug, Parser, Clone)]
#[command(group(ArgGroup::new("command_src").required(true).args(["cmd", "command_file"])))]
pub struct ExecArgs {
    /// Persona that issues the command.
    #[arg(long, default_value = DEFAULT_PERSONA)]
    pub persona: String,

    /// Command to execute (shell expression).
    #[arg(long)]
    pub cmd: Option<String>,

    /// Path to file containing command (first line used).
    #[arg(long = "cmd-file")]
    pub command_file: Option<PathBuf>,

    /// Working directory.
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Shell executable override.
    #[arg(long)]
    pub shell: Option<String>,

    /// Spectral tag override.
    #[arg(long)]
    pub spectral_tag: Option<String>,

    /// Environment variables (KEY=VALUE).
    #[arg(long = "env", value_parser = parse_env, value_name = "KEY=VALUE", num_args = 0..)]
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Parser, Clone)]
pub struct JournalArgs {
    /// Kind of the event to record.
    #[arg(long, default_value = "cli.event")]
    pub kind: String,

    /// Persona issuing the event.
    #[arg(long, default_value = DEFAULT_PERSONA)]
    pub persona: String,

    /// Spectral tag annotation.
    #[arg(long)]
    pub spectral_tag: Option<String>,

    /// JSON payload for the event.
    #[arg(long, value_parser = parse_json, default_value = "{}")]
    pub payload: Value,

    /// Optional byte size associated with the event.
    #[arg(long)]
    pub bytes: Option<usize>,
}

pub async fn run(cmd: AgentCommand) -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("building reqwest client")?;

    match cmd.action {
        AgentAction::Handshake(args) => {
            let value = run_handshake(&client, &cmd.endpoint, args).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        AgentAction::Exec(args) => run_exec(&client, &cmd.endpoint, args).await,
        AgentAction::Journal(args) => {
            let value = submit_event_with_client(&client, &cmd.endpoint, args).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
    }
}

pub async fn default_handshake(endpoint: Option<&str>, persona: Option<&str>) -> Result<()> {
    let endpoint = endpoint.unwrap_or(DEFAULT_ENDPOINT);
    let persona = persona.unwrap_or(DEFAULT_PERSONA);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .context("building reqwest client")?;

    let args = HandshakeArgs {
        persona: persona.to_string(),
        keyboard: vec!["kitty".into(), "legacy".into()],
        graphics: vec!["kitty".into(), "minimal".into()],
        custom_caps: vec![],
    };

    run_handshake(&client, endpoint, args).await.map(|_| ())
}

async fn run_handshake(client: &Client, endpoint: &str, args: HandshakeArgs) -> Result<Value> {
    let mut capabilities: HashMap<String, Value> = HashMap::new();
    capabilities.insert("keyboard".into(), json!(args.keyboard));
    capabilities.insert("graphics".into(), json!(args.graphics));
    for (key, value) in args.custom_caps {
        capabilities.insert(key, value);
    }
    let payload = json!({
        "version": 1,
        "persona": args.persona,
        "capabilities": capabilities,
    });

    let url = format!("{}/sigma/handshake", endpoint.trim_end_matches('/'));
    let response: Value = client
        .post(url)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response)
}

async fn run_exec(client: &Client, endpoint: &str, args: ExecArgs) -> Result<()> {
    let command = resolve_command(&args).await?;
    let mut env_map: HashMap<String, String> = HashMap::new();
    for (k, v) in args.env {
        env_map.insert(k, v);
    }

    let payload = json!({
        "persona": args.persona,
        "command": "agent.exec",
        "spectral_tag": args.spectral_tag,
        "args": {
            "cmd": command,
            "cwd": args.cwd,
            "env": if env_map.is_empty() { Value::Null } else { json!(env_map) },
            "shell": args.shell,
        }
    });

    let url = format!("{}/ack/exec", endpoint.trim_end_matches('/'));
    let response: Value = client
        .post(url)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

fn parse_key_value(s: &str) -> Result<(String, Value)> {
    let (key, raw) = s
        .split_once('=')
        .ok_or_else(|| anyhow!("expected KEY=JSON"))?;
    let value = serde_json::from_str(raw).context("parsing capability JSON")?;
    Ok((key.to_string(), value))
}

fn parse_env(s: &str) -> Result<(String, String)> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| anyhow!("expected KEY=VALUE"))?;
    Ok((key.to_string(), value.to_string()))
}

fn parse_json(s: &str) -> Result<Value> {
    serde_json::from_str(s).context("invalid JSON payload")
}

async fn resolve_command(args: &ExecArgs) -> Result<String> {
    if let Some(cmd) = &args.cmd {
        return Ok(cmd.to_string());
    }
    let path = args
        .command_file
        .as_ref()
        .ok_or_else(|| anyhow!("command source missing"))?
        .clone();
    let content = task::spawn_blocking(move || std::fs::read_to_string(path)).await??;
    let cmd = content
        .lines()
        .next()
        .ok_or_else(|| anyhow!("command file is empty"))?;
    Ok(cmd.to_string())
}

async fn submit_event_with_client(
    client: &Client,
    endpoint: &str,
    args: JournalArgs,
) -> Result<Value> {
    let payload = json!({
        "kind": args.kind,
        "persona": args.persona,
        "spectral_tag": args.spectral_tag,
        "payload": args.payload,
        "bytes": args.bytes,
    });

    let url = format!("{}/journal/event", endpoint.trim_end_matches('/'));
    let response: Value = client
        .post(url)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response)
}

pub async fn submit_event(
    endpoint: Option<&str>,
    kind: &str,
    persona: Option<&str>,
    spectral_tag: Option<&str>,
    payload: Value,
    bytes: Option<usize>,
) -> Result<()> {
    let endpoint = endpoint.unwrap_or(DEFAULT_ENDPOINT);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .context("building reqwest client")?;
    let args = JournalArgs {
        kind: kind.to_string(),
        persona: persona.unwrap_or(DEFAULT_PERSONA).to_string(),
        spectral_tag: spectral_tag.map(|s| s.to_string()),
        payload,
        bytes,
    };
    let _ = submit_event_with_client(&client, endpoint, args).await?;
    Ok(())
}
