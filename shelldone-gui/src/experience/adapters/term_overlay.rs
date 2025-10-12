use crate::experience::domain::value_object::{
    ExperienceAgentState, ExperienceLayer, ExperienceSurfaceRole,
};
use crate::experience::ports::{
    ApprovalSource, ExperienceRendererPort, ExperienceUiBlock, ExperienceUiFrame,
};
use crate::experience::ExperienceViewModel;
use chrono::{DateTime, Utc};

#[derive(Default)]
pub struct TerminalOverlayRenderer;

impl TerminalOverlayRenderer {
    pub fn new() -> Self {
        Self {}
    }
}

impl ExperienceRendererPort for TerminalOverlayRenderer {
    fn compose(&self, view_model: &ExperienceViewModel) -> ExperienceUiFrame {
        let headline = format!(
            "EXPERIENCE HUB · {} tabs · {} automations",
            view_model.metrics.tab_count, view_model.metrics.active_automations
        );

        let mut cards = view_model.cards.clone();
        cards.sort_by_key(|card| match card.layer {
            ExperienceLayer::Foundation => 0,
            ExperienceLayer::Primary => 1,
            ExperienceLayer::Overlay => 2,
            ExperienceLayer::HeadsUp => 3,
        });

        let mut blocks = vec![layout_block(view_model)];

        if let Some(block) = persona_callout_block(view_model) {
            blocks.push(block);
        }

        if let Some(block) = approvals_callout_block(view_model) {
            blocks.push(block);
        }

        if let Some(block) = state_sync_callout_block(view_model) {
            blocks.push(block);
        }

        blocks.extend(cards.into_iter().map(|card| ExperienceUiBlock {
            title: title_for_card(&card, view_model.metrics.tab_count),
            subtitle: subtitle_for_card(&card),
            body_lines: body_for_card(&card),
        }));

        let approval_path = approval_source_label(&view_model.metrics.approvals_source);
        let footer = format!(
            "Approvals pending: {} ({}) · Confidence: {}",
            view_model.metrics.pending_approvals,
            approval_path,
            aggregate_confidence(view_model)
        );

        match view_model.metrics.approvals_source {
            ApprovalSource::None => blocks.push(ExperienceUiBlock {
                title: "Approvals Warning".to_string(),
                subtitle: Some("HTTP /approvals/pending unreachable".to_string()),
                body_lines: vec![
                    "Fallback disabled: approvals data unavailable.".to_string(),
                    "Run `make status` and check agent logs.".to_string(),
                ],
            }),
            ApprovalSource::Local => blocks.push(ExperienceUiBlock {
                title: "Approvals Notice".to_string(),
                subtitle: Some("Using local snapshot".to_string()),
                body_lines: vec![
                    "HTTP endpoint offline; displaying cached approvals.".to_string(),
                    "Run `make status` to refresh live feed.".to_string(),
                ],
            }),
            ApprovalSource::Http => {}
        }

        ExperienceUiFrame {
            headline,
            blocks,
            footer,
        }
    }
}

fn title_for_card(card: &crate::experience::ExperienceSurfaceCard, tab_count: usize) -> String {
    let layer = match card.layer {
        ExperienceLayer::Foundation => "foundation",
        ExperienceLayer::Primary => "primary",
        ExperienceLayer::Overlay => "overlay",
        ExperienceLayer::HeadsUp => "headsup",
    };
    match card.role {
        ExperienceSurfaceRole::Workspace => format!("Workspace ({layer}) · {}", card.title),
        ExperienceSurfaceRole::Persona => format!("Persona ({layer}) · {}", card.title),
        ExperienceSurfaceRole::AgentFeed => {
            let approvals = card.approvals.len();
            if approvals > 0 {
                format!(
                    "Agents ({layer}) · {} entries · {} approvals",
                    card.agents.len(),
                    approvals
                )
            } else {
                format!("Agents ({layer}) · {} entries", card.agents.len())
            }
        }
        ExperienceSurfaceRole::Metrics => format!("Metrics ({layer}) · {} tabs", tab_count),
        ExperienceSurfaceRole::CommandPalette => format!("Palette ({layer})"),
        ExperienceSurfaceRole::StateSync => {
            format!("State Sync ({layer}) · {} snapshots", card.snapshots.len())
        }
    }
}

