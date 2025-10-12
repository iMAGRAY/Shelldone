use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerminalId(String);

impl TerminalId {
    pub fn new(slug: impl Into<String>) -> Self {
        Self::try_new(slug).expect("terminal id cannot be empty")
    }

    pub fn try_new(slug: impl Into<String>) -> Result<Self, String> {
        let slug = slug.into();
        if slug.trim().is_empty() {
            return Err("terminal id cannot be empty".into());
        }
        Ok(Self(slug))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TerminalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TerminalId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.trim().is_empty() {
            Err("terminal id cannot be empty".into())
        } else {
            Ok(TerminalId::new(value))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerminalBindingId(Uuid);

impl TerminalBindingId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        let uuid = Uuid::parse_str(value).map_err(|err| err.to_string())?;
        Ok(Self(uuid))
    }
}

impl fmt::Display for TerminalBindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCapabilities {
    pub spawn: bool,
    pub split: bool,
    pub focus: bool,
    pub duplicate: bool,
    pub close: bool,
    pub send_text: bool,
    pub clipboard_write: bool,
    pub clipboard_read: bool,
    pub cwd_sync: bool,
    pub bracketed_paste: bool,
    pub max_clipboard_kb: Option<u32>,
}

impl TerminalCapabilities {
    pub fn builder() -> TerminalCapabilitiesBuilder {
        TerminalCapabilitiesBuilder::default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CurrentWorkingDirectory(String);

impl CurrentWorkingDirectory {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Self::try_new(value)
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let raw = value.into();
        if raw.chars().any(|c| matches!(c, '\0' | '\n' | '\r')) {
            return Err("cwd contains invalid control characters".into());
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("cwd cannot be empty".into());
        }
        if trimmed.len() > 4096 {
            return Err("cwd exceeds 4096 characters".into());
        }
        let normalized = if Path::new(trimmed).components().next().is_none() {
            return Err("cwd must reference a valid path".into());
        } else {
            trimmed.to_string()
        };
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CurrentWorkingDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<CurrentWorkingDirectory> for String {
    fn from(value: CurrentWorkingDirectory) -> Self {
        value.0
    }
}

impl<'a> From<&'a CurrentWorkingDirectory> for String {
    fn from(value: &'a CurrentWorkingDirectory) -> Self {
        value.0.clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct TerminalCapabilitiesBuilder {
    spawn: bool,
    split: bool,
    focus: bool,
    send_text: bool,
    duplicate: bool,
    close: bool,
    clipboard_write: bool,
    clipboard_read: bool,
    cwd_sync: bool,
    bracketed_paste: bool,
    max_clipboard_kb: Option<u32>,
}

impl TerminalCapabilitiesBuilder {
    pub fn spawn(mut self, value: bool) -> Self {
        self.spawn = value;
        self
    }

    pub fn split(mut self, value: bool) -> Self {
        self.split = value;
        self
    }

    pub fn focus(mut self, value: bool) -> Self {
        self.focus = value;
        self
    }

    pub fn send_text(mut self, value: bool) -> Self {
        self.send_text = value;
        self
    }

    pub fn duplicate(mut self, value: bool) -> Self {
        self.duplicate = value;
        self
    }

    pub fn close(mut self, value: bool) -> Self {
        self.close = value;
        self
    }

    pub fn clipboard_write(mut self, value: bool) -> Self {
        self.clipboard_write = value;
        self
    }

    pub fn clipboard_read(mut self, value: bool) -> Self {
        self.clipboard_read = value;
        self
    }

    pub fn cwd_sync(mut self, value: bool) -> Self {
        self.cwd_sync = value;
        self
    }

    pub fn bracketed_paste(mut self, value: bool) -> Self {
        self.bracketed_paste = value;
        self
    }

    pub fn max_clipboard_kb(mut self, value: Option<u32>) -> Self {
        self.max_clipboard_kb = value;
        self
    }

    pub fn build(self) -> TerminalCapabilities {
        TerminalCapabilities {
            spawn: self.spawn,
            split: self.split,
            focus: self.focus,
            send_text: self.send_text,
            duplicate: self.duplicate,
            close: self.close,
            clipboard_write: self.clipboard_write,
            clipboard_read: self.clipboard_read,
            cwd_sync: self.cwd_sync,
            bracketed_paste: self.bracketed_paste,
            max_clipboard_kb: self.max_clipboard_kb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn terminal_id_validation() {
        assert!(TerminalId::try_new("kitty").is_ok());
        assert!(TerminalId::try_new(" ").is_err());
    }

    #[test]
    fn terminal_binding_id_parse_roundtrip() {
        let id = TerminalBindingId::new();
        let parsed = TerminalBindingId::parse(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn capabilities_builder_sets_flags() {
        let caps = TerminalCapabilities::builder()
            .spawn(true)
            .send_text(true)
            .duplicate(true)
            .close(true)
            .clipboard_write(true)
            .max_clipboard_kb(Some(128))
            .build();
        assert!(caps.spawn);
        assert!(caps.send_text);
        assert!(caps.duplicate);
        assert!(caps.close);
        assert!(caps.clipboard_write);
        assert_eq!(caps.max_clipboard_kb, Some(128));
        assert!(!caps.split);
    }

    #[test]
    fn cwd_validation() {
        let valid = CurrentWorkingDirectory::new("/tmp").unwrap();
        assert_eq!(valid.as_str(), "/tmp");
        assert!(CurrentWorkingDirectory::new(" ").is_err());
        assert!(CurrentWorkingDirectory::new("path\ninvalid").is_err());
    }

    proptest! {
        #[test]
        fn rejects_control_characters(prefix in "[ -~]{0,16}", suffix in "[ -~]{0,16}", ctrl in prop::sample::select(vec!['\n', '\r', '\0'])) {
            let mut candidate = prefix.clone();
            candidate.push(ctrl);
            candidate.push_str(&suffix);
            prop_assert!(CurrentWorkingDirectory::new(candidate).is_err());
        }

        #[test]
        fn accepts_posix_like_paths(segments in prop::collection::vec("[a-zA-Z0-9._-]{1,6}", 1..5)) {
            let path = format!("/{}", segments.join("/"));
            prop_assert!(CurrentWorkingDirectory::new(path).is_ok());
        }
    }
}
