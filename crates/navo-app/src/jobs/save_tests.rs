use std::sync::Arc;

use navo_core::{FileStamp, FsError, FsErrorKind, Location, OperationPlan, write_plan};
use navo_fs::{
    Attributes, CancelToken, EntryKind, FakeFs, FakeOp, Fault, FsEntry, HeldFile, JobContext,
    JobLog, JobOutcome, Provider, StepOutcome, run_job,
};
use navo_ui::browser::OpLog;

use navo_store::JobId;

use super::held::held_of;
use super::held_tests::journal;
use super::{SharedJournal, lock, log_factory, recover};

struct CancelAtHold<'a> {
    inner: Box<dyn OpLog>,
    fs: &'a FakeFs,
    cancel: CancelToken,
}

impl JobLog for CancelAtHold<'_> {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.inner.step(seq, outcome)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.inner.undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        let journaled = self.inner.journal_held(held)?;
        let back = Fault::Fail(FsErrorKind::SharingViolation);
        self.fs
            .fault(&held.held.path().to_text(), FakeOp::Rename, 1, back);
        self.cancel.cancel();
        Ok(journaled)
    }

    fn kept_beside(&mut self, held: &HeldFile, beside: &Location) {
        self.inner.kept_beside(held, beside);
    }

    fn held_stranded(&mut self, held: &HeldFile) {
        self.inner.held_stranded(held);
    }
}

fn file(name: &str, size: u64, modified: i64) -> FsEntry {
    FsEntry {
        name: navo_core::ProviderPath::from_text(name),
        kind: EntryKind::File,
        size,
        modified_ms: Some(modified),
        attributes: Attributes::default(),
    }
}

fn saving() -> (FakeFs, OperationPlan) {
    let fs = FakeFs::new();
    fs.add_folder("dest");
    fs.add_entry("dest", file("a.txt", 3, 1_000));
    fs.add_entry("dest", file(".a.txt.part", 11, 2_000));
    let seen = FileStamp {
        size: 3,
        modified_ms: Some(1_000),
        hash: [0; 32],
    };
    let plan = write_plan(
        &fs.location("dest/a.txt"),
        &fs.location("dest/.a.txt.part"),
        seen,
        11,
    );
    (fs, plan)
}

fn cancelled_at_the_hold<'a>(
    fs: &'a FakeFs,
    plan: &OperationPlan,
    journal: &SharedJournal,
) -> CancelAtHold<'a> {
    let mut log = CancelAtHold {
        inner: log_factory(Arc::clone(journal))(plan).unwrap(),
        fs,
        cancel: CancelToken::new(),
    };
    let cancel = log.cancel.clone();
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: &mut log,
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Cancelled);
    log
}

fn held_old(fs: &FakeFs, journal: &SharedJournal) -> Option<FsEntry> {
    let held = held_of(journal, JobId(1)).unwrap();
    let [file] = held.files.as_slice() else {
        return None;
    };
    fs.metadata(&file.held).ok()
}

#[test]
fn a_cancel_after_the_hold_with_a_failed_put_back_leaves_the_save_pending_restore() {
    let (fs, plan) = saving();
    let journal = journal();
    let mut log = cancelled_at_the_hold(&fs, &plan, &journal);
    log.inner.finish(&JobOutcome::Cancelled, &fs).unwrap();
    assert_eq!(lock(&journal).restore_pending().unwrap().len(), 1);
    assert!(fs.metadata(&fs.location("dest/a.txt")).is_err());
    assert_eq!(
        fs.metadata(&fs.location("dest/.a.txt.part")).unwrap().size,
        11
    );
}

#[test]
fn a_crash_before_placing_then_an_outside_file_at_the_target_keeps_the_old_file_at_start() {
    let (fs, plan) = saving();
    let journal = journal();
    drop(cancelled_at_the_hold(&fs, &plan, &journal));
    assert_eq!(held_old(&fs, &journal).map(|entry| entry.size), Some(3));
    fs.add_entry("dest", file("a.txt", 5, 7_000));
    recover(&journal, &fs).unwrap();
    assert_eq!(lock(&journal).restore_pending().unwrap().len(), 1);
    assert_eq!(held_old(&fs, &journal).map(|entry| entry.size), Some(3));
    assert_eq!(fs.metadata(&fs.location("dest/a.txt")).unwrap().size, 5);
}

#[test]
fn a_put_back_whose_flush_fails_leaves_the_save_pending_restore() {
    let (fs, plan) = saving();
    fs.fault(
        "dest/.a.txt.part",
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::SharingViolation),
    );
    fs.fault("dest", FakeOp::Flush, 2, Fault::Fail(FsErrorKind::Unknown));
    let journal = journal();
    let mut log = log_factory(Arc::clone(&journal))(&plan).unwrap();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: &fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert!(matches!(outcome, JobOutcome::Failed { .. }));
    log.finish(&outcome, &fs).unwrap();
    assert_eq!(lock(&journal).restore_pending().unwrap().len(), 1);
    assert_eq!(fs.metadata(&fs.location("dest/a.txt")).unwrap().size, 3);
    assert_eq!(lock(&journal).held(JobId(1)).unwrap().len(), 1);
}
