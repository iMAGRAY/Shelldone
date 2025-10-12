use super::super::domain::aggregate::{ExperienceLayoutAggregate, ExperienceLayoutSnapshot};
use super::super::domain::value_object::{
    ExperienceAgentState, ExperienceAgentStatus, ExperienceApproval, ExperienceIntent,
    ExperienceLayer, ExperiencePersona, ExperienceSurface, ExperienceSurfaceId,
    ExperienceSurfaceRole,
};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct ExperienceSignal {
    pub workspace_name: String,
    pub persona: Option<PersonaSignal>,
    pub agents: Vec<AgentSignal>,
    pub approvals: Vec<ApprovalSignal>,
    pub tab_count: usize,
    pub pending_approvals: usize,
    pub active_automations: usize,
}

#[derive(Clone, Debug)]
pub struct PersonaSignal {
    pub name: String,
    pub intent: ExperienceIntent,
    pub tone: String,
}

#[derive(Clone, Debug)]
pub struct AgentSignal {
    pub id: String,
    pub label: String,
    pub state: ExperienceAgentState,
    pub confidence: f32,
}

#[derive(Clone, Debug)]
pub struct ApprovalSignal {
    pub id: String,
    pub command: String,
    pub persona: Option<String>,
    pub reason: String,
    pub requested_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ExperienceMetrics {
    pub tab_count: usize,
    pub pending_approvals: usize,
    pub active_automations: usize,
}

#[derive(Clone, Debug)]
pub struct ExperienceSurfaceCard {
    pub id: ExperienceSurfaceId,
    pub title: String,
    pub layer: ExperienceLayer,
    pub role: ExperienceSurfaceRole,
    pub highlight_ratio: f32,
    pub persona: Option<ExperiencePersona>,
    pub agents: Vec<ExperienceAgentStatus>,
    pub active: bool,
    pub approvals: Vec<ExperienceApproval>,
}

#[derive(Clone, Debug)]
pub struct ExperienceViewModel {
    pub layout: ExperienceLayoutSnapshot,
    pub metrics: ExperienceMetrics,
    pub cards: Vec<ExperienceSurfaceCard>,
}

pub struct ExperienceOrchestrator {
    aggregate: ExperienceLayoutAggregate,
}

impl ExperienceOrchestrator {
    pub fn new() -> Self {
        Self {
            aggregate: ExperienceLayoutAggregate::new(),
        }
    }

    pub fn sync(&mut self, signal: ExperienceSignal) -> anyhow::Result<ExperienceViewModel> {
        self.aggregate = ExperienceLayoutAggregate::new();

        let workspace_surface = ExperienceSurface::new(
            ExperienceSurfaceId::new(format!("surface::workspace::{}", signal.workspace_name))?,
            format!("Workspace — {}", signal.workspace_name),
            ExperienceLayer::Primary,
            ExperienceSurfaceRole::Workspace,
            1.0,
            None,
            vec![],
            vec![],
            true,
        )?;
        self.aggregate.register_surface(workspace_surface.clone())?;
        let workspace_id = workspace_surface.id().clone();

        if let Some(persona_signal) = &signal.persona {
            let persona_surface = ExperienceSurface::new(
                ExperienceSurfaceId::new("surface::persona")?,
                format!("Persona — {}", persona_signal.name),
                ExperienceLayer::HeadsUp,
                ExperienceSurfaceRole::Persona,
                0.95,
                Some(ExperiencePersona::new(
                    &persona_signal.name,
                    persona_signal.intent,
                    &persona_signal.tone,
                )),
                vec![],
                vec![],
                true,
            )?;
            self.aggregate.register_surface(persona_surface)?;
        }

        let agent_statuses: Vec<ExperienceAgentStatus> = signal
            .agents
            .iter()
            .map(|agent| {
                let agent_id = ExperienceSurfaceId::new(format!("agent::{}", agent.id))?;
                ExperienceAgentStatus::new(
                    agent_id,
                    agent.label.clone(),
                    agent.state,
                    agent.confidence,
                )
            })
            .collect::<anyhow::Result<_>>()?;

        let approvals: Vec<ExperienceApproval> = signal
            .approvals
            .iter()
            .map(|approval| {
                ExperienceApproval::new(
                    approval.id.clone(),
                    approval.command.clone(),
                    approval.persona.clone(),
                    approval.reason.clone(),
                    approval.requested_at,
                )
            })
            .collect::<anyhow::Result<_>>()?;

        if !agent_statuses.is_empty() || !approvals.is_empty() {
            let pending_ratio = (signal.pending_approvals.max(1) as f32) / 4.0_f32;
            let agent_surface = ExperienceSurface::new(
                ExperienceSurfaceId::new("surface::agent-feed")?,
                "Agent Ops".to_string(),
                ExperienceLayer::Overlay,
                ExperienceSurfaceRole::AgentFeed,
                pending_ratio.clamp(0.15, 1.0),
                None,
                agent_statuses,
                approvals.clone(),
                true,
            )?;
            self.aggregate.register_surface(agent_surface)?;
        }

        let metrics_ratio = ((signal.active_automations as f32) + 1.0)
            / ((signal.tab_count.max(1) + signal.pending_approvals.max(1)) as f32);
        let metrics_surface = ExperienceSurface::new(
            ExperienceSurfaceId::new("surface::metrics")?,
            "Ops Metrics".to_string(),
            ExperienceLayer::Overlay,
            ExperienceSurfaceRole::Metrics,
            metrics_ratio.clamp(0.2, 1.0),
            None,
            vec![],
            vec![],
            false,
        )?;
        self.aggregate.register_surface(metrics_surface)?;

        let palette_surface_id = ExperienceSurfaceId::new("surface::command")?;
        let palette_surface = ExperienceSurface::new(
            palette_surface_id.clone(),
            "Command Palette".to_string(),
            ExperienceLayer::HeadsUp,
            ExperienceSurfaceRole::CommandPalette,
            0.25,
            None,
            vec![],
            vec![],
            false,
        )?;
        self.aggregate.register_surface(palette_surface)?;

        if let Ok(event) = self.aggregate.activate_surface(&workspace_id) {
            log::trace!("Experience hub activated workspace seq={}", event.sequence);
        }
        if signal.tab_count <= 1 {
            if let Ok(Some(event)) = self.aggregate.remove_surface(&palette_surface_id) {
                log::trace!("Experience hub pruned palette seq={}", event.sequence);
            }
        }

        let layout = self.aggregate.snapshot();
        let cards = self.collect_cards(&layout);

        Ok(ExperienceViewModel {
            layout,
            metrics: ExperienceMetrics {
                tab_count: signal.tab_count,
                pending_approvals: signal.pending_approvals,
                active_automations: signal.active_automations,
            },
            cards,
        })
    }

