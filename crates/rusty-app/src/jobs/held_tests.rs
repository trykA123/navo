use std::sync::{Arc, Mutex};

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, ProviderPath, plan};
use rusty_fs::{
    Attributes, CancelToken, EntryKind, FakeFs, FsEntry, HELD_PREFIX, JobContext, JobOutcome,
    Provider, existing_at, run_job, scan,
};
use rusty_store::{Db, JobJournal, JobState};
use rusty_ui::browser::OpLog;

use super::{SharedJournal, lock, log_factory, recover};

const TARGET: &str = "dest/docs/report.txt";

pub(super) fn journal() -> SharedJournal {
    let db = Db::open_in_memory().unwrap();
    Arc::new(Mutex::new(JobJournal::new(db).unwrap()))
}

pub(super) fn old_report() -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text("report.txt"),
        kind: EntryKind::File,
        size: 2,
        modified_ms: Some(5),
        attributes: Attributes {
            readonly: true,
            ..Attributes::default()
        },
    }
}

pub(super) fn source_report(fs: &FakeFs, size: u64) {
    add_timed(fs, "home/docs", "report.txt", size);
}

pub(super) fn fake() -> FakeFs {
    let fs = FakeFs::new();
    source_report(&fs, 7);
    fs.add_folder("dest/docs");
    fs.add_entry("dest/docs", old_report());
    fs
}

pub(super) fn overwrite_plan(fs: &FakeFs) -> OperationPlan {
    let cancel = CancelToken::new();
    let items = scan(fs, &[fs.location("home/docs")], &cancel).unwrap();
    let target = fs.location("dest");
    let existing = existing_at(fs, &items, &target, &cancel).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(target),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: Vec::new(),
    })
}

pub(super) fn run(fs: &FakeFs, journal: &SharedJournal, finish: bool) {
    let plan = overwrite_plan(fs);
    let mut log = log_factory(Arc::clone(journal))(&plan).unwrap();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    if finish {
        log.finish(&outcome, fs).unwrap();
    }
}

pub(super) fn target(fs: &FakeFs) -> Option<FsEntry> {
    fs.metadata(&fs.location(TARGET)).ok()
}

pub(super) fn holding(fs: &FakeFs) -> Vec<String> {
    let mut found = Vec::new();
    let mut sink = |batch: rusty_fs::EntryBatch| {
        found.extend(batch.entries.into_iter().map(|e| e.name.to_text()));
        std::ops::ControlFlow::Continue(())
    };
    let _ = fs.list(&fs.location("dest/docs"), &mut sink, &CancelToken::new());
    found.retain(|name| name.starts_with(HELD_PREFIX));
    found
}

fn rows_placed(journal: &SharedJournal, placed: bool) -> usize {
    let journal = lock(journal);
    (1..=4)
        .flat_map(|id| journal.held(rusty_store::JobId(id)).unwrap())
        .filter(|row| {
            let is_placed = serde_json::from_slice::<serde_json::Value>(row)
                .is_ok_and(|value| value.get("placed_target").is_some());
            is_placed == placed
        })
        .count()
}

pub(super) fn held_rows(journal: &SharedJournal) -> usize {
    rows_placed(journal, false)
}

pub(super) fn placed_rows(journal: &SharedJournal) -> usize {
    rows_placed(journal, true)
}

pub(super) fn add_timed(fs: &FakeFs, folder: &str, name: &str, size: u64) {
    fs.add_entry(
        folder,
        FsEntry {
            name: ProviderPath::from_text(name),
            kind: EntryKind::File,
            size,
            modified_ms: Some(9_000),
            attributes: Attributes::default(),
        },
    );
}

#[test]
fn an_overwrite_holds_the_old_file_while_the_job_runs() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            holding(&fs).len(),
            held_rows(&journal)
        ),
        (Some(7), 1, 1)
    );
}

#[test]
fn an_overwrite_cut_short_gets_the_old_file_back_at_start() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert_eq!(
        (target(&fs), holding(&fs), held_rows(&journal)),
        (Some(old_report()), Vec::new(), 0)
    );
}

#[test]
fn a_finished_overwrite_releases_the_held_file() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, true);
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            holding(&fs),
            held_rows(&journal)
        ),
        (Some(7), Vec::new(), 0)
    );
}

fn finished_without_release(fs: &FakeFs, journal: &SharedJournal) {
    run(fs, journal, false);
    let job = lock(journal).unfinished().unwrap()[0].id;
    lock(journal).finish(job, JobState::Done).unwrap();
}

#[test]
fn held_files_of_a_finished_job_are_released_at_start() {
    let fs = fake();
    let journal = journal();
    finished_without_release(&fs, &journal);
    assert_eq!(recover(&journal, &fs).unwrap(), 0);
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            holding(&fs),
            held_rows(&journal)
        ),
        (Some(7), Vec::new(), 0)
    );
}

#[test]
fn a_held_file_whose_target_is_gone_is_put_back_at_start() {
    let fs = fake();
    let journal = journal();
    finished_without_release(&fs, &journal);
    fs.remove(TARGET);
    assert_eq!(recover(&journal, &fs).unwrap(), 0);
    assert_eq!(
        (target(&fs), holding(&fs), held_rows(&journal)),
        (Some(old_report()), Vec::new(), 0)
    );
}

