//! Types of the plan sheet for "copy all matches" (V31, D31): the dry-run rows the app returns
//! and how the one job ended. The hooks in `hooks.rs` move them between the UI and the app.

use std::sync::Arc;

/// One file of a plan sheet: what it copies, or why it is left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetRow {
    /// The path shown to the user.
    pub path: String,
    /// How many distinct changes the file copies.
    pub hunks: usize,
    /// The source bytes those changes carry.
    pub bytes: u64,
    /// The Fluent id of the refusal, or `None` when the job writes the file.
    pub refusal: Option<&'static str>,
}

/// The dry run of a copy of many changes: one row per file, nothing written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SheetPlan {
    pub rows: Vec<SheetRow>,
}

/// How the one job ended. A refusal carries the text for the status line; nothing was written
/// unless the text says the job stopped halfway (Ctrl+Z undoes the files already written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetResult {
    Copied { files: usize, bytes: u64 },
    Refused(String),
}

/// Receives the plan of a sheet on any thread; it wakes the window.
pub type SheetPlanSink = Arc<dyn Fn(SheetPlan) + Send + Sync>;

/// Receives the end of the sheet's job on any thread; it wakes the window.
pub type SheetSink = Arc<dyn Fn(SheetResult) + Send + Sync>;