    fn collect_cards(&self, layout: &ExperienceLayoutSnapshot) -> Vec<ExperienceSurfaceCard> {
        let mut cards = Vec::new();
        for surface in layout
            .primary
            .iter()
            .chain(layout.overlays.iter())
            .chain(layout.heads_up.iter())
        {
            cards.push(ExperienceSurfaceCard {
                id: surface.id().clone(),
                title: surface.title().to_string(),
                layer: surface.layer(),
                role: surface.role(),
                highlight_ratio: surface.highlight_ratio(),
                persona: surface.persona().cloned(),
                agents: surface.agents().to_vec(),
                active: surface.is_active(),
                approvals: surface.approvals().to_vec(),
            });
        }
        cards
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_layout_with_persona_and_agents() {
        let mut orchestrator = ExperienceOrchestrator::new();
        let persona = PersonaSignal {
            name: "Nova".to_string(),
            intent: ExperienceIntent::Explore,
            tone: "playful precision".to_string(),
        };
        let agents = vec![AgentSignal {
            id: "nova".to_string(),
            label: "Nova-Core".to_string(),
            state: ExperienceAgentState::Running,
            confidence: 0.8,
        }];

        let view_model = orchestrator
            .sync(ExperienceSignal {
                workspace_name: "dev".to_string(),
                persona: Some(persona),
                agents,
                tab_count: 3,
                pending_approvals: 1,
                active_automations: 2,
                approvals: vec![],
            })
            .unwrap();

        assert_eq!(view_model.metrics.tab_count, 3);
        assert_eq!(view_model.layout.primary.len(), 1);
        assert_eq!(view_model.layout.overlays.len(), 2);
        assert_eq!(view_model.layout.heads_up.len(), 2);
        assert_eq!(view_model.cards.len(), 5);
    }

    #[test]
    fn approvals_attach_to_agent_surface() {
        let mut orchestrator = ExperienceOrchestrator::new();
        let approval = ApprovalSignal {
            id: "approval-1".to_string(),
            command: "agent.exec".to_string(),
            persona: Some("Nova".to_string()),
            reason: "Dangerous command".to_string(),
            requested_at: Utc::now(),
        };

        let view_model = orchestrator
            .sync(ExperienceSignal {
                workspace_name: "dev".to_string(),
                persona: None,
                agents: vec![],
                tab_count: 2,
                pending_approvals: 1,
                active_automations: 0,
                approvals: vec![approval.clone()],
            })
            .unwrap();

        let agent_card = view_model
            .cards
            .iter()
            .find(|card| card.role == ExperienceSurfaceRole::AgentFeed)
            .expect("agent feed card");
        assert_eq!(agent_card.approvals.len(), 1);
        assert_eq!(agent_card.approvals[0].id(), approval.id);
        assert_eq!(agent_card.approvals[0].command(), approval.command);
    }
}
