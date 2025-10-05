use crate::domain::termbridge::{ClipboardBackendDescriptor, ClipboardChannel};
use crate::ports::termbridge::{
    ClipboardBackend, ClipboardError, ClipboardFailureTrace, ClipboardReadRequest,
    ClipboardReadResult, ClipboardServiceError, ClipboardWriteRequest, ClipboardWriteResult,
};
use crate::telemetry::PrismMetrics;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_MAX_BYTES: usize = 256 * 1024; // 256 KiB batched payload

pub struct ClipboardBridgeService {
    backends: Vec<Arc<dyn ClipboardBackend>>,
    metrics: Option<Arc<PrismMetrics>>,
    max_bytes: usize,
}

impl ClipboardBridgeService {
    pub fn new(
        backends: Vec<Arc<dyn ClipboardBackend>>,
        metrics: Option<Arc<PrismMetrics>>,
        max_bytes: Option<usize>,
    ) -> Self {
        let max_bytes = max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
        Self {
            backends,
            metrics,
            max_bytes,
        }
    }

    pub fn list_backends(&self) -> Vec<ClipboardBackendDescriptor> {
        self.backends
            .iter()
            .map(|backend| backend.descriptor())
            .collect()
    }

    pub async fn write(
        &self,
        request: ClipboardWriteRequest,
    ) -> Result<ClipboardWriteResult, ClipboardServiceError> {
        if self.backends.is_empty() {
            return Err(ClipboardServiceError::from(ClipboardError::NoBackends));
        }

        if request.content.len() > self.max_bytes {
            return Err(ClipboardServiceError::from(
                ClipboardError::PayloadTooLarge {
                    size: request.content.len(),
                    limit: self.max_bytes,
                },
            ));
        }

        let mut failures = Vec::new();
        let channel = request.channel;
        let candidates = self.select_candidates(request.preferred_backend.as_deref(), channel);
        for backend in candidates {
            let backend_id = backend.id().to_string();
            let started = Instant::now();
            match backend.write(&request.content, channel).await {
                Ok(()) => {
                    self.record_clipboard_metrics(
                        "write",
                        &backend_id,
                        request.content.len(),
                        started.elapsed().as_secs_f64() * 1000.0,
                        "success",
                    );
                    return Ok(ClipboardWriteResult {
                        backend_id,
                        bytes: request.content.len(),
                    });
                }
                Err(err) => {
                    self.record_clipboard_metrics(
                        "write",
                        &backend_id,
                        request.content.len(),
                        started.elapsed().as_secs_f64() * 1000.0,
                        "error",
                    );
                    failures.push(ClipboardFailureTrace {
                        backend_id,
                        reason: err.to_string(),
                    });
                    continue;
                }
            }
        }

        if failures.is_empty() {
            Err(ClipboardServiceError::from(ClipboardError::NoBackends))
        } else {
            Err(ClipboardServiceError::exhausted(failures))
        }
    }

    pub async fn read(
        &self,
        request: ClipboardReadRequest,
    ) -> Result<ClipboardReadResult, ClipboardServiceError> {
        if self.backends.is_empty() {
            return Err(ClipboardServiceError::from(ClipboardError::NoBackends));
        }

        let mut failures = Vec::new();
        let channel = request.channel;
        let candidates = self.select_candidates(request.preferred_backend.as_deref(), channel);
        for backend in candidates {
            let backend_id = backend.id().to_string();
            let started = Instant::now();
            match backend.read(channel).await {
                Ok(content) => {
                    if let Some(expected_mime) = request.expected_mime.as_ref() {
                        if content.mime() != expected_mime {
                            failures.push(ClipboardFailureTrace {
                                backend_id: backend_id.clone(),
                                reason: format!(
                                    "unexpected mime {}; expected {}",
                                    content.mime().as_str(),
                                    expected_mime.as_str()
                                ),
                            });
                            continue;
                        }
                    }
                    self.record_clipboard_metrics(
                        "read",
                        &backend_id,
                        content.len(),
                        started.elapsed().as_secs_f64() * 1000.0,
                        "success",
                    );
                    return Ok(ClipboardReadResult {
                        backend_id,
                        content,
                    });
                }
                Err(err) => {
                    self.record_clipboard_metrics(
                        "read",
                        &backend_id,
                        0,
                        started.elapsed().as_secs_f64() * 1000.0,
                        "error",
                    );
                    failures.push(ClipboardFailureTrace {
                        backend_id,
                        reason: err.to_string(),
                    });
                    continue;
                }
            }
        }

        Err(ClipboardServiceError::exhausted(failures))
    }

    fn select_candidates(
        &self,
        preferred: Option<&str>,
        channel: ClipboardChannel,
    ) -> Vec<Arc<dyn ClipboardBackend>> {
        let mut result = Vec::new();
        let mut seen = HashMap::new();
        if let Some(pref) = preferred {
            for backend in &self.backends {
                if backend.supports_channel(channel) && backend.id() == pref {
                    result.push(backend.clone());
                    seen.insert(backend.id().to_string(), true);
                    break;
                }
            }
        }
        for backend in &self.backends {
            if backend.supports_channel(channel) && !seen.contains_key(backend.id()) {
                result.push(backend.clone());
                seen.insert(backend.id().to_string(), true);
            }
        }
        result
    }

