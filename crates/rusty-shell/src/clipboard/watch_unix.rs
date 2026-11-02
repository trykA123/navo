use super::{ClipboardError, ShellCapture};

/// Clipboard watching needs Windows; `start` always fails here.
pub struct Watcher;

impl Watcher {
    pub fn start(_on_change: Box<dyn Fn(ShellCapture) + Send>) -> Result<Self, ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
}
