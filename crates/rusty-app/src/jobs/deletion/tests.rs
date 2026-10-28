use crate::jobs::SharedJournal;
use rusty_core::retention::RetainedState;
use rusty_core::{ConflictPolicy, Location, OpKind, OperationPlan, PlanRequest, Step, plan};
use rusty_fs::{CancelToken, FakeFs, LocalFs, Provider, ProviderOps, scan};
use rusty_store::{Db, RetentionCatalog, StoreDirs};
use rusty_ui::browser::OpLog;

use super::*;

mod confirmed;

fn setup() -> (tempfile::TempDir, StoreDirs, SharedJournal) {
    let profile = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(profile.path());
    dirs.ensure().unwrap();
    std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = true\n").unwrap();
    let journal = super::super::open(&dirs.journal_file()).unwrap();
    (profile, dirs, journal)
}

fn delete(ops: &dyn ProviderOps, roots: &[Location]) -> OperationPlan {
    plan(&PlanRequest {
        kind: OpKind::Delete,
        items: scan(ops, roots, &CancelToken::new()).unwrap(),
        target: None,
        existing: vec![],
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: vec![],
    })
}

fn execute(log: &mut dyn OpLog, plan: &OperationPlan, ops: &dyn ProviderOps) -> JobOutcome {
    let confirmed = Some(rusty_core::retention::RetentionPolicy::default());
    execute_confirmed(log, plan, ops, confirmed)
}

fn execute_confirmed(
    log: &mut dyn OpLog,
    plan: &OperationPlan,
    ops: &dyn ProviderOps,
    confirmed: Option<rusty_core::retention::RetentionPolicy>,
) -> JobOutcome {
    log.check(LoggedOperation {
        plan,
        ops,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: confirmed,
    })
    .unwrap();
    log.execute(LoggedOperation {
        plan,
        ops,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: confirmed,
    })
    .unwrap()
}

#[test]
fn changed_job_kind_does_not_authorize_legacy_rollback_of_retained_data() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let plan = delete(&fs, &[fs.location("home/a")]);
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop(log);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute("UPDATE job SET kind = 'delete'", [])
        .unwrap();
    let report = super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(report.restored.is_empty());
    assert_eq!(report.pending.len(), 1);
    assert!(entries(&dirs)[0].pinned);
}

#[test]
fn another_profile_cannot_move_or_recycle_a_folder_with_retained_children() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 8);
    let plan = delete(&fs, &[fs.location("home/docs/a")]);
    let mut log = super::super::experimental_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    drop(log);
    let (_other_profile, other, journal) = setup();
    let mut plan = plan.clone();
    plan.kind = OpKind::Move;
    for step in [
        Step::Rename {
            source: fs.location("home/docs"),
            target: fs.location("home/moved"),
        },
        Step::Recycle {
            location: fs.location("home/docs"),
        },
    ] {
        plan.steps = vec![step];
        let mut log =
            super::super::experimental_factory(journal.clone(), other.clone())(&plan).unwrap();
        assert!(
            log.check(LoggedOperation {
                plan: &plan,
                ops: &fs,
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
                waiting: &mut |_| {},
                retain_deletes: Some(rusty_core::retention::RetentionPolicy::default()),
            })
            .is_err()
        );
    }
    assert!(fs.metadata(&fs.location("home/docs")).is_ok());
}

#[test]
fn writes_cannot_detach_profile_lock_files() {
    let (_profile, dirs, journal) = setup();
    let plan = OperationPlan {
        kind: OpKind::Delete,
        steps: vec![Step::RemoveFile {
            location: Location::new(
                dirs.data
                    .join("operations.lock")
                    .to_string_lossy()
                    .into_owned(),
            )
            .unwrap(),
        }],
        totals: rusty_core::Totals {
            files: 1,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    };
    assert!(super::super::experimental_factory(journal, dirs.clone())(&plan).is_err());
    assert!(
        super::super::claim_write(
            &dirs,
            &Location::new(
                dirs.data
                    .join("operations.lock")
                    .to_string_lossy()
                    .into_owned()
            )
            .unwrap(),
            true
        )
        .is_err()
    );
}

fn entries(dirs: &StoreDirs) -> Vec<rusty_core::retention::RetentionEntry> {
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    RetentionCatalog::new(db)
        .unwrap()
        .list_after(0, 100)
        .unwrap()
}

#[test]
fn ordinary_delete_keeps_a_complete_tree_with_its_durable_policy() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/docs/nested/a", 8);
    let source = fs.location("home/docs");
    let plan = delete(&fs, std::slice::from_ref(&source));
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    let saved = entries(&dirs);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].record.source, source);
    assert!(saved[0].pinned);
    assert_eq!(saved[0].record.nodes.len(), 3);
    assert_eq!(
        saved[0].record.expires_at - saved[0].record.created_at,
        3 * rusty_core::retention::DAY_MS
    );
    log.finish(&JobOutcome::Done, &fs).unwrap();
    assert!(!entries(&dirs)[0].pinned);
    assert!(fs.metadata(&source).is_err());
    assert!(fs.metadata(&saved[0].record.held().unwrap()).is_ok());
}

