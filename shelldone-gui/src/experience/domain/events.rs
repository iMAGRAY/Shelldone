use super::value_object::ExperienceSurfaceId;

#[derive(Debug, Clone, PartialEq)]
pub enum ExperienceEventPayload {
    Registered { surface_id: ExperienceSurfaceId },
    Activated { surface_id: ExperienceSurfaceId },
    Removed { surface_id: ExperienceSurfaceId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExperienceEvent {
    pub sequence: u64,
    pub payload: ExperienceEventPayload,
}

impl ExperienceEvent {
    pub fn new(sequence: u64, payload: ExperienceEventPayload) -> Self {
        Self { sequence, payload }
    }
}
