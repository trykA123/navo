#![forbid(unsafe_code)]

mod apply;
pub mod brand;
mod command;
mod direction;
pub mod encoding;
mod file_identity;
pub mod find;
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
pub mod rename;
mod replay;
pub mod retention;
mod selection;
mod session;
pub mod shell_menu;
mod state;

pub use apply::{
    CommandError, Effect, Event, apply, apply_event, apply_workspace, list_active_tabs,
    list_missing, list_unloaded, relist_tab,
};
pub use brand::TEMP_SUFFIX;
pub use command::{Command, CommandId, CursorMove, ParseError, ParseErrorKind};
pub use direction::Direction;
pub use file_identity::{FileIdentity, FileIdentityKind};
pub use fs_error::{FsError, FsErrorKind, LockHolder, RecoveryAction};
pub use id::{PaneId, TabId, WindowId};
pub use layout::{Layout, LayoutError, Rect};
pub use location::{Location, LocationError, ProviderId, ProviderPath, typed_location};
pub use name::{NameError, check_name};
pub use op_plan::{
    Conflict, ConflictPolicy, DropKeys, DropOp, ExistingItem, ItemKind, OpKind, OperationPlan,
    PlanRequest, Resolution, Risk, ScannedItem, Step, Totals, drop_op, is_protected, plan,
    right_button_drag,
};
pub use replay::{CommandLog, LogEntry, LogError, Record, Recorder};
pub use selection::{Pattern, PatternError, Selection, SpanMode};
pub use session::{
    EntryState, LayoutState, MAX_LAYOUT_DEPTH, PaneState, RowsState, SessionError, SessionState,
    SnapshotState, TabState, WindowState, WorkspaceState,
};
pub use state::{
    AppState, Entry, History, Listing, MAX_WORKSPACES, Pane, SAVE_ON_SWITCH, Snapshot, StateError,
    Tab, ViewState, Window, Workspace, WorkspaceAction, changes,
};
