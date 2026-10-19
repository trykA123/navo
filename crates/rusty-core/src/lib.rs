#![forbid(unsafe_code)]

mod apply;
mod command;
mod direction;
pub mod encoding;
mod fs_error;
mod id;
mod layout;
pub mod line_ends;
mod location;
pub mod marks;
mod name;
mod op_plan;
pub mod paging;
pub mod placement;
mod replay;
mod selection;
mod session;
pub mod shell_menu;
mod state;

pub use apply::{
    CommandError, Effect, Event, apply, apply_event, list_active_tabs, list_missing, list_unloaded,
    relist_tab,
};
pub use command::{Command, CommandId, CursorMove, ParseError, ParseErrorKind};
pub use direction::Direction;
pub use fs_error::{FsError, FsErrorKind, LockHolder, RecoveryAction};
pub use id::{PaneId, TabId, WindowId};
pub use layout::{Layout, LayoutError, Rect};
pub use location::{Location, LocationError, ProviderId, ProviderPath, typed_location};
pub use name::{NameError, check_name};
pub use op_plan::{
    Conflict, ConflictPolicy, ExistingItem, ItemKind, OpKind, OperationPlan, PlanRequest,
    Resolution, Risk, ScannedItem, Step, TEMP_SUFFIX, Totals, is_protected, plan,
};
pub use replay::{CommandLog, LogEntry, LogError, Record, Recorder};
pub use selection::{Pattern, PatternError, Selection};
pub use session::{
    EntryState, LayoutState, MAX_LAYOUT_DEPTH, PaneState, RowsState, SessionError, SessionState,
    TabState, WindowState,
};
pub use state::{AppState, Entry, History, Listing, Pane, StateError, Tab, ViewState, Window};
