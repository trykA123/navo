use std::sync::{Arc, Mutex};

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, ProviderPath, plan};
use rusty_fs::{
    Attributes, CancelToken, EntryKind, FakeFs, FsEntry, HELD_PREFIX, JobContext, JobOutcome,
    Provider, existing_at, run_job, scan,
};
use rusty_store::{Db, JobJournal, JobState};

use super::{SharedJournal, lock, log_factory, recover};

const TARGET: &str = "dest/docs/report.txt";

fn journal() -> SharedJournal {
    let db = Db::open_in_memory().unwrap();
    Arc::new(Mutex::new(JobJournal::new(db).unwrap()))
}

fn old_report() -> FsEntry {
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

fn fake() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("home/docs/report.txt", 7);
    fs.add_folder("dest/docs");
    fs.add_entry("dest/docs", old_report());
    fs
}

fn overwrite_plan(fs: &FakeFs) -> OperationPlan {
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

fn run(fs: &FakeFs, journal: &SharedJournal, finish: bool) {
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
        log.finish(&outcome, fs);
    }
}

fn target(fs: &FakeFs) -> Option<FsEntry> {
    fs.metadata(&fs.location(TARGET)).ok()
}

fn holding(fs: &FakeFs) -> Vec<String> {
    let mut found = Vec::new();
    let mut sink = |batch: rusty_fs::EntryBatch| {
        found.extend(batch.entries.into_iter().map(|e| e.name.to_text()));
        std::ops::ControlFlow::Continue(())
    };
    let _ = fs.list(&fs.location("dest/docs"), &mut sink, &CancelToken::new());
    found.retain(|name| name.starts_with(HELD_PREFIX));
    found
}

fn held_rows(journal: &SharedJournal) -> usize {
    let journal = lock(journal);
    (1..=4)
        .map(|id| journal.held(rusty_store::JobId(id)).unwrap().len())
        .sum()
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
