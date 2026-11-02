//! Every size cap of the clipboard module. Change a cap here only.

pub const MAX_ENTRIES: usize = 500;
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
pub const MAX_HISTORY_TEXT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PATHS: usize = 10_000;
pub const MAX_SAVED_BYTES: usize = 256 * 1024;

/// Caps the admit rules and the store apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_text_bytes: usize,
    pub max_paths: usize,
    pub max_entries: usize,
    pub max_history_text_bytes: usize,
    pub max_saved_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_text_bytes: MAX_TEXT_BYTES,
            max_paths: MAX_PATHS,
            max_entries: MAX_ENTRIES,
            max_history_text_bytes: MAX_HISTORY_TEXT_BYTES,
            max_saved_bytes: MAX_SAVED_BYTES,
        }
    }
}