    fn record_clipboard_metrics(
        &self,
        action: &str,
        backend_id: &str,
        bytes: usize,
        latency_ms: f64,
        outcome: &str,
    ) {
        if let Some(metrics) = &self.metrics {
            metrics.record_termbridge_clipboard(
                action,
                backend_id,
                bytes as u64,
                latency_ms,
                outcome,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::termbridge::{ClipboardBackendDescriptor, ClipboardContent, ClipboardMime};
    use crate::ports::termbridge::{ClipboardBackend, ClipboardError};
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockBackend {
        id: &'static str,
        channels: Vec<ClipboardChannel>,
        content_log: Mutex<Vec<Vec<u8>>>,
        fail_write: bool,
        fail_read: bool,
    }

    impl MockBackend {
        fn new(id: &'static str, channels: Vec<ClipboardChannel>) -> Self {
            Self {
                id,
                channels,
                content_log: Mutex::new(Vec::new()),
                fail_write: false,
                fail_read: false,
            }
        }

        fn with_fail_write(mut self) -> Self {
            self.fail_write = true;
            self
        }

        fn with_fail_read(mut self) -> Self {
            self.fail_read = true;
            self
        }
    }

    #[async_trait]
    impl ClipboardBackend for MockBackend {
        fn id(&self) -> &str {
            self.id
        }

        fn descriptor(&self) -> ClipboardBackendDescriptor {
            ClipboardBackendDescriptor::new(
                self.id,
                self.channels.clone(),
                !self.fail_read,
                !self.fail_write,
                Vec::new(),
            )
        }

        fn supports_channel(&self, channel: ClipboardChannel) -> bool {
            self.channels.contains(&channel)
        }

        async fn write(
            &self,
            content: &ClipboardContent,
            _channel: ClipboardChannel,
        ) -> Result<(), ClipboardError> {
            if self.fail_write {
                return Err(ClipboardError::backend_failure(self.id, "write failed"));
            }
            self.content_log
                .lock()
                .unwrap()
                .push(content.bytes().to_vec());
            Ok(())
        }

        async fn read(
            &self,
            _channel: ClipboardChannel,
        ) -> Result<ClipboardContent, ClipboardError> {
            if self.fail_read {
                return Err(ClipboardError::backend_failure(self.id, "read failed"));
            }
            ClipboardContent::from_text("mock") // qa:allow-realness
                .map_err(|e| ClipboardError::backend_failure(self.id, e))
        }
    }

    #[tokio::test]
    async fn write_uses_preferred_backend() {
        let backend_a = Arc::new(MockBackend::new("a", vec![ClipboardChannel::Clipboard]));
        let backend_b = Arc::new(MockBackend::new("b", vec![ClipboardChannel::Clipboard]));
        let service =
            ClipboardBridgeService::new(vec![backend_a.clone(), backend_b.clone()], None, None);
        let request = ClipboardWriteRequest::new(ClipboardContent::from_text("data").unwrap())
            .with_backend("b");
        let result = service.write(request).await.unwrap();
        assert_eq!(result.backend_id, "b");
        assert_eq!(backend_b.content_log.lock().unwrap().len(), 1);
        assert_eq!(backend_a.content_log.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn write_falls_back_on_failure() {
        let backend_a =
            Arc::new(MockBackend::new("a", vec![ClipboardChannel::Clipboard]).with_fail_write());
        let backend_b = Arc::new(MockBackend::new("b", vec![ClipboardChannel::Clipboard]));
        let service =
            ClipboardBridgeService::new(vec![backend_a.clone(), backend_b.clone()], None, None);
        let request = ClipboardWriteRequest::new(ClipboardContent::from_text("data").unwrap());
        let result = service.write(request).await.unwrap();
        assert_eq!(result.backend_id, "b");
        assert_eq!(backend_b.content_log.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn write_respects_size_limit() {
        let backend = Arc::new(MockBackend::new("a", vec![ClipboardChannel::Clipboard]));
        let service = ClipboardBridgeService::new(vec![backend], None, Some(4));
        let content = ClipboardContent::from_text("12345").unwrap();
        let request = ClipboardWriteRequest::new(content);
        let err = service.write(request).await.unwrap_err();
        assert!(matches!(
            err,
            ClipboardServiceError::Clipboard(ClipboardError::PayloadTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn read_prefers_available_backend() {
        let backend_a =
            Arc::new(MockBackend::new("a", vec![ClipboardChannel::Clipboard]).with_fail_read());
        let backend_b = Arc::new(MockBackend::new("b", vec![ClipboardChannel::Clipboard]));
        let service = ClipboardBridgeService::new(vec![backend_a, backend_b.clone()], None, None);
        let request = ClipboardReadRequest::new(ClipboardChannel::Clipboard);
        let result = service.read(request).await.unwrap();
        assert_eq!(result.backend_id, "b");
        assert_eq!(result.content.bytes(), b"mock"); // qa:allow-realness
    }

    #[tokio::test]
    async fn read_respects_expected_mime() {
        let backend = Arc::new(MockBackend::new("a", vec![ClipboardChannel::Clipboard]));
        let service = ClipboardBridgeService::new(vec![backend], None, None);
        let request = ClipboardReadRequest::new(ClipboardChannel::Clipboard)
            .with_expected_mime(ClipboardMime::new("text/html").unwrap());
        let err = service.read(request).await.unwrap_err();
        assert!(matches!(err, ClipboardServiceError::ExhaustedBackends(_)));
    }
}