fn subtitle_for_card(card: &crate::experience::ExperienceSurfaceCard) -> Option<String> {
    if let Some(persona) = &card.persona {
        return Some(format!(
            "Persona: {} · Intent: {:?} · Tone: {}",
            persona.name(),
            persona.intent(),
            persona.tone()
        ));
    }
    match card.role {
        ExperienceSurfaceRole::AgentFeed => {
            let active_count = card
                .agents
                .iter()
                .filter(|agent| agent.state() == ExperienceAgentState::Running)
                .count();
            Some(format!(
                "{} active · {} pending approvals · highlight {:.0}%",
                active_count,
                card.approvals.len(),
                card.highlight_ratio * 100.0
            ))
        }
        ExperienceSurfaceRole::StateSync => card.snapshots.first().map(|latest| {
            format!(
                "Latest {} · {}",
                format_ago(latest.created_at()),
                format_size(latest.size_bytes())
            )
        }),
        _ => None,
    }
}

fn body_for_card(card: &crate::experience::ExperienceSurfaceCard) -> Vec<String> {
    let mut lines = match card.role {
        ExperienceSurfaceRole::AgentFeed => {
            let mut feed_lines = Vec::new();
            if !card.agents.is_empty() {
                feed_lines.extend(card.agents.iter().map(|agent| {
                    format!(
                        "{:>10} · {:<14} · confidence {:>5.0}% · id {}",
                        format_agent_state(agent.state()),
                        agent.label(),
                        agent.confidence() * 100.0,
                        agent.agent_id().as_str()
                    )
                }));
            }
            if !card.approvals.is_empty() {
                if !feed_lines.is_empty() {
                    feed_lines.push("--- approvals ---".to_string());
                }
                feed_lines.extend(card.approvals.iter().map(|approval| {
                    let persona = approval
                        .persona()
                        .map(|p| format!(" persona {p}"))
                        .unwrap_or_default();
                    format!(
                        "{:>10} · {:<18} ·{} {} · {} · id {}",
                        "PENDING",
                        truncate_middle(approval.command(), 18),
                        persona,
                        format_ago(approval.requested_at()),
                        approval.reason(),
                        approval.id()
                    )
                }));
            }
            feed_lines
        }
        ExperienceSurfaceRole::StateSync => {
            if card.snapshots.is_empty() {
                vec!["No snapshots available".to_string()]
            } else {
                let mut snapshot_lines: Vec<String> = card
                    .snapshots
                    .iter()
                    .enumerate()
                    .take(6)
                    .map(|(index, snapshot)| {
                        let tags = if snapshot.tags().is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", snapshot.tags().join(", "))
                        };
                        format!(
                            "#{:>2} {} · {} · {}{} · id {}",
                            index + 1,
                            snapshot.label(),
                            format_ago(snapshot.created_at()),
                            format_size(snapshot.size_bytes()),
                            tags,
                            snapshot.id()
                        )
                    })
                    .collect();
                if let Some(latest) = card.snapshots.first() {
                    snapshot_lines.push(format!(
                        "path {} · id {}",
                        truncate_middle(latest.path(), 44),
                        latest.id()
                    ));
                }
                snapshot_lines
            }
        }
        _ => {
            if card.agents.is_empty() {
                Vec::new()
            } else {
                card.agents
                    .iter()
                    .map(|agent| {
                        format!(
                            "{:>10} · {:<14} · confidence {:>5.0}% · id {}",
                            format_agent_state(agent.state()),
                            agent.label(),
                            agent.confidence() * 100.0,
                            agent.agent_id().as_str()
                        )
                    })
                    .collect()
            }
        }
    };

    lines.push(format!(
        "{} focus {:>3}% · {}",
        progress_bar(card.highlight_ratio, 12),
        (card.highlight_ratio * 100.0).round() as i32,
        if card.active { "active" } else { "idle" }
    ));
    lines.push(format!("surface {}", card.id.as_str()));
    lines
}

