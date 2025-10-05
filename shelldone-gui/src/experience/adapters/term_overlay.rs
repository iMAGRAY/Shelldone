use crate::experience::domain::value_object::{
    ExperienceAgentState, ExperienceLayer, ExperienceSurfaceRole,
};
use crate::experience::ports::{ExperienceRendererPort, ExperienceUiBlock, ExperienceUiFrame};
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

        blocks.extend(cards.into_iter().map(|card| ExperienceUiBlock {
            title: title_for_card(&card, view_model.metrics.tab_count),
            subtitle: subtitle_for_card(&card),
            body_lines: body_for_card(&card),
        }));

        let footer = format!(
            "Approvals pending: {} · Confidence: {}",
            view_model.metrics.pending_approvals,
            aggregate_confidence(view_model)
        );

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
    if card.role == ExperienceSurfaceRole::AgentFeed {
        let active_count = card
            .agents
            .iter()
            .filter(|agent| agent.state() == ExperienceAgentState::Running)
            .count();
        return Some(format!(
            "{} active · {} pending approvals · highlight {:.0}%",
            active_count,
            card.approvals.len(),
            card.highlight_ratio * 100.0
        ));
    }
    None
}

fn body_for_card(card: &crate::experience::ExperienceSurfaceCard) -> Vec<String> {
    if card.role == ExperienceSurfaceRole::AgentFeed {
        let mut lines = Vec::new();

        if !card.agents.is_empty() {
            lines.extend(card.agents.iter().map(|agent| {
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
            if !lines.is_empty() {
                lines.push("--- approvals ---".to_string());
            }
            lines.extend(card.approvals.iter().map(|approval| {
                let persona = approval
                    .persona()
                    .map(|p| format!(" persona {p}"))
                    .unwrap_or_default();
                format!(
                    "{:>10} · {:<18} ·{} {} · {}",
                    "PENDING",
                    approval.command(),
                    persona,
                    format_ago(approval.requested_at()),
                    approval.reason()
                )
            }));
        }

        if lines.is_empty() {
            lines.push(format!("id {}", card.id.as_str()));
            lines.push(format!(
                "highlight {:>4.0}% · active {}",
                card.highlight_ratio * 100.0,
                if card.active { "yes" } else { "no" }
            ));
        }

        lines
    } else {
        let mut lines = Vec::new();

        if !card.agents.is_empty() {
            lines.extend(card.agents.iter().map(|agent| {
                format!(
                    "{:>10} · {:<14} · confidence {:>5.0}% · id {}",
                    format_agent_state(agent.state()),
                    agent.label(),
                    agent.confidence() * 100.0,
                    agent.agent_id().as_str()
                )
            }));
        }

        if lines.is_empty() {
            lines.push(format!("id {}", card.id.as_str()));
            lines.push(format!(
                "highlight {:>4.0}% · active {}",
                card.highlight_ratio * 100.0,
                if card.active { "yes" } else { "no" }
            ));
        }

        lines
    }
}

fn format_agent_state(state: ExperienceAgentState) -> &'static str {
    match state {
        ExperienceAgentState::Idle => "IDLE",
        ExperienceAgentState::Running => "RUN",
        ExperienceAgentState::WaitingApproval => "WAIT",
        ExperienceAgentState::Error => "ERR",
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
    ExperienceUiBlock {
        title: "Layout Summary".to_string(),
        subtitle: None,
        body_lines: body,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::app::service::ExperienceMetrics;
    use crate::experience::domain::aggregate::ExperienceLayoutSnapshot;
    use crate::experience::domain::value_object::{
        ExperienceLayer, ExperienceSurfaceId, ExperienceSurfaceRole,
    };
    use crate::experience::{ExperienceSurfaceCard, ExperienceViewModel};

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
            },
            cards,
        };

        let renderer = TerminalOverlayRenderer::new();
        let frame = renderer.compose(&view_model);
        assert_eq!(frame.headline, "EXPERIENCE HUB · 2 tabs · 1 automations");
        assert_eq!(frame.blocks.len(), 2);
        assert!(frame.blocks[0].title.contains("Layout"));
        assert!(frame.footer.contains("Approvals"));
    }
}
