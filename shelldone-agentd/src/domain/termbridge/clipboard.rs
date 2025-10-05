use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Logical clipboard channels supported by TermBridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardChannel {
    Clipboard,
    Primary,
}

impl ClipboardChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClipboardChannel::Clipboard => "clipboard",
            ClipboardChannel::Primary => "primary",
        }
    }
}

impl fmt::Display for ClipboardChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ClipboardChannel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "clipboard" | "clip" | "cb" => Ok(ClipboardChannel::Clipboard),
            "primary" | "selection" | "sel" => Ok(ClipboardChannel::Primary),
            other => Err(format!("unknown clipboard channel: {other}")),
        }
    }
}

/// Clipboard MIME type descriptor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardMime(String);

impl ClipboardMime {
    pub fn text_plain_utf8() -> Self {
        Self("text/plain; charset=utf-8".to_string())
    }

    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("clipboard mime must be non-empty".into());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Clipboard payload (immutable).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardContent {
    bytes: Vec<u8>,
    mime: ClipboardMime,
}

impl ClipboardContent {
    pub fn new(bytes: Vec<u8>, mime: ClipboardMime) -> Result<Self, String> {
        Ok(Self { bytes, mime })
    }

    #[allow(dead_code)]
    pub fn from_text(text: impl Into<String>) -> Result<Self, String> {
        let text = text.into();
        Ok(Self {
            bytes: text.into_bytes(),
            mime: ClipboardMime::text_plain_utf8(),
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[allow(dead_code)]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn mime(&self) -> &ClipboardMime {
        &self.mime
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }
}

/// Descriptor of a clipboard backend exposed to clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardBackendDescriptor {
    pub id: String,
    pub channels: Vec<ClipboardChannel>,
    pub can_read: bool,
    pub can_write: bool,
    pub notes: Vec<String>,
}

impl ClipboardBackendDescriptor {
    pub fn new(
        id: impl Into<String>,
        channels: Vec<ClipboardChannel>,
        can_read: bool,
        can_write: bool,
        notes: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            channels,
            can_read,
            can_write,
            notes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_roundtrip() {
        assert_eq!(ClipboardChannel::Clipboard.as_str(), "clipboard");
        assert_eq!(
            "clipboard".parse::<ClipboardChannel>().unwrap(),
            ClipboardChannel::Clipboard
        );
        assert_eq!(
            "selection".parse::<ClipboardChannel>().unwrap(),
            ClipboardChannel::Primary
        );
        assert!("".parse::<ClipboardChannel>().is_err());
    }

    #[test]
    fn mime_validation() {
        assert!(ClipboardMime::new("text/plain").is_ok());
        assert!(ClipboardMime::new(" ").is_err());
    }

    #[test]
    fn content_from_text() {
        let content = ClipboardContent::from_text("hello").unwrap();
        assert_eq!(content.bytes(), b"hello");
        assert_eq!(content.mime().as_str(), "text/plain; charset=utf-8");
    }
}