fn format_agent_state(state: ExperienceAgentState) -> &'static str {
    match state {
        ExperienceAgentState::Idle => "IDLE",
        ExperienceAgentState::Running => "RUN",
        ExperienceAgentState::WaitingApproval => "WAIT",
        ExperienceAgentState::Error => "ERR",
    }
}

fn approval_source_label(source: &ApprovalSource) -> &'static str {
    match source {
        ApprovalSource::Http => "live",
        ApprovalSource::Local => "local snapshot",
        ApprovalSource::None => "unavailable",
    }
}

fn format_ago(timestamp: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now.signed_duration_since(timestamp);
    if delta < chrono::Duration::seconds(0) {
        return "now".to_string();
    }
    if delta < chrono::Duration::seconds(60) {
        return format!("{}s ago", delta.num_seconds());
    }
    if delta < chrono::Duration::minutes(60) {
        return format!("{}m ago", delta.num_minutes());
    }
    if delta < chrono::Duration::hours(24) {
        return format!("{}h ago", delta.num_hours());
    }
    format!("{}d ago", delta.num_days())
}

fn layout_block(view_model: &ExperienceViewModel) -> ExperienceUiBlock {
    let snapshot = &view_model.layout;
    let mut body = Vec::new();
    body.push(format!(
        "primary: {} · overlays: {} · heads-up: {}",
        snapshot.primary.len(),
        snapshot.overlays.len(),
        snapshot.heads_up.len()
    ));
    if let Some(first_primary) = snapshot.primary.first() {
        body.push(format!(
            "primary focus: {} (layer {:?})",
            first_primary.title(),
            first_primary.layer()
        ));
    }
    if !view_model.metrics.termbridge_terminals.is_empty() {
        body.push("termbridge actions:".to_string());
        for terminal in &view_model.metrics.termbridge_terminals {
            let mut actions = Vec::new();
            if terminal.duplicate {
                actions.push("duplicate");
            }
            if terminal.close {
                actions.push("close");
            }
            if terminal.send_text {
                actions.push("send_text");
            }
            let action_summary = if actions.is_empty() {
                "no actions".to_string()
            } else {
                actions.join("/")
            };
            let source_suffix = terminal
                .source
                .as_deref()
                .and_then(|value| {
                    if value.eq_ignore_ascii_case("local") {
                        None
                    } else {
                        Some(format!(" · source {}", value))
                    }
                })
                .unwrap_or_default();
            if terminal.requires_opt_in {
                body.push(format!(
                    "{} · opt-in · {}{}",
                    terminal.terminal, action_summary, source_suffix
                ));
            } else {
                body.push(format!(
                    "{} · {}{}",
                    terminal.terminal, action_summary, source_suffix
                ));
            }
        }
    }
    ExperienceUiBlock {
        title: "Layout Summary".to_string(),
        subtitle: None,
        body_lines: body,
    }
}

fn persona_callout_block(view_model: &ExperienceViewModel) -> Option<ExperienceUiBlock> {
    let persona_card = view_model
        .cards
        .iter()
        .find(|card| card.role == ExperienceSurfaceRole::Persona && card.persona.is_some())?;
    let persona = persona_card.persona.as_ref()?;
    let mut body = Vec::new();
    body.push(format!(
        "intent {:?} · tone {}",
        persona.intent(),
        persona.tone()
    ));
    if !persona_card.agents.is_empty() {
        let mut agent_lines: Vec<String> = persona_card
            .agents
            .iter()
            .map(|agent| {
                format!(
                    "{} {:<14} · conf {:>5.0}%",
                    format_agent_state(agent.state()),
                    agent.label(),
                    agent.confidence() * 100.0
                )
            })
            .collect();
        body.append(&mut agent_lines);
    }
    body.push(format!(
        "{} focus {:>3}% · {}",
        progress_bar(persona_card.highlight_ratio, 12),
        (persona_card.highlight_ratio * 100.0).round() as i32,
        if persona_card.active {
            "active"
        } else {
            "idle"
        }
    ));

    Some(ExperienceUiBlock {
        title: format!("Persona Spotlight · {}", persona.name()),
        subtitle: Some("Heads-up display".to_string()),
        body_lines: body,
    })
}

