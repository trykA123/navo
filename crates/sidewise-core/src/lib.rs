//! Core types shared by the compare stack: locations, paging, encoding and operation plans.

#![forbid(unsafe_code)]

pub mod components;
mod cursor_move;
pub mod encoding;
mod entry_kind;
mod file_identity;
mod fs_error;
pub mod lines;
mod location;
mod network_walk;
mod op_plan;
pub mod paging;
pub mod shell_menu;

pub use cursor_move::CursorMove;
pub use entry_kind::EntryKind;
pub use file_identity::{FileIdentity, FileIdentityKind};
pub use fs_error::{FsError, FsErrorKind, LockHolder, RecoveryAction};
pub use location::{
    AncestorMemo, Location, LocationError, ProviderId, ProviderPath, WindowsFoldKey, typed_location,
};
pub use network_walk::{
    NETWORK_WALK_MAX_ENTRIES, NETWORK_WALK_MAX_TIME, NetworkStop, NetworkWalkBudget,
};
pub use op_plan::{
    Conflict, ConflictPolicy, DropKeys, DropOp, ExistingItem, FileStamp, ItemKind, OpKind,
    OperationPlan, PlanRequest, Resolution, Risk, ScannedItem, Step, Totals, WriteManyError,
    WriteTarget, drop_op, is_protected, plan, plan_keeping, relative_folders, right_button_drag,
    write_many_plan, write_plan,
};

/// Suffix added to a file name while it is being written.
pub const TEMP_SUFFIX: &str = ".navo-part";
/// Largest number of nodes one operation plan may hold.
pub const MAX_PLAN_NODES: usize = 100_000;
