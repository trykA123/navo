use super::*;

#[test]
fn sequential_failure_retires_its_proved_partial_and_allows_retry() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = single(&fs);
    fs.fault(
        "source/a",
        rusty_fs::FakeOp::Copy,
        1,
        rusty_fs::Fault::FailAfterBytes {
            kind: rusty_core::FsErrorKind::Unknown,
            bytes: 3,
        },
    );
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    log.check(LoggedOperation {
        plan: &plan,
        ops: &fs,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    let failed = rusty_fs::run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: &fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert!(matches!(failed, JobOutcome::Failed { .. }));
    log.finish(&failed, &fs).unwrap();
    no_partial(&fs);
    drop(log);
    let mut retry = profile_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(retry.as_mut(), &plan, &fs), JobOutcome::Done);
    retry.finish(&JobOutcome::Done, &fs).unwrap();
}

#[test]
fn parallel_registration_failures_drain_every_waiting_worker_without_copying_bytes() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = pair(&fs);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection().execute_batch("CREATE TRIGGER refuse_created_temporary BEFORE UPDATE OF detail ON job_step WHEN NEW.detail IS NOT NULL AND json_valid(NEW.detail) AND json_extract(NEW.detail, '$.identity') IS NOT NULL BEGIN SELECT RAISE(ABORT, 'registration refused'); END;").unwrap();
    let mut log = profile_factory(journal.clone(), dirs)(&plan).unwrap();
    let outcome = execute(log.as_mut(), &plan, &fs);
    assert!(matches!(outcome, JobOutcome::Failed { .. }));
    assert!(log.finish(&outcome, &fs).is_err());
    let job = lock(&journal).restore_pending().unwrap().remove(0);
    assert_eq!(job.steps.len(), 2);
    for step in &job.steps {
        let proof: rusty_core::retention::CopyTemporary =
            serde_json::from_slice(step.detail.as_deref().unwrap()).unwrap();
        assert!(proof.identity.is_none());
        assert_eq!(fs.metadata(&proof.temporary).unwrap().size, 0);
    }
    for name in ["a", "b"] {
        assert_eq!(
            fs.metadata(&fs.location(&format!("dest/{name}")))
                .unwrap()
                .size,
            2
        );
    }
}

#[test]
fn created_handle_registration_never_reopens_the_live_destination() {
    let mut interrupted = Interrupted::new();
    let path = key(&interrupted.proof.temporary);
    let before = interrupted.fs.calls(&path, rusty_fs::FakeOp::Identity);
    interrupted.fs.fault(
        &path,
        rusty_fs::FakeOp::Identity,
        before + 1,
        rusty_fs::Fault::Fail(rusty_core::FsErrorKind::SharingViolation),
    );
    interrupted
        .log
        .created_temporary(
            (interrupted.proof.seq, interrupted.proof.identity.unwrap()),
            &interrupted.fs,
        )
        .unwrap();
    assert_eq!(
        interrupted.fs.calls(&path, rusty_fs::FakeOp::Identity),
        before
    );
}