fn approvals_callout_block(view_model: &ExperienceViewModel) -> Option<ExperienceUiBlock> {
    let mut approvals: Vec<_> = view_model
        .cards
        .iter()
        .flat_map(|card| card.approvals.iter())
        .collect();

    if approvals.is_empty() {
        return None;
    }

    approvals.sort_by_key(|approval| approval.requested_at());

    let mut body = Vec::new();
    for (index, approval) in approvals.iter().rev().take(3).enumerate() {
        let persona = approval
            .persona()
            .map(|p| format!(" persona {p}"))
            .unwrap_or_default();
        body.push(format!(
            "#{:>2} {}{} · {} · {}",
            index + 1,
            truncate_middle(approval.command(), 24),
            persona,
            format_ago(approval.requested_at()),
            truncate_middle(approval.reason(), 36)
        ));
    }

    Some(ExperienceUiBlock {
        title: "Approvals Queue".to_string(),
        subtitle: Some(format!(
            "{} pending · source {}",
            approvals.len(),
            approval_source_label(&view_model.metrics.approvals_source)
        )),
        body_lines: body,
    })
}

fn state_sync_callout_block(view_model: &ExperienceViewModel) -> Option<ExperienceUiBlock> {
    let card = view_model
        .cards
        .iter()
        .find(|card| card.role == ExperienceSurfaceRole::StateSync && !card.snapshots.is_empty())?;
    let latest = card.snapshots.first()?;
    let mut body = Vec::new();
    body.push(format!(
        "{} · {}",
        format_ago(latest.created_at()),
        format_size(latest.size_bytes())
    ));
    body.push(truncate_middle(latest.path(), 48));
    if card.snapshots.len() > 1 {
        body.push(format!("+{} more", card.snapshots.len() - 1));
    }
    Some(ExperienceUiBlock {
        title: "State Sync".to_string(),
        subtitle: Some("Latest snapshot".to_string()),
        body_lines: body,
    })
}

fn aggregate_confidence(view_model: &ExperienceViewModel) -> String {
    let mut confidences = vec![];
    for card in &view_model.cards {
        confidences.extend(card.agents.iter().map(|agent| agent.confidence()));
    }

    if confidences.is_empty() {
        "—".to_string()
    } else {
        let avg: f32 = confidences.iter().sum::<f32>() / confidences.len() as f32;
        format!("{avg:.0}%")
    }
}

fn progress_bar(ratio: f32, width: usize) -> String {
    let clamped = ratio.clamp(0.0, 1.0);
    let filled = (clamped * width as f32).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    let mut bar = String::with_capacity(width + 2);
    bar.push('[');
    bar.push_str(&"#".repeat(filled));
    bar.push_str(&"-".repeat(empty));
    bar.push(']');
    bar
}