#[test]
fn cancellation_after_capture_restores_the_original_and_leaves_no_live_payload() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let source = fs.location("home/a");
    let plan = delete(&fs, std::slice::from_ref(&source));
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let cancel = CancelToken::new();
    let outcome = log
        .execute(LoggedOperation {
            plan: &plan,
            ops: &fs,
            cancel: &cancel,
            progress: &mut |_| {
                if fs.metadata(&source).is_err() {
                    cancel.cancel();
                }
            },
            waiting: &mut |_| {},
            retain_deletes: Some(rusty_core::retention::RetentionPolicy::default()),
        })
        .unwrap();
    assert_eq!(outcome, JobOutcome::Cancelled);
    log.finish(&outcome, &fs).unwrap();
    assert_eq!(fs.metadata(&source).unwrap().size, 8);
    assert!(entries(&dirs).iter().all(|entry| entry.state.is_terminal()));
}

#[test]
fn incomplete_job_is_restored_on_profile_recovery() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let source = fs.location("home/a");
    let plan = delete(&fs, std::slice::from_ref(&source));
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop(log);
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-deletes = false\n",
    )
    .unwrap();
    let report = super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert_eq!(report.restored.len(), 1);
    assert!(report.pending.is_empty());
    assert_eq!(fs.metadata(&source).unwrap().size, 8);
    assert_eq!(entries(&dirs)[0].state, RetainedState::Restored);
}

#[test]
fn occupied_original_stays_pending_with_readable_retained_paths_and_can_retry() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let source = fs.location("home/a");
    let plan = delete(&fs, std::slice::from_ref(&source));
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop(log);
    fs.add_file("home/a", 99);
    let report = super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert_eq!(report.pending.len(), 1);
    assert!(report.pending[0].left.contains(&source));
    assert!(
        report.pending[0]
            .left
            .contains(&entries(&dirs)[0].record.container)
    );
    assert_eq!(fs.metadata(&source).unwrap().size, 99);
    fs.remove_file(&source).unwrap();
    assert!(
        super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true)
            .unwrap()
            .pending
            .is_empty()
    );
    assert_eq!(fs.metadata(&source).unwrap().size, 8);
}

#[test]
fn failed_finish_write_reports_failure_and_preserves_recovery_evidence() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let plan = delete(&fs, &[fs.location("home/a")]);
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection().execute_batch("CREATE TRIGGER no_finish BEFORE UPDATE OF state ON job BEGIN SELECT RAISE(ABORT, 'finish refused'); END;").unwrap();
    assert!(log.finish(&JobOutcome::Done, &fs).is_err());
    drop(log);
    assert!(entries(&dirs)[0].pinned);
    db.connection()
        .execute_batch("DROP TRIGGER no_finish;")
        .unwrap();
    assert_eq!(
        super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true)
            .unwrap()
            .restored
            .len(),
        1
    );
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
}

#[test]
fn missing_catalog_payload_is_not_misreported_as_successful_recovery() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let plan = delete(&fs, &[fs.location("home/a")]);
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop(log);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute("DELETE FROM retained_entry", [])
        .unwrap();
    let report = super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(report.restored.is_empty());
    assert_eq!(report.pending.len(), 1);
}

#[test]
fn profile_lease_and_private_containers_block_other_mutations() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let plan = delete(&fs, &[fs.location("home/a")]);
    let factory = super::super::experimental_factory(journal.clone(), dirs.clone());
    let mut log = factory(&plan).unwrap();
    assert!(factory(&plan).is_ok());
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    drop(log);
    let container = entries(&dirs)[0].record.container.clone();
    assert!(factory(&delete(&fs, &[container])).is_err());
    assert!(factory(&delete(&fs, &[fs.location("home")])).is_err());
}

