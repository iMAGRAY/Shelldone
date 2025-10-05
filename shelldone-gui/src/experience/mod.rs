pub mod adapters;
pub mod app;
pub mod domain;
pub mod ports;
pub mod telemetry;

#[allow(unused_imports)]
pub use app::service::{
    AgentSignal, ApprovalSignal, ExperienceOrchestrator, ExperienceSignal, ExperienceSurfaceCard,
    ExperienceViewModel, PersonaSignal,
};
#[allow(unused_imports)]
pub use app::{build_hub_state_from_snapshot, ExperienceHubState, ExperienceTelemetryService};
pub use telemetry::experience_telemetry;

pub fn experience_hub_service() -> ExperienceTelemetryService<&'static telemetry::TelemetryManager>
{
    ExperienceTelemetryService::new(experience_telemetry())
}
