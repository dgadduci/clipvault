//! Clipboard trait and a deterministic fake used by core tests.
//!
//! No real platform clipboard implementation lives here yet. That will
//! arrive with the `desktop-platform-integration` change.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("clipboard read returned no text")]
    Empty,
    #[error("clipboard backend failed: {0}")]
    Backend(String),
}

/// Minimal trait describing what ClipVault needs from a clipboard backend.
pub trait Clipboard: Send + Sync {
    fn read_text(&self) -> Result<Option<String>, ClipboardError>;
}

/// In-memory clipboard used by tests and by the bootstrap before the real
/// platform adapter is wired in.
#[derive(Debug, Default)]
pub struct FakeClipboard {
    inner: parking_lot::Mutex<Option<String>>,
}

impl FakeClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            inner: parking_lot::Mutex::new(Some(text.into())),
        }
    }

    pub fn set_text(&self, text: impl Into<String>) {
        *self.inner.lock() = Some(text.into());
    }

    pub fn clear(&self) {
        *self.inner.lock() = None;
    }
}

impl Clipboard for FakeClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        Ok(self.inner.lock().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clipboard_round_trip() {
        let cb = FakeClipboard::with_text("hello");
        assert_eq!(cb.read_text().unwrap().as_deref(), Some("hello"));
        cb.set_text("world");
        assert_eq!(cb.read_text().unwrap().as_deref(), Some("world"));
        cb.clear();
        assert_eq!(cb.read_text().unwrap(), None);
    }
}
