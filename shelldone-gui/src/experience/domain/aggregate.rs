use super::events::{ExperienceEvent, ExperienceEventPayload};
use super::value_object::{
    ExperienceLayer, ExperienceSurface, ExperienceSurfaceId, ExperienceSurfaceRole,
};

#[derive(Default)]
pub struct ExperienceLayoutAggregate {
    surfaces: Vec<ExperienceSurface>,
    sequence: u64,
}

impl ExperienceLayoutAggregate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_surface(
        &mut self,
        surface: ExperienceSurface,
    ) -> anyhow::Result<ExperienceEvent> {
        if self
            .surfaces
            .iter()
            .any(|existing| existing.id() == surface.id())
        {
            anyhow::bail!("surface with id {} already registered", surface.id());
        }

        if surface.layer() == ExperienceLayer::Primary && surface.is_active() {
            self.deactivate_primary_layer();
        }

        if surface.role() == ExperienceSurfaceRole::Persona {
            self.remove_existing_persona();
        }

        self.surfaces.push(surface.clone());
        self.sequence += 1;
        Ok(ExperienceEvent::new(
            self.sequence,
            ExperienceEventPayload::SurfaceRegistered {
                surface_id: surface.id().clone(),
            },
        ))
    }

    pub fn activate_surface(
        &mut self,
        surface_id: &ExperienceSurfaceId,
    ) -> anyhow::Result<ExperienceEvent> {
        let mut target_layer = None;
        for existing in &self.surfaces {
            if existing.id() == surface_id {
                target_layer = Some(existing.layer());
                break;
            }
        }
        if target_layer.is_none() {
            anyhow::bail!("surface {surface_id} is not registered");
        }

        if target_layer == Some(ExperienceLayer::Primary) {
            self.deactivate_primary_layer();
        }

        for item in &mut self.surfaces {
            if item.id() == surface_id {
                *item = item.clone().with_active(true);
            } else if Some(item.layer()) == target_layer {
                *item = item.clone().with_active(false);
            }
        }

        self.sequence += 1;
        Ok(ExperienceEvent::new(
            self.sequence,
            ExperienceEventPayload::SurfaceActivated {
                surface_id: surface_id.clone(),
            },
        ))
    }

    pub fn remove_surface(
        &mut self,
        surface_id: &ExperienceSurfaceId,
    ) -> anyhow::Result<Option<ExperienceEvent>> {
        if let Some(index) = self
            .surfaces
            .iter()
            .position(|existing| existing.id() == surface_id)
        {
            self.surfaces.remove(index);
            self.sequence += 1;
            Ok(Some(ExperienceEvent::new(
                self.sequence,
                ExperienceEventPayload::SurfaceRemoved {
                    surface_id: surface_id.clone(),
                },
            )))
        } else {
            Ok(None)
        }
    }

    pub fn snapshot(&self) -> ExperienceLayoutSnapshot {
        ExperienceLayoutSnapshot {
            primary: self
                .surfaces
                .iter()
                .filter(|s| s.layer() == ExperienceLayer::Primary)
                .cloned()
                .collect(),
            overlays: self
                .surfaces
                .iter()
                .filter(|s| s.layer() == ExperienceLayer::Overlay)
                .cloned()
                .collect(),
            heads_up: self
                .surfaces
                .iter()
                .filter(|s| s.layer() == ExperienceLayer::HeadsUp)
                .cloned()
                .collect(),
        }
    }

    fn deactivate_primary_layer(&mut self) {
        for surface in &mut self.surfaces {
            if surface.layer() == ExperienceLayer::Primary {
                *surface = surface.clone().with_active(false);
            }
        }
    }

    fn remove_existing_persona(&mut self) {
        self.surfaces
            .retain(|surface| surface.role() != ExperienceSurfaceRole::Persona);
    }
}

#[derive(Clone, Debug)]
pub struct ExperienceLayoutSnapshot {
    pub primary: Vec<ExperienceSurface>,
    pub overlays: Vec<ExperienceSurface>,
    pub heads_up: Vec<ExperienceSurface>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::domain::events::ExperienceEventPayload;
    use crate::experience::domain::value_object::{
        ExperienceAgentState, ExperienceAgentStatus, ExperienceIntent, ExperiencePersona,
    };

