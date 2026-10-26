use rusty_core::retention::{RetainedState, RetentionEntry};
use rusty_core::{ConflictPolicy, Location, OpKind, OperationPlan, PlanRequest, plan};
use rusty_fs::{
    CancelToken, FakeFs, JobContext, JobOutcome, LocalFs, Provider, ProviderOps, existing_at,
    run_job_parallel, scan,
};
use rusty_retention::{RestoreRequest, RetentionRun, Vault};
use rusty_store::{Db, JobState, RetentionCatalog, StoreDirs};
use rusty_ui::browser::{LoggedOperation, OpLog};

use super::super::{
    SharedJournal, experimental_factory as profile_factory, lock, open, recover_profile,
};

fn setup() -> (tempfile::TempDir, StoreDirs, SharedJournal) {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    dirs.ensure().unwrap();
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-overwrites = true\n",
    )
    .unwrap();
    let journal = open(&dirs.journal_file()).unwrap();
    (root, dirs, journal)
}

fn overwrite(
    ops: &dyn ProviderOps,
    sources: Vec<Location>,
    target: Location,
    kind: OpKind,
) -> OperationPlan {
    let items = scan(ops, &sources, &CancelToken::new()).unwrap();
    let existing = existing_at(ops, &items, &target, &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind,
        items,
        target: Some(target),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: vec![],
    })
}

fn execute(log: &mut dyn OpLog, plan: &OperationPlan, ops: &dyn ProviderOps) -> JobOutcome {
    log.check(LoggedOperation {
        plan,
        ops,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    run_job_parallel(
        plan,
        &[],
        &mut JobContext {
            ops,
            log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        2,
    )
}

fn entries(dirs: &StoreDirs) -> Vec<RetentionEntry> {
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    RetentionCatalog::new(db)
        .unwrap()
        .list_after(0, 100)
        .unwrap()
}

#[test]
fn copy_and_move_preserve_old_versions_after_completion_and_restart() {
    for kind in [OpKind::Copy, OpKind::Move] {
        let (_root, dirs, journal) = setup();
        let fs = FakeFs::new();
        fs.add_file("source/a", 7);
        fs.add_file("dest/a", 2);
        let old = fs.file_identity(&fs.location("dest/a")).unwrap();
        let plan = overwrite(
            &fs,
            vec![fs.location("source/a")],
            fs.location("dest"),
            kind,
        );
        let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
        assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
        assert!(entries(&dirs).is_empty());
        log.finish(&JobOutcome::Done, &fs).unwrap();
        drop(log);
        let saved = entries(&dirs);
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].record.version, 2);
        assert!(!saved[0].pinned);
        assert_eq!(
            fs.file_identity(&saved[0].record.held().unwrap()).unwrap(),
            old
        );
        assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 7);
        assert_eq!(
            fs.metadata(&fs.location("source/a")).is_ok(),
            kind == OpKind::Copy
        );
        assert!(
            lock(&journal)
                .held(rusty_store::JobId(saved[0].job_id.unwrap()))
                .unwrap()
                .is_empty()
        );
        recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
        assert_eq!(
            fs.metadata(&saved[0].record.held().unwrap()).unwrap().size,
            2
        );
    }
}

#[test]
fn interrupted_overwrite_preserves_both_versions_and_completes_as_failed() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 2);
    let plan = overwrite(
        &fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Copy,
    );
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop(log);
    let report = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(report.pending.is_empty());
    assert!(report.restored.is_empty());
    let saved = entries(&dirs);
    assert!(!saved[0].pinned);
    assert_eq!(
        lock(&journal)
            .read_job(rusty_store::JobId(saved[0].job_id.unwrap()))
            .unwrap()
            .0,
        JobState::Failed
    );
    assert_eq!(
        fs.metadata(&saved[0].record.held().unwrap()).unwrap().size,
        2
    );
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 7);
    let repeated = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(repeated.pending.is_empty());
    assert_eq!(entries(&dirs).len(), 1);
}

fn pair(fs: &FakeFs) -> OperationPlan {
    for name in ["a", "b"] {
        fs.add_file(&format!("source/{name}"), 7);
        fs.add_file(&format!("dest/{name}"), 2);
    }
    overwrite(
        fs,
        vec![fs.location("source/a"), fs.location("source/b")],
        fs.location("dest"),
        OpKind::Copy,
    )
}

fn execute_partial(
    log: &mut dyn OpLog,
    (fs, plan): (&FakeFs, &OperationPlan),
    cancelled: bool,
) -> JobOutcome {
    let cancel = CancelToken::new();
    log.check(LoggedOperation {
        plan,
        ops: fs,
        cancel: &cancel,
        progress: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    rusty_fs::run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log,
            cancel: &cancel,
            progress: &mut |_| {
                if cancelled && fs.metadata(&fs.location("dest/a")).unwrap().size == 7 {
                    cancel.cancel();
                }
            },
        },
    )
}

