#![forbid(unsafe_code)]

mod apply;
mod command;
mod direction;
mod fs_error;
mod id;
mod layout;
mod location;
mod replay;
mod selection;
mod session;
mod state;

pub use apply::{CommandError, Effect, Event, apply, apply_event, list_unloaded};
pub use command::{Command, CommandId, CursorMove, ParseError, ParseErrorKind};
pub use direction::Direction;
pub use fs_error::{FsError, FsErrorKind, RecoveryAction};
pub use id::{PaneId, TabId, WindowId};
pub use layout::{Layout, LayoutError, Rect};
pub use location::{Location, LocationError, ProviderId, ProviderPath};
pub use replay::{CommandLog, LogEntry, LogError, Record, Recorder};
pub use selection::{Pattern, PatternError, Selection};
pub use session::{
    EntryState, LayoutState, MAX_LAYOUT_DEPTH, PaneState, RowsState, SessionError, SessionState,
    TabState, WindowState,
};
pub use state::{AppState, Entry, History, Listing, Pane, StateError, Tab, ViewState, Window};
