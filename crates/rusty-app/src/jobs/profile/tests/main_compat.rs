use super::*;
use rusty_fs::{FakeOp, Fault, JobContext, JobOutcome, run_job};

fn execute_default(log: &mut dyn OpLog, plan: &OperationPlan, fs: &FakeFs) {
    let cancel = CancelToken::new();
    check(log, plan, fs, &cancel).unwrap();
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log,
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, fs).unwrap();
}

fn stale_holder_steps(kind: OpKind, fs: &FakeFs) -> Vec<Step> {
    let at = |path| fs.location(path);
    match kind {
        OpKind::Recycle => vec![Step::Recycle {
            location: at("home/docs"),
        }],
        OpKind::Rename => vec![Step::Rename {
            source: at("home/docs"),
            target: at("home/papers"),
        }],
        OpKind::Delete => vec![
            Step::RemoveFile {
                location: at("home/docs/.ferrite-held-ab/a"),
            },
            Step::RemoveFolder {
                location: at("home/docs/.ferrite-held-ab"),
            },
            Step::RemoveFolder {
                location: at("home/docs/unreadable"),
            },
            Step::RemoveFolder {
                location: at("home/docs"),
            },
        ],
        _ => vec![Step::Rename {
            source: at("home/docs"),
            target: at("dest/docs"),
        }],
    }
}

#[test]
fn main_compat_default_jobs_allow_stale_holders_without_tree_scans() {
    for kind in [
        OpKind::Recycle,
        OpKind::Move,
        OpKind::Rename,
        OpKind::Delete,
    ] {
        let (_root, dirs, journal) = setup();
        let fs = FakeFs::new();
        fs.add_file("home/docs/.ferrite-held-ab/a", 2);
        fs.add_folder("home/docs/unreadable");
        fs.add_folder("dest");
        let steps = stale_holder_steps(kind, &fs);
        let plan = OperationPlan {
            kind,
            steps,
            totals: rusty_core::Totals {
                files: 0,
                folders: 0,
                bytes: 0,
            },
            conflicts: vec![],
            risks: vec![],
        };
        fs.fault(
            "home/docs/unreadable",
            FakeOp::List,
            1,
            Fault::Fail(rusty_core::FsErrorKind::AccessDenied),
        );
        let mut log = profile_factory(journal, dirs)(&plan).unwrap();
        execute_default(log.as_mut(), &plan, &fs);
        assert_eq!(fs.calls("home/docs/unreadable", FakeOp::List), 0);
        assert!(fs.metadata(&fs.location("home/docs")).is_err());
    }
}

#[test]
fn main_compat_default_moves_a_stranded_held_file_out() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("home/.ferrite-held-ab/a", 2);
    fs.add_folder("dest");
    let plan = OperationPlan {
        kind: OpKind::Move,
        steps: vec![Step::Rename {
            source: fs.location("home/.ferrite-held-ab/a"),
            target: fs.location("dest/a"),
        }],
        totals: rusty_core::Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    };
    let mut log = profile_factory(journal, dirs)(&plan).unwrap();
    execute_default(log.as_mut(), &plan, &fs);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 2);
}

#[test]
fn main_compat_legacy_recovery_uses_a_real_profile_data_path() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_folder("home/.ferrite-held-ab");
    fs.add_file("dest/a", 2);
    let plan = OperationPlan {
        kind: OpKind::Move,
        steps: vec![Step::Rename {
            source: fs.location("home/.ferrite-held-ab/a"),
            target: fs.location("dest/a"),
        }],
        totals: rusty_core::Totals {
            files: 1,
            folders: 0,
            bytes: 2,
        },
        conflicts: vec![],
        risks: vec![],
    };
    let job = crate::jobs::lock(&journal)
        .create_job("move", &serde_json::to_vec(&plan).unwrap(), 1, 0)
        .unwrap();
    crate::jobs::lock(&journal)
        .mark_step(job, 0, rusty_store::StepState::Done, None)
        .unwrap();
    let report = crate::jobs::recover_profile(&journal, &fs, (&dirs.data, &[]), false).unwrap();
    assert!(report.pending.is_empty());
    assert_eq!(
        crate::jobs::lock(&journal).read_job(job).unwrap().0,
        rusty_store::JobState::RolledBack
    );
}

#[test]
fn main_compat_a_failing_journal_finish_does_not_fail_a_finished_job() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = copy(&fs);
    let mut log = profile_factory(journal, dirs.clone())(&plan).unwrap();
    let (db, _) = rusty_store::Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute_batch(
            "CREATE TRIGGER refuse_finish BEFORE UPDATE OF state ON job \
             BEGIN SELECT RAISE(ABORT, 'disk gone'); END;",
        )
        .unwrap();
    execute_default(log.as_mut(), &plan, &fs);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 7);
}
