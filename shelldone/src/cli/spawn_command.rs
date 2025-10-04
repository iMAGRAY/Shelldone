use crate::cli::resolve_relative_cwd;
use clap::{Parser, ValueHint};
use config::keyassignment::SpawnTabDomain;
use config::ConfigHandle;
use mux::pane::PaneId;
use mux::window::WindowId;
use portable_pty::cmdbuilder::CommandBuilder;
use serde_json::json;
use shelldone_client::client::Client;
use std::ffi::OsString;

#[derive(Debug, Parser, Clone)]
pub struct SpawnCommand {
    /// Specify the current pane.
    /// The default is to use the current pane based on the
    /// environment variable SHELLDONE_PANE.
    /// The pane is used to determine the current domain
    /// and window.
    #[arg(long)]
    pane_id: Option<PaneId>,

    #[arg(long)]
    domain_name: Option<String>,

    /// Specify the window into which to spawn a tab.
    /// If omitted, the window associated with the current
    /// pane is used.
    /// Cannot be used with `--workspace` or `--new-window`.
    #[arg(long, conflicts_with_all=&["workspace", "new_window"])]
    window_id: Option<WindowId>,

    /// Spawn into a new window, rather than a new tab.
    #[arg(long)]
    new_window: bool,

    /// Specify the current working directory for the initially
    /// spawned program
    #[arg(long, value_parser, value_hint=ValueHint::DirPath)]
    cwd: Option<OsString>,

    /// When creating a new window, override the default workspace name
    /// with the provided name.  The default name is "default".
    /// Requires `--new-window`.
    #[arg(long, requires = "new_window")]
    workspace: Option<String>,

    /// Instead of executing your shell, run PROG.
    /// For example: `shelldone cli spawn -- bash -l` will spawn bash
    /// as if it were a login shell.
    #[arg(value_parser, value_hint=ValueHint::CommandWithArguments, num_args=1..)]
    prog: Vec<OsString>,
}

impl SpawnCommand {
    pub async fn run(self, client: Client, config: &ConfigHandle) -> anyhow::Result<()> {
        let window_id = if self.new_window {
            None
        } else {
            match self.window_id {
                Some(w) => Some(w),
                None => {
                    let pane_id = client.resolve_pane_id(self.pane_id).await?;

                    let panes = client.list_panes().await?;
                    let mut window_id = None;
                    'outer: for tabroot in panes.tabs {
                        let mut cursor = tabroot.into_tree().cursor();

                        loop {
                            if let Some(entry) = cursor.leaf_mut() {
                                if entry.pane_id == pane_id {
                                    window_id.replace(entry.window_id);
                                    break 'outer;
                                }
                            }
                            match cursor.preorder_next() {
                                Ok(c) => cursor = c,
                                Err(_) => break,
                            }
                        }
                    }
                    window_id
                }
            }
        };

        let workspace = self
            .workspace
            .as_deref()
            .unwrap_or(
                config
                    .default_workspace
                    .as_deref()
                    .unwrap_or(mux::DEFAULT_WORKSPACE),
            )
            .to_string();

        let size = config.initial_size(0, None);

        let domain_name = self.domain_name.clone();
        let command_args = self.prog.clone();

        let spawned = client
            .spawn_v2(codec::SpawnV2 {
                domain: domain_name
                    .clone()
                    .map_or(SpawnTabDomain::DefaultDomain, SpawnTabDomain::DomainName),
                window_id,
                command: if command_args.is_empty() {
                    None
                } else {
                    let builder = CommandBuilder::from_argv(command_args.clone());
                    Some(builder)
                },
                command_dir: resolve_relative_cwd(self.cwd)?,
                size,
                workspace: workspace.clone(),
            })
            .await?;

        log::debug!("{:?}", spawned);
        println!("{}", spawned.pane_id);

        let endpoint_env = std::env::var("SHELLDONE_AGENT_ENDPOINT").ok();
        let persona_env = std::env::var("SHELLDONE_AGENT_PERSONA").ok();
        let argv_serialized = if command_args.is_empty() {
            None
        } else {
            Some(
                command_args
                    .iter()
                    .map(|arg| arg.to_string_lossy().to_string())
                    .collect::<Vec<_>>(),
            )
        };

        if let Err(err) = crate::cli::agent::submit_event(
            endpoint_env.as_deref(),
            "cli.spawn",
            persona_env.as_deref(),
            Some("exec::spawn"),
            json!({
                "pane_id": spawned.pane_id,
                "tab_id": spawned.tab_id,
                "window_id": spawned.window_id,
                "workspace": workspace,
                "domain": domain_name,
                "argv": argv_serialized,
            }),
            None,
        )
        .await
        {
            log::warn!("failed to journal spawn event: {err:?}");
        }

        Ok(())
    }
}
