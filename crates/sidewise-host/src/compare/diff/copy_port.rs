//! The write port of the file diff glue (NC109): the trait a host implements to copy changes,
//! and the refusals a backend without a host sends (the read-only alpha).

use navo_ui::compare_diff::{
    CopyChange, CopyDone, CopyResult, CopySink, SheetPlanSink, SheetResult, SheetSink,
};

/// The host's write side for a file pair; an app without one refuses every copy (read-only).
pub trait CopyHost: Send + Sync {
    /// Copy one `change` and report its end through `done`, exactly once.
    fn copy_change(&self, change: CopyChange, done: CopySink);
    /// Copy `changes` of one pair as one job and report through `done`; nothing for none.
    fn copy_changes(&self, changes: Vec<CopyChange>, done: CopySink);
    /// Plan the sheet for `changes` and report its rows through `planned`, exactly once.
    fn plan_sheet(&self, changes: Vec<CopyChange>, planned: SheetPlanSink);
    /// Run the last planned sheet as one job and report through `done`, exactly once.
    fn run_sheet(&self, done: SheetSink);
}

/// The text a read-only backend shows for a refused write; the same as `CopyRefusal::NotCopyable`.
fn not_copyable() -> String {
    navo_l10n::tr("compare-copy-not-copyable")
}

/// Refuse `change` through `done` with the read-only text; nothing is written.
pub fn refuse_change(change: &CopyChange, done: &CopySink) {
    done(CopyDone {
        request: change.pair.id,
        direction: change.direction,
        result: CopyResult::Refused(not_copyable()),
    });
}

/// Refuse a sheet run through `done` with the read-only text; nothing is written.
pub fn refuse_sheet(done: SheetSink) {
    done(SheetResult::Refused(not_copyable()));
}
