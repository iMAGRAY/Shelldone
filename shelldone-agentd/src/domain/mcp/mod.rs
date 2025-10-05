pub mod aggregate;
pub mod events;
pub mod value_object;

#[allow(unused_imports)]
pub use aggregate::{McpSession, McpSessionSnapshot, SessionStatus};
pub use events::{McpDomainEvent, McpEventEnvelope};
pub use value_object::{CapabilityName, PersonaProfile, SessionId, ToolName};