#[test]
fn invalid_settings_refuse_mutations_without_creating_a_job() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-deletes = true\ndays = 8\n",
    )
    .unwrap();
    assert!(
        super::super::experimental_factory(journal.clone(), dirs)(&delete(
            &fs,
            &[fs.location("home/a")]
        ))
        .is_err()
    );
    assert!(lock(&journal).unfinished().unwrap().is_empty());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
}

#[test]
fn native_tree_keeps_exact_bytes_and_restores_through_startup_preflight() {
    let (_profile, dirs, journal) = setup();
    let files = tempfile::tempdir().unwrap();
    let tree = files.path().join("tree");
    std::fs::create_dir_all(tree.join("nested")).unwrap();
    std::fs::write(tree.join("nested/a"), b"retained bytes\0\xff").unwrap();
    let fs = LocalFs::new();
    let source = Location::new(tree.to_string_lossy().into_owned()).unwrap();
    let plan = delete(&fs, std::slice::from_ref(&source));
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop((log, journal));
    assert!(!tree.exists());
    let saved = entries(&dirs);
    let held = std::path::Path::new(saved[0].record.container.as_str()).join("payload/nested/a");
    assert_eq!(std::fs::read(held).unwrap(), b"retained bytes\0\xff");
    let started = crate::crash::store::check_with(Some(&dirs.data), true, &fs, true);
    assert_eq!(
        std::fs::read(tree.join("nested/a")).unwrap(),
        b"retained bytes\0\xff"
    );
    assert_eq!(entries(&dirs)[0].state, RetainedState::Restored);
    drop(started);
}

fn finished_rename(
    fs: &FakeFs,
    plan: &OperationPlan,
    (journal, dirs): (&SharedJournal, &StoreDirs),
) -> rusty_store::JobId {
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(plan).unwrap();
    log.check(LoggedOperation {
        plan,
        ops: fs,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: Some(rusty_core::retention::RetentionPolicy::default()),
    })
    .unwrap();
    let outcome = rusty_fs::run_job(
        plan,
        &[],
        &mut rusty_fs::JobContext {
            ops: fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, fs).unwrap();
    drop(log);
    lock(journal).last_undoable("rename").unwrap().unwrap().id
}

#[test]
fn refused_undo_checks_the_current_target_and_keeps_the_finished_job_terminal() {
    let (_other_profile, other, other_journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 8);
    let mut rename = delete(&fs, &[fs.location("home/docs")]);
    rename.kind = OpKind::Rename;
    rename.steps = vec![Step::Rename {
        source: fs.location("home/docs"),
        target: fs.location("home/renamed"),
    }];
    let job = finished_rename(&fs, &rename, (&other_journal, &other));
    let (_profile, dirs, journal) = setup();
    let plan = delete(&fs, &[fs.location("home/renamed/a")]);
    let mut log = super::super::experimental_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    drop(log);
    let mut undo = super::super::profile_undo(&other_journal, job, &other, true).unwrap();
    assert!(
        undo.check(LoggedOperation {
            plan: &rename,
            ops: &fs,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            waiting: &mut |_| {},
            retain_deletes: Some(rusty_core::retention::RetentionPolicy::default()),
        })
        .is_err()
    );
    drop(undo);
    assert_eq!(
        lock(&other_journal).read_job(job).unwrap().0,
        rusty_store::JobState::Done
    );
    assert!(
        super::super::recover_profile(&other_journal, &fs, (&other.data, &[]), true)
            .unwrap()
            .pending
            .is_empty()
    );
    assert!(fs.metadata(&fs.location("home/renamed")).is_ok());
    assert!(fs.metadata(&fs.location("home/docs")).is_err());
}

#[test]
fn browser_worker_routes_confirmed_delete_through_the_retention_adapter() {
    use rusty_core::{AppState, Command};
    use rusty_ui::browser::{Browser, JobStatus, Operations};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn settle(browser: &mut Browser) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while (browser.loading() || browser.running()) && Instant::now() < deadline {
            browser.poll();
            std::thread::yield_now();
        }
        browser.poll();
        assert!(!browser.loading() && !browser.running());
    }

    let (_profile, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    fs.add_file("home/docs/a", 8);
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        fs.clone(),
        Arc::new(|| {}),
    );
    let confirming = dirs.clone();
    browser.set_operations(Operations {
        retain_deletes: Some(Arc::new(move || super::super::retains_deletes(&confirming))),
        log: super::super::experimental_factory(journal, dirs.clone()),
        write: Some(super::super::write_factory(dirs.clone(), true)),
        protected: vec![],
    });
    browser.start();
    settle(&mut browser);
    browser.command(&Command::SelectionToggle).unwrap();
    assert!(browser.prepare(OpKind::Delete));
    settle(&mut browser);
    assert!(browser.confirm());
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert_eq!(entries(&dirs).len(), 1);
    assert!(!entries(&dirs)[0].pinned);
    assert!(fs.metadata(&fs.location("home/docs")).is_err());
}

