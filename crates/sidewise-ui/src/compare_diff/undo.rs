//! Types of the undo seam (NC148): how the app reports the end of an undo to the compare
//! screen. The hooks in `hooks.rs` move them between the UI and the app.

use std::sync::Arc;

use navo_core::Location;

use super::CopyResult;

/// The end of one undo: the files it touched and how it went. `Copied` means the copy was
/// undone; `Refused` carries the text for the status line and nothing was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoDone {
    pub targets: Vec<Location>,
    pub result: CopyResult,
}

/// Receives the end of an undo on any thread; it wakes the window.
pub type UndoSink = Arc<dyn Fn(UndoDone) + Send + Sync>;

/// How an undo request was taken. `Started`: the end comes through the sink (a running write
/// reports there too). `Nothing`: the stack is empty. `NotNewest`: the asked copy is not the
/// newest, so nothing was undone (NC166). `Gone`: the asked copy is no longer on the stack
/// (already undone, or dropped after a lasting failure), so nothing was undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoStart {
    Started,
    Nothing,
    NotNewest,
    Gone,
}