fn truncate_middle(input: &str, max: usize) -> String {
    if input.len() <= max {
        return input.to_string();
    }
    if max <= 3 {
        return "...".to_string();
    }
    let half = (max - 3) / 2;
    let rest = max - 3 - half;
    format!("{}...{}", &input[..half], &input[input.len() - rest..])
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let size = bytes as f64;
    if size < KB {
        format!("{:.0} B", size)
    } else if size < MB {
        format!("{:.1} KB", size / KB)
    } else if size < GB {
        format!("{:.1} MB", size / MB)
    } else {
        format!("{:.2} GB", size / GB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::app::service::{ExperienceMetrics, TermBridgeTerminalSummary};
    use crate::experience::domain::aggregate::ExperienceLayoutSnapshot;
    use crate::experience::domain::value_object::{
        ExperienceAgentState, ExperienceAgentStatus, ExperienceApproval, ExperienceIntent,
        ExperienceLayer, ExperiencePersona, ExperienceSnapshot, ExperienceSurfaceId,
        ExperienceSurfaceRole,
    };
    use crate::experience::ports::ApprovalSource;
    use crate::experience::{ExperienceSurfaceCard, ExperienceViewModel};
    use chrono::{Duration as ChronoDuration, Utc};

    #[test]
    fn compose_builds_blocks() {
        let cards = vec![ExperienceSurfaceCard {
            id: ExperienceSurfaceId::new("surface::workspace::dev").unwrap(),
            title: "Workspace — dev".to_string(),
            layer: ExperienceLayer::Primary,
            role: ExperienceSurfaceRole::Workspace,
            highlight_ratio: 1.0,
            persona: None,
            agents: vec![],
            active: true,
            approvals: vec![],
            snapshots: vec![],
        }];
        let view_model = ExperienceViewModel {
            layout: ExperienceLayoutSnapshot {
                primary: vec![],
                overlays: vec![],
                heads_up: vec![],
            },
            metrics: ExperienceMetrics {
                tab_count: 2,
                pending_approvals: 0,
                active_automations: 1,
                approvals_source: ApprovalSource::None,
                termbridge_terminals: vec![],
            },
            cards,
        };

        let renderer = TerminalOverlayRenderer::new();
        let frame = renderer.compose(&view_model);
        assert_eq!(frame.headline, "EXPERIENCE HUB · 2 tabs · 1 automations");
        assert_eq!(frame.blocks.len(), 3);
        assert!(frame.blocks[0].title.contains("Layout"));
        assert!(frame
            .blocks
            .iter()
            .any(|b| b.title.contains("Approvals Warning")));
        assert!(frame.footer.contains("Approvals"));
        assert!(frame
            .blocks
            .iter()
            .any(|b| b.body_lines.iter().any(|line| line.contains("[#"))));
    }

    #[test]
    fn compose_lists_termbridge_sources() {
        let cards = vec![ExperienceSurfaceCard {
            id: ExperienceSurfaceId::new("surface::workspace::dev").unwrap(),
            title: "Workspace — dev".to_string(),
            layer: ExperienceLayer::Primary,
            role: ExperienceSurfaceRole::Workspace,
            highlight_ratio: 1.0,
            persona: None,
            agents: vec![],
            active: true,
            approvals: vec![],
            snapshots: vec![],
        }];
        let view_model = ExperienceViewModel {
            layout: ExperienceLayoutSnapshot {
                primary: vec![],
                overlays: vec![],
                heads_up: vec![],
            },
            metrics: ExperienceMetrics {
                tab_count: 1,
                pending_approvals: 0,
                active_automations: 0,
                approvals_source: ApprovalSource::None,
                termbridge_terminals: vec![TermBridgeTerminalSummary {
                    terminal: "mcp-sync-e2e".to_string(),
                    requires_opt_in: false,
                    source: Some("mcp".to_string()),
                    duplicate: true,
                    close: false,
                    send_text: true,
                }],
            },
            cards,
        };

        let renderer = TerminalOverlayRenderer::new();
        let frame = renderer.compose(&view_model);
        let layout_block = frame
            .blocks
            .iter()
            .find(|block| block.title.contains("Layout"))
            .expect("layout summary block present");
        assert!(
            layout_block
                .body_lines
                .iter()
                .any(|line| line.contains("source mcp")),
            "expected layout summary to include source indicator"
        );
    }

    #[test]
    fn compose_notes_local_snapshot() {
        let cards = vec![];
        let view_model = ExperienceViewModel {
            layout: ExperienceLayoutSnapshot {
                primary: vec![],
                overlays: vec![],
                heads_up: vec![],
            },
            metrics: ExperienceMetrics {
                tab_count: 1,
                pending_approvals: 2,
                active_automations: 0,
                approvals_source: ApprovalSource::Local,
                termbridge_terminals: vec![],
            },
            cards,
        };

        let renderer = TerminalOverlayRenderer::new();
        let frame = renderer.compose(&view_model);
        assert_eq!(frame.blocks.len(), 2);
        assert!(frame.blocks[1].title.contains("Approvals Notice"));
        assert!(frame.footer.contains("local snapshot"));
    }

    #[test]
    fn compose_includes_approvals_queue() {
        let approval = ExperienceApproval::new(
            "app-1",
            "cargo ship release",
            Some("Nova".to_string()),
            "waiting for lead",
            Utc::now() - chrono::Duration::minutes(2),
        )
        .unwrap();
        let agent_status = ExperienceAgentStatus::new(
            ExperienceSurfaceId::new("agent::nova").unwrap(),
            "Nova",
            ExperienceAgentState::Running,
            0.87,
        )
        .unwrap();
        let cards = vec![ExperienceSurfaceCard {
            id: ExperienceSurfaceId::new("surface::agent-feed").unwrap(),
            title: "Agent Ops".into(),
            layer: ExperienceLayer::Overlay,
            role: ExperienceSurfaceRole::AgentFeed,
            highlight_ratio: 0.6,
            persona: None,
            agents: vec![agent_status],
            active: true,
            approvals: vec![approval],
            snapshots: vec![],
        }];
        let view_model = ExperienceViewModel {
            layout: ExperienceLayoutSnapshot {
                primary: vec![],
                overlays: vec![],
                heads_up: vec![],
            },
            metrics: ExperienceMetrics {
                tab_count: 3,
                pending_approvals: 1,
                active_automations: 2,
                approvals_source: ApprovalSource::Http,
                termbridge_terminals: vec![],
            },
            cards,
        };

        let renderer = TerminalOverlayRenderer::new();
        let frame = renderer.compose(&view_model);
        assert!(frame
            .blocks
            .iter()
            .any(|block| block.title == "Approvals Queue"));
    }

    #[test]
    fn compose_includes_persona_callout() {
        let persona = ExperiencePersona::new("Nova", ExperienceIntent::Explore, "warm");
        let cards = vec![ExperienceSurfaceCard {
            id: ExperienceSurfaceId::new("surface::persona").unwrap(),
            title: "Persona — Nova".into(),
            layer: ExperienceLayer::HeadsUp,
            role: ExperienceSurfaceRole::Persona,
            highlight_ratio: 0.8,
            persona: Some(persona),
            agents: vec![],
            active: true,
            approvals: vec![],
            snapshots: vec![],
        }];
        let view_model = ExperienceViewModel {
            layout: ExperienceLayoutSnapshot {
                primary: vec![],
                overlays: vec![],
                heads_up: vec![],
            },
            metrics: ExperienceMetrics {
                tab_count: 1,
                pending_approvals: 0,
                active_automations: 0,
                approvals_source: ApprovalSource::Http,
                termbridge_terminals: vec![],
            },
            cards,
        };

        let renderer = TerminalOverlayRenderer::new();
        let frame = renderer.compose(&view_model);
        assert!(frame
            .blocks
            .iter()
            .any(|block| block.title.contains("Persona Spotlight")));
    }

    #[test]
    fn compose_includes_state_sync_callout() {
        let snapshot = ExperienceSnapshot::new(
            "snap-001",
            "Manual backup",
            Utc::now() - ChronoDuration::minutes(5),
            8 * 1024 * 1024,
            "/var/tmp/snap-001.json.zst",
            vec!["manual".to_string()],
        )
        .unwrap();
        let cards = vec![ExperienceSurfaceCard {
            id: ExperienceSurfaceId::new("surface::state-sync").unwrap(),
            title: "State Sync".into(),
            layer: ExperienceLayer::Overlay,
            role: ExperienceSurfaceRole::StateSync,
            highlight_ratio: 0.75,
            persona: None,
            agents: vec![],
            active: true,
            approvals: vec![],
            snapshots: vec![snapshot],
        }];
        let view_model = ExperienceViewModel {
            layout: ExperienceLayoutSnapshot {
                primary: vec![],
                overlays: vec![],
                heads_up: vec![],
            },
            metrics: ExperienceMetrics {
                tab_count: 4,
                pending_approvals: 0,
                active_automations: 1,
                approvals_source: ApprovalSource::Http,
                termbridge_terminals: vec![],
            },
            cards,
        };

        let renderer = TerminalOverlayRenderer::new();
        let frame = renderer.compose(&view_model);
        assert!(frame
            .blocks
            .iter()
            .any(|block| block.title.contains("State Sync")));
        let state_block = frame
            .blocks
            .iter()
            .find(|block| block.title.contains("State Sync"))
            .expect("state callout block");
        assert!(state_block
            .subtitle
            .as_ref()
            .expect("subtitle")
            .contains("Latest"));
    }
}
