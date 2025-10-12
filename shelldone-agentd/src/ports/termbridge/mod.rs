pub mod binding_repo;
pub mod capability_repo;
pub mod clipboard_port;
pub mod consent_repo;
pub mod terminal_port;

pub use binding_repo::TerminalBindingRepository;
pub use capability_repo::TermBridgeStateRepository;
pub use clipboard_port::{
    ClipboardBackend, ClipboardError, ClipboardFailureTrace, ClipboardReadRequest,
    ClipboardReadResult, ClipboardServiceError, ClipboardWriteRequest, ClipboardWriteResult,
};
pub use consent_repo::{ConsentRepository, FileConsentRepository};
pub use terminal_port::{
    CapabilityObservation, DuplicateOptions, DuplicateStrategy, SpawnRequest,
    TermBridgeCommandRequest, TermBridgeError, TerminalControlPort,
};
