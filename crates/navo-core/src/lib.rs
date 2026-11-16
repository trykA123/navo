//! Navo's shared core types; the compare types are re-exported from `sidewise-core`.

#![forbid(unsafe_code)]

mod apply;
pub mod brand;
mod command;
mod direction;
pub mod find;
mod id;
mod layout;
pub mod line_ends;
pub mod list_text;
pub mod marks;
mod name;
pub mod placement;
pub mod rename;
mod replay;
pub mod retention;
mod selection;
mod session;
mod state;

pub use apply::{
    CommandError, Effect, Event, apply, apply_event, apply_workspace, list_active_tabs,
    list_missing, list_unloaded, relist_tab,
};
pub use brand::TEMP_SUFFIX;
pub use command::{Command, CommandId, ParseError, ParseErrorKind, TabCommand};
pub use direction::Direction;
pub use id::{PaneId, TabId, WindowId};
pub use layout::{
    Applied, Axis, Layout, LayoutError, MAX_PANES, Preset, Rect, SplitInfo, SplitPath, Tiles,
    TilesError,
};
pub use name::{NameError, check_name};
pub use replay::{CommandLog, LogEntry, LogError, Record, Recorder};
pub use selection::{Pattern, PatternError, Selection, SpanMode};
pub use session::{
    EntryState, LayoutState, MAX_LAYOUT_DEPTH, PaneState, RowsState, SessionError, SessionState,
    SnapshotState, TabState, WindowState, WorkspaceState,
};
pub use sidewise_core::{
    AncestorMemo, Conflict, ConflictPolicy, DropKeys, DropOp, ExistingItem, FileIdentity,
    FileIdentityKind, FileStamp, ItemKind, Location, LocationError, NETWORK_WALK_MAX_ENTRIES,
    NETWORK_WALK_MAX_TIME, NetworkStop, NetworkWalkBudget, OpKind, OperationPlan, PlanRequest,
    ProviderId, ProviderPath, Resolution, Risk, ScannedItem, Step, Totals, WindowsFoldKey,
    WriteManyError, WriteTarget, drop_op, is_protected, plan, plan_keeping, relative_folders,
    right_button_drag, typed_location, write_many_plan, write_plan,
};
pub use sidewise_core::{
    CursorMove, EntryKind, FsError, FsErrorKind, LockHolder, RecoveryAction, shell_menu,
};
pub use sidewise_core::{encoding, paging};
pub use state::{
    AppState, Entry, History, Listing, MAX_WORKSPACES, Pane, SAVE_ON_SWITCH, Snapshot, StateError,
    Tab, TabMove, ViewState, Window, Workspace, WorkspaceAction, changes,
};