    fn persona_surface(id_suffix: &str, active: bool) -> ExperienceSurface {
        let persona = ExperiencePersona::new("Nova", ExperienceIntent::Explore, "vivid");
        let agent = ExperienceAgentStatus::new(
            ExperienceSurfaceId::new("agent::nova").unwrap(),
            "Nova-Core",
            ExperienceAgentState::Running,
            0.8,
        )
        .unwrap();
        ExperienceSurface::new(
            ExperienceSurfaceId::new(format!("surface::{id_suffix}")).unwrap(),
            "Persona",
            ExperienceLayer::HeadsUp,
            ExperienceSurfaceRole::Persona,
            0.9,
            Some(persona),
            vec![agent],
            vec![],
            active,
        )
        .unwrap()
    }

    #[test]
    fn register_rejects_duplicates() {
        let mut aggregate = ExperienceLayoutAggregate::new();
        let surface = persona_surface("primary", true);
        aggregate.register_surface(surface.clone()).unwrap();
        assert!(aggregate.register_surface(surface).is_err());
    }

    #[test]
    fn activate_primary_deactivates_others() {
        let mut aggregate = ExperienceLayoutAggregate::new();
        let primary_a = ExperienceSurface::new(
            ExperienceSurfaceId::new("primary::a").unwrap(),
            "Workspace A",
            ExperienceLayer::Primary,
            ExperienceSurfaceRole::Workspace,
            1.0,
            None,
            vec![],
            vec![],
            true,
        )
        .unwrap();
        let primary_b = ExperienceSurface::new(
            ExperienceSurfaceId::new("primary::b").unwrap(),
            "Workspace B",
            ExperienceLayer::Primary,
            ExperienceSurfaceRole::Workspace,
            0.8,
            None,
            vec![],
            vec![],
            false,
        )
        .unwrap();

        aggregate.register_surface(primary_a).unwrap();
        aggregate.register_surface(primary_b).unwrap();
        aggregate
            .activate_surface(&ExperienceSurfaceId::new("primary::b").unwrap())
            .unwrap();

        let snapshot = aggregate.snapshot();
        assert_eq!(snapshot.primary.len(), 2);
        let active_count = snapshot
            .primary
            .iter()
            .filter(|surface| surface.is_active())
            .count();
        assert_eq!(active_count, 1);
        assert!(snapshot
            .primary
            .iter()
            .find(|surface| surface.id().as_str() == "primary::b")
            .unwrap()
            .is_active());
    }

    #[test]
    fn persona_is_singleton() {
        let mut aggregate = ExperienceLayoutAggregate::new();
        aggregate
            .register_surface(persona_surface("persona1", true))
            .unwrap();
        aggregate
            .register_surface(persona_surface("persona2", true))
            .unwrap();

        let snapshot = aggregate.snapshot();
        assert_eq!(snapshot.heads_up.len(), 1);
        assert_eq!(snapshot.heads_up[0].id().as_str(), "surface::persona2");
    }

    #[test]
    fn activate_and_remove_emit_events() {
        let mut aggregate = ExperienceLayoutAggregate::new();
        let surface_a = ExperienceSurface::new(
            ExperienceSurfaceId::new("primary::a").unwrap(),
            "Workspace A",
            ExperienceLayer::Primary,
            ExperienceSurfaceRole::Workspace,
            1.0,
            None,
            vec![],
            vec![],
            true,
        )
        .unwrap();
        let surface_b = ExperienceSurface::new(
            ExperienceSurfaceId::new("primary::b").unwrap(),
            "Workspace B",
            ExperienceLayer::Primary,
            ExperienceSurfaceRole::Workspace,
            0.8,
            None,
            vec![],
            vec![],
            false,
        )
        .unwrap();

        aggregate.register_surface(surface_a).unwrap();
        aggregate.register_surface(surface_b.clone()).unwrap();

        let activate_event = aggregate
            .activate_surface(surface_b.id())
            .expect("activation succeeds");
        assert!(matches!(
            activate_event.payload,
            ExperienceEventPayload::SurfaceActivated { .. }
        ));

        let removed = aggregate
            .remove_surface(surface_b.id())
            .expect("removal succeeds")
            .expect("event present");
        assert!(matches!(
            removed.payload,
            ExperienceEventPayload::SurfaceRemoved { .. }
        ));
    }
}
