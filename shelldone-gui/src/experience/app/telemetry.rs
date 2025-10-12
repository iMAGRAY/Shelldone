use super::service::{
    AgentSignal, ApprovalSignal, ExperienceOrchestrator, ExperienceSignal, ExperienceViewModel,
    PersonaSignal,
};
use crate::experience::app::service::{StateSnapshotSignal, TermBridgeTerminalSummary};
use crate::experience::domain::value_object::{ExperienceAgentState, ExperienceIntent};
use crate::experience::ports::telemetry_port::{StateSnapshotFrame, TermBridgeDeltaSnapshot};
use crate::experience::ports::{
    AgentFrame, AgentFrameStatus, ApprovalFrame, ExperienceTelemetryPort, PersonaFrame,
    TelemetrySnapshot,
};
use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};

/// Aggregates telemetry data into the Experience Hub view model.
pub struct ExperienceTelemetryService<P> {
    port: P,
}

/// Composite result containing raw telemetry and rendered hub view.
pub struct ExperienceHubState {
    pub snapshot: TelemetrySnapshot,
    pub signal: ExperienceSignal,
    pub view_model: ExperienceViewModel,
}

impl<P> ExperienceTelemetryService<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }
}

impl<P> ExperienceTelemetryService<P>
where
    P: ExperienceTelemetryPort,
{
    pub fn sync_hub_state(
        &self,
        workspace_name: &str,
        tab_count: usize,
        activity_count_hint: usize,
    ) -> Result<ExperienceHubState> {
        let snapshot = self.port.snapshot()?;
        build_hub_state_from_snapshot(snapshot, workspace_name, tab_count, activity_count_hint)
    }
}

pub fn build_hub_state_from_snapshot(
    snapshot: TelemetrySnapshot,
    workspace_name: &str,
    tab_count: usize,
    activity_count_hint: usize,
) -> Result<ExperienceHubState> {
    let approvals = map_approvals(&snapshot.approvals);
    let snapshots = map_state_snapshots(&snapshot.state_snapshots);
    let pending_approvals = approvals.len();
    metrics::gauge!("experience.approvals.pending").set(pending_approvals as f64);
    metrics::gauge!("experience.tabs.count").set(tab_count as f64);
    metrics::gauge!("experience.snapshots.count").set(snapshots.len() as f64);
    if snapshot.telemetry_ready {
        metrics::counter!("experience.telemetry.ready", "state" => "true").increment(1);
    } else {
        metrics::counter!("experience.telemetry.ready", "state" => "false").increment(1);
    }

    let telemetry_active = snapshot
        .agents
        .iter()
        .filter(|agent| matches!(agent.status, AgentFrameStatus::Active))
        .count();
    let active_automations = activity_count_hint.max(telemetry_active);

    let persona = persona_signal(
        workspace_name,
        tab_count,
        active_automations,
        snapshot.persona.as_ref(),
        snapshot.termbridge_delta.as_ref(),
    );

    let agents = if snapshot.agents.is_empty() {
        fallback_agent_signals(active_automations, pending_approvals)
    } else {
        build_agent_signals_from_snapshot(&snapshot, pending_approvals)
    };

    let termbridge_terminals = map_termbridge_terminals(snapshot.termbridge_delta.as_ref());

    let signal = ExperienceSignal {
        workspace_name: workspace_name.to_string(),
        persona: Some(persona.clone()),
        agents,
        approvals,
        snapshots: snapshots.clone(),
        tab_count,
        pending_approvals,
        active_automations,
        approvals_source: snapshot.approvals_source.clone(),
        termbridge_terminals,
    };

    let mut orchestrator = ExperienceOrchestrator::new();
    let view_model = orchestrator.sync(signal.clone())?;

    Ok(ExperienceHubState {
        snapshot,
        signal,
        view_model,
    })
}

fn map_approvals(frames: &[ApprovalFrame]) -> Vec<ApprovalSignal> {
    let mut approvals: Vec<_> = frames
        .iter()
        .map(|frame| ApprovalSignal {
            id: frame.id.clone(),
            command: frame.command.clone(),
            persona: frame.persona.clone(),
            reason: frame.reason.clone(),
            requested_at: frame.requested_at,
        })
        .collect();
    approvals.sort_by_key(|frame| frame.requested_at);
    approvals
}

