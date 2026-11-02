//! Clipboard history model and admit rules. Bodies are never logged.

mod admit;
mod limits;
mod model;

pub use admit::{Reason, admit, content_key};
pub use limits::{
    Limits, MAX_ENTRIES, MAX_HISTORY_TEXT_BYTES, MAX_PATHS, MAX_SAVED_BYTES, MAX_TEXT_BYTES,
};
pub use model::{Captured, Content, Entry, Saved, SavedKind, Scope, Transfer};
