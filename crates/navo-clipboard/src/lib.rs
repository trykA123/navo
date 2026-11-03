//! Clipboard history model and store. Bodies are never logged.

mod admit;
mod limits;
mod model;
mod note_search;
mod scope;
mod search;
mod service;
mod store;

pub use admit::{Reason, admit, content_key};
pub use limits::{
    Limits, MAX_ENTRIES, MAX_HISTORY_TEXT_BYTES, MAX_PATHS, MAX_SAVED_BYTES, MAX_SAVED_ROWS,
    MAX_TEXT_BYTES, MAX_TITLE_BYTES,
};
pub use model::{Captured, Content, Entry, NoteRow, Saved, SavedKind, Scope, Sealed, Transfer};
pub use note_search::{NoteSummary, search_notes};
pub use scope::{folder_key, folder_name, same_folder};
pub use search::{Field, Found, Item, Mode, Rows, join_paths, search};
pub use service::{
    ClipService, ClipSink, Clock, FailKind, How, NoSealer, NoteHeader, Ports, Reply,
    SECRET_CLEAR_AFTER, SealError, Sealer, SecretBytes, Sequence, SinkError, Snapshot, SystemClock,
};
pub use store::{ClipError, ClipStore, Listed, NoteListed, Opened, Stored};