#[test]
fn held_files_that_cannot_be_released_stay_journaled() {
    let fs = fake();
    let journal = journal();
    finished_without_release(&fs, &journal);
    let folder = holding(&fs).remove(0);
    let held = format!("dest/docs/{folder}/report.txt");
    fs.fault(
        &held,
        rusty_fs::FakeOp::Remove,
        1,
        rusty_fs::Fault::Fail(rusty_core::FsErrorKind::Locked),
    );
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((holding(&fs).len(), held_rows(&journal)), (1, 1));
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!((holding(&fs), held_rows(&journal)), (Vec::new(), 0));
}

pub(super) fn held_path(fs: &FakeFs) -> String {
    format!("dest/docs/{}/report.txt", holding(fs)[0])
}

#[test]
fn a_failed_restore_at_start_keeps_the_held_file_for_the_next_start() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    let sharing = rusty_fs::Fault::Fail(rusty_core::FsErrorKind::SharingViolation);
    for call in [1, 2] {
        fs.fault(
            &held_path(&fs),
            rusty_fs::FakeOp::Replace,
            call,
            sharing.clone(),
        );
    }
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            holding(&fs).len(),
            held_rows(&journal)
        ),
        (Some(7), 1, 1)
    );
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (target(&fs), holding(&fs), held_rows(&journal)),
        (Some(old_report()), Vec::new(), 0)
    );
}

#[test]
fn a_missing_target_volume_at_start_keeps_the_held_rows() {
    let fs = fake();
    let journal = journal();
    finished_without_release(&fs, &journal);
    fs.remove("dest");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(held_rows(&journal), 1);
}

#[test]
fn two_crashed_jobs_over_one_target_roll_back_newest_first() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    source_report(&fs, 9);
    run(&fs, &journal, false);
    assert_eq!(recover(&journal, &fs).unwrap(), 2);
    assert_eq!(
        (target(&fs), holding(&fs), held_rows(&journal)),
        (Some(old_report()), Vec::new(), 0)
    );
}

const NO_FINISH: &str = "CREATE TRIGGER no_finish BEFORE UPDATE OF state ON job
     BEGIN SELECT RAISE(ABORT, 'finish refused'); END;";

#[test]
fn a_failed_finish_write_keeps_the_held_file_for_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    let fs = fake();
    let journal = super::open(&file).unwrap();
    let (guard, _) = Db::open(&file, false).unwrap();
    guard.connection().execute_batch(NO_FINISH).unwrap();
    run(&fs, &journal, false);
    let job = lock(&journal).unfinished().unwrap()[0].id;
    let mut log = super::JournalLog::new(journal.clone(), job, "copy".to_owned());
    assert!(log.finish(&JobOutcome::Done, &fs).is_ok());
    assert_eq!((holding(&fs).len(), held_rows(&journal)), (1, 1));
    log.experimental_retention = true;
    assert!(log.finish(&JobOutcome::Done, &fs).is_err());
    assert_eq!((holding(&fs).len(), held_rows(&journal)), (1, 1));
    guard
        .connection()
        .execute_batch("DROP TRIGGER no_finish;")
        .unwrap();
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert_eq!(
        (target(&fs), holding(&fs), held_rows(&journal)),
        (Some(old_report()), Vec::new(), 0)
    );
}

const BESIDE: &str = "dest/docs/report (before copy).txt";

pub(super) fn crashed_with_a_failing_restore(fs: &FakeFs, journal: &SharedJournal) {
    run(fs, journal, false);
    let sharing = rusty_fs::Fault::Fail(rusty_core::FsErrorKind::SharingViolation);
    for call in [1, 2] {
        fs.fault(
            &held_path(fs),
            rusty_fs::FakeOp::Replace,
            call,
            sharing.clone(),
        );
    }
    let _ = recover(journal, fs).unwrap();
    assert_eq!(held_rows(journal), 1);
}

pub(super) fn beside(fs: &FakeFs) -> Option<FsEntry> {
    fs.metadata(&fs.location(BESIDE)).ok().map(|entry| FsEntry {
        name: old_report().name,
        ..entry
    })
}

#[test]
fn a_retried_restore_keeps_what_the_user_saved_since() {
    let fs = fake();
    let journal = journal();
    crashed_with_a_failing_restore(&fs, &journal);
    fs.remove(TARGET);
    fs.add_file(TARGET, 11);
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            beside(&fs),
            holding(&fs),
            held_rows(&journal)
        ),
        (Some(11), Some(old_report()), Vec::new(), 0)
    );
}

#[test]
fn a_retried_restore_keeps_a_later_jobs_file() {
    let fs = fake();
    let journal = journal();
    crashed_with_a_failing_restore(&fs, &journal);
    source_report(&fs, 9);
    run(&fs, &journal, true);
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            beside(&fs),
            held_rows(&journal)
        ),
        (Some(9), Some(old_report()), 0)
    );
}

#[test]
fn a_missing_source_volume_keeps_the_job_waiting_for_its_restore() {
    let fs = fake();
    let journal = journal();
    run(&fs, &journal, false);
    fs.remove("home");
    let _ = recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            holding(&fs).len(),
            held_rows(&journal)
        ),
        (Some(7), 1, 1)
    );
}
