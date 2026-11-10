//! `DiffHooks::plan_sheet` and `run_sheet` for the app: the plan sheet and the one-job copy of
//! many matching changes (V31, D31). Each runs on its own thread; the last plan is kept for the run.

use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use navo_ui::compare_diff::{
    CopyChange, SheetPlan, SheetPlanSink, SheetResult, SheetRow, SheetSink,
};

use super::copy::{CopyRefusal, CopyRequest};
use super::copy_hook::{CopyService, guarded, request_of};
use super::copy_many::{Sheet, plan::plan_sheet, run_sheet};

/// A plan and the requests it was made from; `run_sheet` needs both.
struct Planned {
    sheet: Sheet,
    requests: Vec<CopyRequest>,
}

/// The last plan, shared by the threads that plan and run.
#[derive(Clone, Default)]
pub(super) struct SheetSlot(Arc<Mutex<Option<Planned>>>);

impl SheetSlot {
    /// Keeps `planned` in place of any older plan.
    fn put(&self, planned: Planned) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(planned);
    }

    /// Takes the plan, so one plan runs at most once.
    fn take(&self) -> Option<Planned> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// The UI's rows for the sheet's files.
fn rows_of(sheet: &Sheet) -> Vec<SheetRow> {
    sheet
        .files
        .iter()
        .map(|file| SheetRow {
            path: file.path.clone(),
            hunks: file.hunks,
            bytes: file.bytes,
            refusal: file.refusal,
        })
        .collect()
}

impl CopyService {
    /// Plan `changes` on a new thread and report the rows through `planned`. A thread that
    /// cannot start reports an empty plan, so the UI never waits for an answer that will not come.
    pub(super) fn plan_sheet(&self, changes: Vec<CopyChange>, planned: SheetPlanSink) {
        let service = self.clone();
        let report = planned.clone();
        let spawned = thread::Builder::new()
            .name("compare-sheet-plan".to_owned())
            .spawn(move || {
                guarded(planned, SheetPlan::default, || {
                    let requests: Vec<CopyRequest> = changes.iter().map(request_of).collect();
                    let sheet = service.with_context(|ctx| plan_sheet(&requests, ctx));
                    let rows = rows_of(&sheet);
                    service.sheet.put(Planned { sheet, requests });
                    SheetPlan { rows }
                });
            });
        if spawned.is_err() {
            report(SheetPlan::default());
        }
    }

    /// Run the last plan as one job on a new thread and report through `done`. A plan already
    /// run, or none, is refused.
    pub(super) fn run_sheet(&self, done: SheetSink) {
        let service = self.clone();
        let report = done.clone();
        let spawned = thread::Builder::new()
            .name("compare-sheet-run".to_owned())
            .spawn(move || {
                let stopped = || SheetResult::Refused(CopyRefusal::NotCopyable.text());
                guarded(done, stopped, || service.run_planned());
            });
        if let Err(error) = spawned {
            report(SheetResult::Refused(
                CopyRefusal::Failed(error.to_string()).text(),
            ));
        }
    }

    /// Run the kept plan now and turn the engine's answer into the UI's.
    fn run_planned(&self) -> SheetResult {
        let Some(planned) = self.sheet.take() else {
            return SheetResult::Refused(CopyRefusal::NotCopyable.text());
        };
        let ran = self.with_context(|ctx| run_sheet(&planned.sheet, &planned.requests, ctx));
        match ran {
            Ok(started) => SheetResult::Copied {
                files: started.files,
                bytes: started.bytes,
            },
            Err(refusal) => SheetResult::Refused(refusal.text()),
        }
    }
}
