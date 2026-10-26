use super::*;

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, Step, Totals, plan};
use rusty_fs::{
    Attributes, CancelToken, EntryKind, FakeOp, Fault, FsEntry, JobContext, JobOutcome, Provider,
    run_job, scan,
};

fn pending_copy(data: &Path, fs: &FakeFs) -> OperationPlan {
    fs.add_entry(
        "home",
        FsEntry {
            name: rusty_core::ProviderPath::from_text("a"),
            kind: EntryKind::File,
            size: 7,
            modified_ms: Some(9_000),
            attributes: Attributes::default(),
        },
    );
    fs.add_folder("dest");
    let plan = plan(&PlanRequest {
        kind: OpKind::Copy,
        items: scan(fs, &[fs.location("home/a")], &CancelToken::new()).unwrap(),
        target: Some(fs.location("dest")),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    });
    run_pending(data, fs, &plan);
    plan
}

fn run_pending(data: &Path, fs: &FakeFs, plan: &OperationPlan) {
    let journal = crate::jobs::open(&database(data)).unwrap();
    let mut log = crate::jobs::log_factory(journal.clone())(plan).unwrap();
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(crate::jobs::lock(&journal).unfinished().unwrap().len(), 1);
}

#[test]
fn automatic_startup_recovers_only_the_salvaged_verified_journal() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    pending_copy(data.path(), &fs);
    damaged(data.path());
    let start = check_with(Some(data.path()), true, &fs, true);
    assert!(fs.metadata(&fs.location("dest/a")).is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert_eq!(
        State::load(data.path()).unwrap().notices[0].recovered_jobs,
        1
    );
    assert!(start.notice.is_some());
}

#[test]
fn automatic_startup_leaves_user_files_alone_when_the_journal_is_damaged() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    pending_copy(data.path(), &fs);
    let path = database(data.path());
    let page = crate::crash::test_damage::root_page(&path, "job_step");
    crate::crash::test_damage::corrupt_page(&path, page);
    check_with(Some(data.path()), true, &fs, true);
    assert!(fs.metadata(&fs.location("dest/a")).is_ok());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices[0].unreadable_jobs, 1);
    assert_eq!(state.notices[0].recovered_jobs, 0);
    assert!(!state.notices[0].pending_paths.is_empty());
}

fn manual_rename(data: &Path, fs: &FakeFs) {
    fs.add_file("home/a", 7);
    let plan = OperationPlan {
        kind: OpKind::Rename,
        steps: vec![Step::Rename {
            source: fs.location("home/a"),
            target: fs.location("home/b"),
        }],
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: 7,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    };
    run_pending(data, fs, &plan);
    damaged(data);
    for call in [1, 2] {
        fs.fault(
            "home/b",
            FakeOp::Rename,
            call,
            Fault::Fail(rusty_core::FsErrorKind::Locked),
        );
    }
    let mut first = check_with(Some(data), true, fs, true);
    assert_eq!(State::load(data).unwrap().notices[0].unreadable_jobs, 1);
    first.share();
}

#[test]
fn a_later_successful_retry_refreshes_the_persistent_manual_recovery_notice() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    manual_rename(data.path(), &fs);
    check_with(Some(data.path()), false, &fs, true);
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices[0].unreadable_jobs, 0);
    assert_eq!(state.notices[0].recovered_jobs, 1);
    assert!(fs.metadata(&fs.location("home/b")).is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
}

#[test]
fn a_completed_retry_refreshes_its_notice_after_an_interrupted_refresh() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    manual_rename(data.path(), &fs);
    let (db, _) = Db::open(&database(data.path()), true).unwrap();
    let journal = std::sync::Arc::new(std::sync::Mutex::new(
        rusty_store::JobJournal::new(db).unwrap(),
    ));
    crate::jobs::recover_profile(&journal, &fs, (data.path(), &[]), false).unwrap();
    drop(journal);
    fs.add_file("home/b", 33);
    check_with(Some(data.path()), false, &fs, true);
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices[0].unreadable_jobs, 0);
    assert_eq!(state.notices[0].recovered_jobs, 1);
    assert_eq!(fs.metadata(&fs.location("home/b")).unwrap().size, 33);
}