fn map_state_snapshots(frames: &[StateSnapshotFrame]) -> Vec<StateSnapshotSignal> {
    let mut entries: Vec<_> = frames.iter().collect();
    entries.sort_by_key(|frame| frame.created_at);
    entries.reverse();
    entries
        .into_iter()
        .take(12)
        .map(|frame| {
            let label = if frame.tags.is_empty() {
                frame.id.clone()
            } else {
                format!("{} ({})", frame.id, frame.tags.join(", "))
            };
            StateSnapshotSignal {
                id: frame.id.clone(),
                label,
                created_at: frame.created_at,
                size_bytes: frame.size_bytes,
                path: frame.path.clone(),
                tags: frame.tags.clone(),
            }
        })
        .collect()
}

fn map_termbridge_terminals(
    delta: Option<&TermBridgeDeltaSnapshot>,
) -> Vec<TermBridgeTerminalSummary> {
    delta
        .map(|snapshot| {
            snapshot
                .terminals
                .iter()
                .map(|terminal| TermBridgeTerminalSummary {
                    terminal: terminal.terminal.clone(),
                    requires_opt_in: terminal.requires_opt_in,
                    source: terminal.source.clone(),
                    duplicate: terminal.capabilities.duplicate,
                    close: terminal.capabilities.close,
                    send_text: terminal.capabilities.send_text,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn persona_signal(
    workspace_name: &str,
    tab_count: usize,
    active_automations: usize,
    frame: Option<&PersonaFrame>,
    termbridge_delta: Option<&TermBridgeDeltaSnapshot>,
) -> PersonaSignal {
    match frame {
        Some(frame) => persona_from_frame(
            frame,
            workspace_name,
            tab_count,
            active_automations,
            termbridge_delta,
        ),
        None => fallback_persona(
            workspace_name,
            tab_count,
            active_automations,
            termbridge_delta,
        ),
    }
}

fn persona_from_frame(
    frame: &PersonaFrame,
    workspace_name: &str,
    tab_count: usize,
    active_automations: usize,
    termbridge_delta: Option<&TermBridgeDeltaSnapshot>,
) -> PersonaSignal {
    let fallback = fallback_persona(
        workspace_name,
        tab_count,
        active_automations,
        termbridge_delta,
    );

    let name = if frame.name.trim().is_empty() {
        fallback.name
    } else {
        frame.name.clone()
    };

    let intent = frame
        .intent_hint
        .as_ref()
        .and_then(|hint| intent_from_hint(hint))
        .unwrap_or(fallback.intent);

    let tone = frame
        .tone_hint
        .clone()
        .unwrap_or_else(|| tone_for_intent(intent).to_string());

    PersonaSignal { name, intent, tone }
}

fn fallback_persona(
    workspace_name: &str,
    tab_count: usize,
    active_automations: usize,
    termbridge_delta: Option<&TermBridgeDeltaSnapshot>,
) -> PersonaSignal {
    let workspace_lc = workspace_name.to_ascii_lowercase();
    let mut intent = if active_automations > 0 {
        ExperienceIntent::Automate
    } else if workspace_lc.contains("recover") || workspace_lc.contains("restore") {
        ExperienceIntent::Recover
    } else if tab_count > 4 {
        ExperienceIntent::Explore
    } else {
        ExperienceIntent::Focus
    };

    if let Some(delta) = termbridge_delta {
        if delta.changed {
            if !delta.removed.is_empty() {
                intent = ExperienceIntent::Recover;
            } else if delta
                .added
                .iter()
                .chain(delta.updated.iter())
                .any(|entry| entry.requires_opt_in)
            {
                intent = ExperienceIntent::Focus;
            }
        }
    }

    let tone = tone_for_intent(intent).to_string();
    let name = match workspace_lc.as_str() {
        "ops" | "production" => "Guardian",
        "ai" | "agents" => "Flux",
        name if name.contains("recover") => "Healer",
        _ => "Nova",
    };

    PersonaSignal {
        name: name.to_string(),
        intent,
        tone,
    }
}

fn intent_from_hint(hint: &str) -> Option<ExperienceIntent> {
    match hint.to_ascii_lowercase().as_str() {
        "automate" | "flux" => Some(ExperienceIntent::Automate),
        "recover" => Some(ExperienceIntent::Recover),
        "explore" => Some(ExperienceIntent::Explore),
        "focus" => Some(ExperienceIntent::Focus),
        _ => None,
    }
}

fn tone_for_intent(intent: ExperienceIntent) -> &'static str {
    match intent {
        ExperienceIntent::Automate => "strategic calm",
        ExperienceIntent::Explore => "playful precision",
        ExperienceIntent::Focus => "laser-focused clarity",
        ExperienceIntent::Recover => "grounded restoration",
    }
}

fn build_agent_signals_from_snapshot(
    snapshot: &TelemetrySnapshot,
    pending_approvals: usize,
) -> Vec<AgentSignal> {
    let snapshot_age = snapshot
        .generated_at
        .map(|ts| Utc::now().signed_duration_since(ts));

    snapshot
        .agents
        .iter()
        .map(|frame| {
            let state = match frame.status {
                AgentFrameStatus::Active => ExperienceAgentState::Running,
                AgentFrameStatus::Registered => {
                    if pending_approvals > 0 {
                        ExperienceAgentState::WaitingApproval
                    } else {
                        ExperienceAgentState::Idle
                    }
                }
                AgentFrameStatus::Disabled => ExperienceAgentState::Error,
            };
            let confidence = confidence_from_agent_frame(frame, state, snapshot_age);
            AgentSignal {
                id: frame.id.clone(),
                label: frame.label.clone(),
                state,
                confidence,
            }
        })
        .collect()
}

fn confidence_from_agent_frame(
    frame: &AgentFrame,
    state: ExperienceAgentState,
    snapshot_age: Option<ChronoDuration>,
) -> f32 {
    let mut confidence = match state {
        ExperienceAgentState::Running => {
            let base = 0.86;
            if let Some(heartbeat) = frame.last_heartbeat_at {
                let age = Utc::now().signed_duration_since(heartbeat);
                if age > ChronoDuration::minutes(15) {
                    0.45
                } else if age > ChronoDuration::minutes(5) {
                    0.62
                } else {
                    base
                }
            } else {
                base * 0.85
            }
        }
        ExperienceAgentState::Idle | ExperienceAgentState::WaitingApproval => {
            let age = Utc::now().signed_duration_since(frame.registered_at);
            if age < ChronoDuration::minutes(2) {
                0.6
            } else if age > ChronoDuration::hours(1) {
                0.48
            } else {
                0.52
            }
        }
        ExperienceAgentState::Error => 0.25,
    };

    if let Some(age) = snapshot_age {
        if age > ChronoDuration::minutes(10) {
            confidence *= 0.85;
        }
    }

    confidence
}

fn fallback_agent_signals(active_automations: usize, pending_approvals: usize) -> Vec<AgentSignal> {
    let mut agents = Vec::new();
    let nova_confidence = if active_automations > 0 { 0.82 } else { 0.58 };
    agents.push(AgentSignal {
        id: "nova".to_string(),
        label: "Nova".to_string(),
        state: if active_automations > 0 {
            ExperienceAgentState::Running
        } else {
            ExperienceAgentState::Idle
        },
        confidence: nova_confidence,
    });

    let guardian_state = if pending_approvals > (active_automations.saturating_add(1) * 2) {
        ExperienceAgentState::Error
    } else if pending_approvals > 0 {
        ExperienceAgentState::WaitingApproval
    } else {
        ExperienceAgentState::Idle
    };
    agents.push(AgentSignal {
        id: "guardian".to_string(),
        label: "Guardian".to_string(),
        state: guardian_state,
        confidence: if matches!(guardian_state, ExperienceAgentState::Error) {
            0.35
        } else if pending_approvals > 0 {
            0.66
        } else {
            0.52
        },
    });

    agents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::ports::telemetry_port::{
        TermBridgeTerminalCapabilities, TermBridgeTerminalChange,
    };
    use crate::experience::ports::{ApprovalSource, ExperienceTelemetryPort};

    struct StubPort {
        snapshot: TelemetrySnapshot,
    }

    impl ExperienceTelemetryPort for StubPort {
        fn snapshot(&self) -> anyhow::Result<TelemetrySnapshot> {
            Ok(self.snapshot.clone())
        }
    }

    fn approval(id: &str, seconds_ago: i64) -> ApprovalFrame {
        ApprovalFrame {
            id: id.to_string(),
            command: "agent.exec".to_string(),
            persona: Some("Nova".to_string()),
            reason: "danger".to_string(),
            requested_at: Utc::now() - ChronoDuration::seconds(seconds_ago),
        }
    }

    fn agent(id: &str, status: AgentFrameStatus, registered_secs: i64) -> AgentFrame {
        AgentFrame {
            id: id.to_string(),
            label: format!("Agent-{id}"),
            provider: "openai".to_string(),
            channel: None,
            status,
            last_heartbeat_at: Some(Utc::now() - ChronoDuration::seconds(30)),
            registered_at: Utc::now() - ChronoDuration::seconds(registered_secs),
        }
    }

    #[test]
    fn fallback_persona_prefers_recover_when_terminals_removed() {
        let mut delta = TermBridgeDeltaSnapshot {
            changed: true,
            ..Default::default()
        };
        delta.removed.push(TermBridgeTerminalChange {
            terminal: "wezterm".to_string(),
            requires_opt_in: false,
            source: None,
            capabilities: TermBridgeTerminalCapabilities::default(),
        });
        let persona = fallback_persona("dev", 2, 0, Some(&delta));
        assert_eq!(persona.intent, ExperienceIntent::Recover);
    }

    #[test]
    fn fallback_persona_prefers_focus_when_opt_in_needed() {
        let mut delta = TermBridgeDeltaSnapshot {
            changed: true,
            ..Default::default()
        };
        delta.added.push(TermBridgeTerminalChange {
            terminal: "kitty".to_string(),
            requires_opt_in: true,
            source: None,
            capabilities: TermBridgeTerminalCapabilities::default(),
        });
        let persona = fallback_persona("workspace", 1, 0, Some(&delta));
        assert_eq!(persona.intent, ExperienceIntent::Focus);
    }

    #[test]
    fn map_termbridge_terminals_preserves_source() {
        let mut delta = TermBridgeDeltaSnapshot::default();
        delta.terminals.push(TermBridgeTerminalChange {
            terminal: "mcp-sync-e2e".to_string(),
            requires_opt_in: false,
            source: Some("mcp".to_string()),
            capabilities: TermBridgeTerminalCapabilities {
                spawn: true,
                split: false,
                focus: true,
                duplicate: true,
                close: false,
                send_text: true,
            },
        });

        let summaries = map_termbridge_terminals(Some(&delta));
        assert_eq!(summaries.len(), 1);
        let summary = &summaries[0];
        assert_eq!(summary.terminal, "mcp-sync-e2e");
        assert_eq!(summary.source.as_deref(), Some("mcp"));
        assert!(!summary.requires_opt_in);
        assert!(summary.duplicate);
        assert!(!summary.close);
        assert!(summary.send_text);
    }

    #[test]
    fn builds_view_model_from_snapshot() {
        let snapshot = TelemetrySnapshot {
            generated_at: Some(Utc::now()),
            persona: Some(PersonaFrame {
                name: "Nova".to_string(),
                intent_hint: Some("explore".to_string()),
                tone_hint: Some("playful".to_string()),
            }),
            agents: vec![agent("nova", AgentFrameStatus::Active, 30)],
            approvals: vec![approval("1", 120)],
            telemetry_ready: true,
            approvals_source: ApprovalSource::Local,
            termbridge_delta: None,
            state_snapshots: vec![],
        };
        let service = ExperienceTelemetryService::new(StubPort { snapshot });

        let state = service.sync_hub_state("dev", 3, 2).expect("hub state");

        assert_eq!(state.signal.tab_count, 3);
        assert_eq!(state.signal.approvals.len(), 1);
        assert_eq!(state.signal.agents.len(), 1);
        assert_eq!(state.view_model.cards.len(), 4);
        assert_eq!(state.view_model.metrics.active_automations, 2);
        assert!(state.view_model.metrics.pending_approvals > 0);
        assert_eq!(
            state.view_model.metrics.approvals_source,
            ApprovalSource::Local
        );
        assert_eq!(state.signal.approvals_source, ApprovalSource::Local);
        assert!(state.snapshot.telemetry_ready);
        assert_eq!(state.snapshot.agents.len(), 1);
    }

    #[test]
    fn falls_back_to_persona_and_agents() {
        let snapshot = TelemetrySnapshot::default();
        let service = ExperienceTelemetryService::new(StubPort { snapshot });

        let state = service
            .sync_hub_state("RecoverOps", 6, 0)
            .expect("hub state");

        let persona = state.signal.persona.as_ref().expect("persona");
        assert_eq!(persona.intent, ExperienceIntent::Recover);
        assert_eq!(state.signal.agents.len(), 2);
        assert!(!state.snapshot.telemetry_ready);
        assert_eq!(state.signal.approvals_source, ApprovalSource::None);
    }

    #[test]
    fn approvals_sorted_oldest_first() {
        let snapshot = TelemetrySnapshot {
            generated_at: None,
            persona: None,
            agents: vec![],
            approvals: vec![approval("2", 30), approval("1", 120)],
            telemetry_ready: false,
            approvals_source: ApprovalSource::Local,
            termbridge_delta: None,
            state_snapshots: vec![],
        };
        let service = ExperienceTelemetryService::new(StubPort { snapshot });
        let state = service.sync_hub_state("dev", 1, 0).expect("hub state");

        let approvals: Vec<_> = state
            .signal
            .approvals
            .iter()
            .map(|a| a.id.clone())
            .collect();
        assert_eq!(approvals, vec!["1", "2"]);
        assert_eq!(state.snapshot.approvals.len(), 2);
    }
}
