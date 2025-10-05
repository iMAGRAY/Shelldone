use crate::domain::termbridge::{
    ClipboardBackendDescriptor, ClipboardChannel, ClipboardContent, ClipboardMime,
};
use async_trait::async_trait;
use thiserror::Error;

/// Runtime error for clipboard operations.
#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("no clipboard backends configured")]
    NoBackends,
    #[error("channel {0} not supported")]
    ChannelNotSupported(String),
    #[error("clipboard operation not supported by backend {backend}")]
    OperationNotSupported { backend: String },
    #[error("clipboard payload too large: {size} bytes (limit {limit})")]
    PayloadTooLarge { size: usize, limit: usize },
    #[error("clipboard backend {backend} failed: {reason}")]
    BackendFailure { backend: String, reason: String },
}

impl ClipboardError {
    pub fn backend_failure(backend: impl Into<String>, reason: impl Into<String>) -> Self {
        ClipboardError::BackendFailure {
            backend: backend.into(),
            reason: reason.into(),
        }
    }
}

/// Clipboard backend contract.
#[async_trait]
pub trait ClipboardBackend: Send + Sync {
    fn id(&self) -> &str;

    /// Describe channels/operations supported by backend.
    fn descriptor(&self) -> ClipboardBackendDescriptor;

    /// Whether backend supports provided channel.
    fn supports_channel(&self, channel: ClipboardChannel) -> bool;

    /// Write content to target channel.
    async fn write(
        &self,
        content: &ClipboardContent,
        channel: ClipboardChannel,
    ) -> Result<(), ClipboardError>;

    /// Read content from target channel (if supported).
    async fn read(&self, channel: ClipboardChannel) -> Result<ClipboardContent, ClipboardError>;
}

/// Clipboard write request (application layer convenience type).
#[derive(Clone, Debug)]
pub struct ClipboardWriteRequest {
    pub content: ClipboardContent,
    pub channel: ClipboardChannel,
    pub preferred_backend: Option<String>,
}

impl ClipboardWriteRequest {
    pub fn new(content: ClipboardContent) -> Self {
        Self {
            content,
            channel: ClipboardChannel::Clipboard,
            preferred_backend: None,
        }
    }

    pub fn with_channel(mut self, channel: ClipboardChannel) -> Self {
        self.channel = channel;
        self
    }

    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.preferred_backend = Some(backend.into());
        self
    }
}

/// Clipboard read request.
#[derive(Clone, Debug)]
pub struct ClipboardReadRequest {
    pub channel: ClipboardChannel,
    pub preferred_backend: Option<String>,
    pub expected_mime: Option<ClipboardMime>,
}

impl ClipboardReadRequest {
    pub fn new(channel: ClipboardChannel) -> Self {
        Self {
            channel,
            preferred_backend: None,
            expected_mime: None,
        }
    }

    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.preferred_backend = Some(backend.into());
        self
    }

    #[allow(dead_code)]
    pub fn with_expected_mime(mut self, mime: ClipboardMime) -> Self {
        self.expected_mime = Some(mime);
        self
    }
}

/// Result of successful clipboard write operation.
#[derive(Clone, Debug)]
pub struct ClipboardWriteResult {
    pub backend_id: String,
    pub bytes: usize,
}

/// Result of clipboard read.
#[derive(Clone, Debug)]
pub struct ClipboardReadResult {
    pub backend_id: String,
    pub content: ClipboardContent,
}

/// Aggregated failure context for multi-backend attempts.
#[derive(Clone, Debug)]
pub struct ClipboardFailureTrace {
    pub backend_id: String,
    pub reason: String,
}

/// Outcome of multi-backend attempt.
#[derive(Debug, Error)]
pub enum ClipboardServiceError {
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
    #[error("all clipboard backends failed: {0:?}")]
    ExhaustedBackends(Vec<ClipboardFailureTrace>),
}

impl ClipboardServiceError {
    pub fn exhausted(failures: Vec<ClipboardFailureTrace>) -> Self {
        ClipboardServiceError::ExhaustedBackends(failures)
    }
}