#[test]
fn a_reused_job_identifier_cannot_resolve_an_older_manual_notice() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    manual_rename(data.path(), &fs);
    let state = State::load(data.path()).unwrap();
    let id = rusty_store::JobId(state.notices[0].retry_jobs[0].id);
    let (db, _) = Db::open(&database(data.path()), true).unwrap();
    let mut journal = rusty_store::JobJournal::new(db).unwrap();
    let (_, original) = journal.read_job(id).unwrap();
    journal.finish(id, rusty_store::JobState::Failed).unwrap();
    assert_eq!(journal.prune(0).unwrap(), 1);
    let new_id = journal
        .create_job(&original.kind, b"different plan", 0, original.created_at)
        .unwrap();
    assert_eq!(new_id, id);
    journal
        .finish(new_id, rusty_store::JobState::RolledBack)
        .unwrap();
    drop(journal);
    check_with(Some(data.path()), false, &fs, true);
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices[0].unreadable_jobs, 1);
    assert_eq!(state.notices[0].recovered_jobs, 0);
    assert!(fs.metadata(&fs.location("home/b")).is_ok());
}

#[test]
fn a_pruned_failed_job_keeps_its_manual_notice_without_blocking_startup() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    pending_copy(data.path(), &fs);
    damaged(data.path());
    fs.fault(
        "dest/a",
        FakeOp::Remove,
        1,
        Fault::Fail(rusty_core::FsErrorKind::Locked),
    );
    let mut first = check_with(Some(data.path()), true, &fs, true);
    first.share();
    drop(first);
    let (db, _) = Db::open(&database(data.path()), true).unwrap();
    let mut journal = rusty_store::JobJournal::new(db).unwrap();
    assert_eq!(journal.prune(0).unwrap(), 1);
    drop(journal);
    let second = check_with(Some(data.path()), false, &fs, true);
    assert!(super::super::super::open_db(&database(data.path())).is_ok());
    assert_eq!(
        second.notice.unwrap().content().title,
        rusty_l10n::tr("store-recovery-title")
    );
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices[0].unreadable_jobs, 1);
    assert_eq!(state.notices[0].recovered_jobs, 0);
    assert!(fs.metadata(&fs.location("dest/a")).is_ok());
}

#[test]
fn a_failed_copy_cleanup_is_reported_as_manual_instead_of_recovered() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    pending_copy(data.path(), &fs);
    damaged(data.path());
    fs.fault(
        "dest/a",
        FakeOp::Remove,
        1,
        Fault::Fail(rusty_core::FsErrorKind::Locked),
    );
    check_with(Some(data.path()), true, &fs, true);
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices[0].unreadable_jobs, 1);
    assert_eq!(state.notices[0].recovered_jobs, 0);
    assert!(fs.metadata(&fs.location("dest/a")).is_ok());
}

#[test]
fn recovery_resumes_after_sqlite_finished_but_before_the_notice_was_committed() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    pending_copy(data.path(), &fs);
    let mut state = built(data.path());
    let Some(Progress::Promote { notice, identity }) = state.pending.clone() else {
        panic!("built candidate");
    };
    promote::install(data.path(), &notice, &identity).unwrap();
    let (db, _) = Db::open(&database(data.path()), true).unwrap();
    let journal = std::sync::Arc::new(std::sync::Mutex::new(
        rusty_store::JobJournal::new(db).unwrap(),
    ));
    let attempted = crate::jobs::lock(&journal)
        .unfinished()
        .unwrap()
        .iter()
        .map(|job| job.id.0)
        .collect();
    state.pending = Some(Progress::Recover {
        notice,
        identity,
        attempted,
    });
    state.save(data.path()).unwrap();
    crate::jobs::recover_profile(&journal, &fs, (data.path(), &[]), false).unwrap();
    drop(journal);
    fs.add_file("dest/a", 33);
    check_with(Some(data.path()), false, &fs, true);
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices[0].recovered_jobs, 1);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 33);
    assert!(state.pending.is_none());
}