#[test]
fn cancellation_and_partial_failure_keep_previous_versions_without_releasing_them() {
    for cancelled in [false, true] {
        let (_root, dirs, journal) = setup();
        let fs = FakeFs::new();
        let plan = pair(&fs);
        if !cancelled {
            fs.fault(
                "source/b",
                rusty_fs::FakeOp::Copy,
                1,
                rusty_fs::Fault::Fail(rusty_core::FsErrorKind::AccessDenied),
            );
        }
        let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
        let outcome = execute_partial(log.as_mut(), (&fs, &plan), cancelled);
        if cancelled {
            assert_eq!(outcome, JobOutcome::Cancelled);
        } else {
            assert!(matches!(outcome, JobOutcome::Failed { .. }));
        }
        log.finish(&outcome, &fs).unwrap();
        drop(log);
        let saved = entries(&dirs);
        assert_eq!(saved.len(), 1);
        assert!(!saved[0].pinned);
        assert_eq!(saved[0].state, RetainedState::Ready);
        assert_eq!(
            fs.metadata(&saved[0].record.held().unwrap()).unwrap().size,
            2
        );
        let expected = if cancelled {
            JobState::Cancelled
        } else {
            JobState::Failed
        };
        assert_eq!(
            lock(&journal)
                .read_job(rusty_store::JobId(saved[0].job_id.unwrap()))
                .unwrap()
                .0,
            expected
        );
        assert_eq!(fs.metadata(&fs.location("dest/b")).unwrap().size, 2);
    }
}

#[test]
fn proof_journal_failure_keeps_the_original_target_and_no_adopted_versions() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 2);
    let original = fs.file_identity(&fs.location("dest/a")).unwrap();
    let plan = overwrite(
        &fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Copy,
    );
    let mut log = profile_factory(journal, dirs.clone())(&plan).unwrap();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection().execute_batch("CREATE TRIGGER refuse_proof BEFORE INSERT ON undo BEGIN SELECT RAISE(ABORT, 'refused'); END;").unwrap();
    let outcome = execute(log.as_mut(), &plan, &fs);
    assert!(matches!(outcome, JobOutcome::Failed { .. }));
    log.finish(&outcome, &fs).unwrap();
    assert_eq!(fs.file_identity(&fs.location("dest/a")).unwrap(), original);
    assert!(entries(&dirs).is_empty());
}

#[test]
fn failed_completion_rolls_back_catalog_and_preserves_all_held_evidence() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 2);
    let plan = overwrite(
        &fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Copy,
    );
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection().execute_batch("CREATE TRIGGER refuse_finish BEFORE UPDATE OF state ON job WHEN NEW.state = 'done' BEGIN SELECT RAISE(ABORT, 'refused'); END;").unwrap();
    assert!(log.finish(&JobOutcome::Done, &fs).is_err());
    drop(log);
    assert!(entries(&dirs).is_empty());
    let job = lock(&journal).restore_pending().unwrap().remove(0);
    let held = super::super::held::held_of(&journal, job.id).unwrap();
    assert_eq!(held.proved.len(), 1);
    assert_eq!(fs.metadata(&held.files[0].held).unwrap().size, 2);
    db.connection()
        .execute_batch("DROP TRIGGER refuse_finish")
        .unwrap();
    assert_eq!(
        recover_profile(&journal, &fs, (&dirs.data, &[]), true)
            .unwrap()
            .pending
            .len(),
        0
    );
    assert_eq!(entries(&dirs).len(), 1);
}

#[test]
fn overwrite_adoption_rechecks_prior_payloads_after_progress_callbacks() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = pair(&fs);
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    let job = lock(&journal).unfinished().unwrap().remove(0);
    let held = super::super::held::held_of(&journal, job.id).unwrap();
    let first = held.proved[0].0.held.clone();
    let trigger = held.proved[1].0.original.clone();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    let mut vault = Vault::new(db, &dirs.data, &fs).unwrap();
    let result = vault.adopt_job(
        rusty_retention::AdoptJob {
            job: job.id,
            state: JobState::Done,
            files: &held.proved,
            policy: Default::default(),
            created_at: job.created_at,
        },
        &mut RetentionRun::new(&CancelToken::new(), &mut |update| {
            if update.current == trigger {
                fs.remove_file(&first).unwrap();
                fs.add_file(&first.path().to_text(), 2);
            }
        }),
    );
    assert!(result.is_err());
    assert!(entries(&dirs).is_empty());
    assert_eq!(
        lock(&journal).read_job(job.id).unwrap().0,
        JobState::Running
    );
}

#[test]
fn native_shared_holder_restores_each_version_without_removing_its_neighbor() {
    let (_root, dirs, journal) = setup();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("source");
    let dest = files.path().join("dest");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&dest).unwrap();
    for name in ["a", "b"] {
        std::fs::write(source.join(name), b"new version").unwrap();
        std::fs::write(dest.join(name), b"old version\0\xff").unwrap();
    }
    let location =
        |path: &std::path::Path| Location::new(path.to_string_lossy().into_owned()).unwrap();
    let fs = LocalFs::new();
    let plan = overwrite(
        &fs,
        vec![location(&source.join("a")), location(&source.join("b"))],
        location(&dest),
        OpKind::Copy,
    );
    let mut log = profile_factory(journal, dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    drop(log);
    let saved = entries(&dirs);
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].record.container, saved[1].record.container);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    let mut vault = Vault::new(db, &dirs.data, &fs).unwrap();
    for (index, entry) in saved.iter().enumerate() {
        let target = dest.join(format!("restored-{index}"));
        vault
            .restore(
                RestoreRequest {
                    id: entry.id,
                    target: &location(&target),
                },
                &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
            )
            .unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"old version\0\xff");
        assert_eq!(std::fs::read(dest.join("a")).unwrap(), b"new version");
        if index == 0 {
            assert!(std::path::Path::new(entry.record.container.as_str()).exists());
        }
    }
    assert!(!std::path::Path::new(saved[0].record.container.as_str()).exists());
}

mod boundaries;

mod temporaries;