#[test]
fn capacity_a_delete_never_purges_its_own_entry_during_its_finish_pass() {
    let (_root, dirs, journal) = setup();
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-deletes = true\nmax-volume-mib = 1\nmin-free-mib = 1\n",
    )
    .unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.set_available_space("home", Some(0));
    let plan = delete(&fs, &[fs.location("home/a")]);
    let mut log = super::super::experimental_factory(journal, dirs.clone())(&plan).unwrap();
    let confirmed = super::super::retains_deletes(&dirs);
    let outcome = execute_confirmed(log.as_mut(), &plan, &fs, confirmed);
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, &fs).unwrap();
    drop(log);
    let retained = entries(&dirs);
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].state, RetainedState::Ready);
    assert_eq!(
        fs.metadata(&retained[0].record.held().unwrap())
            .unwrap()
            .size,
        7
    );
}

#[test]
fn capacity_legacy_over_cap_v1_job_still_recovers_its_proved_payload() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    let plan = delete(&fs, &[fs.location("home/a")]);
    let mut log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    drop(log);
    let job = super::super::lock(&journal).unfinished().unwrap().remove(0);
    let mut saved: serde_json::Value = serde_json::from_slice(&job.plan).unwrap();
    saved["retention"]["max_volume_bytes"] = 1.into();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute(
            "UPDATE job SET plan = ?1 WHERE id = ?2",
            (serde_json::to_vec(&saved).unwrap(), job.id.0),
        )
        .unwrap();
    let report = super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(report.pending.is_empty());
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 8);
    assert_eq!(
        super::super::lock(&journal).read_job(job.id).unwrap().0,
        rusty_store::JobState::RolledBack
    );
}

fn interrupted_preparation(
    (dirs, journal): (&StoreDirs, &SharedJournal),
    fs: &FakeFs,
) -> rusty_core::retention::RetentionEntry {
    let job = super::super::lock(journal).unfinished().unwrap().remove(0);
    let source = fs.location("home/a");
    let identity = fs.file_identity(&source).unwrap().unwrap();
    super::super::identity::record(journal, job.id, (0, identity)).unwrap();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    let mut vault = Vault::new(db, &dirs.data, fs).unwrap();
    assert!(
        vault
            .capture(
                CaptureRequest {
                    source: &source,
                    policy: Default::default(),
                    created_at: job.created_at,
                    job: Some(job.id),
                },
                &mut RetentionRun::new(&CancelToken::new(), &mut |_| {})
            )
            .is_err()
    );
    let entry = vault.list_after(0, 10).unwrap().remove(0);
    assert_eq!(entry.state, RetainedState::Preparing);
    entry
}

#[test]
fn crash_before_delete_container_identity_commit_finishes_profile_recovery() {
    let (_profile, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    fs.fault(
        "home/a",
        rusty_fs::FakeOp::Rename,
        1,
        rusty_fs::Fault::Fail(rusty_core::FsErrorKind::Locked),
    );
    let plan = delete(&fs, &[fs.location("home/a")]);
    let log = super::super::experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let mut entry = interrupted_preparation((&dirs, &journal), &fs);
    drop(log);
    let job = rusty_store::JobId(entry.job_id.unwrap());
    entry.record.container_identity = None;
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute(
            "UPDATE retained_entry SET record = ?1 WHERE id = ?2",
            (serde_json::to_vec(&entry.record).unwrap(), entry.id.0),
        )
        .unwrap();
    let report = super::super::recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(report.pending.is_empty());
    assert_eq!(
        super::super::lock(&journal).read_job(job).unwrap().0,
        rusty_store::JobState::RolledBack
    );
    assert_eq!(entries(&dirs)[0].state, RetainedState::Aborted);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 8);
    assert!(fs.metadata(&entry.record.container).is_err());
}
