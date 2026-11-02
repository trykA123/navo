use navo_fs::existing_at;

use super::*;

pub(super) fn copy_with(session: &Session, factory: &LogFactory) {
    session.put("source/a", (7, 2_000), NEW_HASH);
    session.put("dest/a", (2, 1_000), OLD_HASH);
    let ops: &dyn ProviderOps = session.fs.as_ref();
    let cancel = CancelToken::new();
    let items = scan(ops, &[session.at("source/a")], &cancel).unwrap();
    let existing = existing_at(ops, &items, &session.at("dest"), &cancel).unwrap();
    let plan = plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(session.at("dest")),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: Vec::new(),
    });
    let mut log = factory(&plan).unwrap();
    log.check(LoggedOperation {
        plan: &plan,
        ops,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops,
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, ops).unwrap();
}

#[test]
fn restoring_under_retention_lands_beside_the_current_file() {
    let session = Session::new("[retention]\nretain-overwrites = true\n");
    copy_with(
        &session,
        &session.holds.compare_factory(Some(session.ops())),
    );
    let beside = session.at("dest/a (restored)");
    assert_eq!(
        session.restore(),
        Ok(PreviousVersion::Retained {
            at: vec![beside.clone()]
        })
    );
    assert_eq!(session.fs.metadata(&session.at("dest/a")).unwrap().size, 7);
    assert_eq!(session.fs.metadata(&beside).unwrap().size, 2);
    assert_eq!(session.restore(), Err(RestoreError::NotKept));
}

#[test]
fn compare_copy_keeps_the_old_file_until_exit_through_the_production_factory() {
    for experimental in [false, true] {
        let session = Session::new("[retention]\nretain-overwrites = false\n");
        let drive = experimental.then(|| session.ops());
        copy_with(&session, &session.holds.compare_factory(drive));
        let held = session.held();
        assert_eq!(session.state(), JobState::Done);
        assert_eq!(held.len(), 1);
        assert_eq!(session.fs.metadata(&held[0]).unwrap().size, 2);
        assert_eq!(session.fs.metadata(&session.at("dest/a")).unwrap().size, 7);
        session.holds.release(session.fs.as_ref());
        assert!(session.present(&held[0]).is_none());
        assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
        assert_eq!(session.fs.metadata(&session.at("dest/a")).unwrap().size, 7);
    }
}

#[test]
fn ordinary_copy_releases_the_old_file_through_the_production_factory() {
    let session = Session::new(OFF);
    copy_with(&session, &session.holds.factory(None));
    assert_eq!(session.state(), JobState::Done);
    assert_eq!(session.fs.metadata(&session.at("dest/a")).unwrap().size, 7);
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
    assert_eq!(session.restore(), Err(RestoreError::NotKept));
}

#[test]
fn restoring_a_compare_copy_without_retention_uses_the_job_undo() {
    for experimental in [false, true] {
        let session = Session::new("[retention]\nretain-overwrites = false\n");
        let drive = experimental.then(|| session.ops());
        copy_with(&session, &session.holds.compare_factory(drive));
        let old = session.held().remove(0);
        let original_hash = session.present(&old).unwrap();
        let target = session.at("dest/a");
        assert_eq!(
            session.restore(),
            Ok(PreviousVersion::Undone { at: target.clone() })
        );
        assert_eq!(session.fs.metadata(&target).unwrap().size, 2);
        assert_eq!(session.present(&target), Some(original_hash));
        assert_eq!(
            session.fs.metadata(&session.at("source/a")).unwrap().size,
            7
        );
        assert_eq!(session.state(), JobState::Undone);
        let (_, stored) = lock(&session.journal).read_job(SAVE).unwrap();
        assert!(
            stored
                .steps
                .iter()
                .filter(|step| step.state == StepState::Done)
                .all(|step| step.is_undone())
        );
        assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
        assert_eq!(session.restore(), Err(RestoreError::NotKept));
    }
}

#[test]
fn a_compare_copy_with_a_missing_old_file_refuses_restore_and_releases_at_exit() {
    let session = Session::new(OFF);
    copy_with(&session, &session.holds.compare_factory(None));
    let old = session.held().remove(0);
    session.fs.remove_file(&old).unwrap();
    assert_eq!(
        session.restore(),
        Err(RestoreError::Fs(FsErrorKind::NotFound))
    );
    assert_eq!(session.state(), JobState::Done);
    session.holds.release(session.fs.as_ref());
    assert_eq!(session.fs.metadata(&session.at("dest/a")).unwrap().size, 7);
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
}

#[test]
fn a_compare_copy_refused_before_changes_releases_at_exit_without_replay() {
    let session = Session::new(OFF);
    copy_with(&session, &session.holds.compare_factory(None));
    let old = session.held().remove(0);
    let next = session.fs.calls("dest/a", FakeOp::Metadata) + 1;
    session.fs.fault(
        "dest/a",
        FakeOp::Metadata,
        next,
        Fault::Fail(FsErrorKind::SharingViolation),
    );
    assert_eq!(
        session.restore(),
        Err(RestoreError::Fs(FsErrorKind::SharingViolation))
    );
    assert_eq!(session.state(), JobState::Done);
    assert!(
        !lock(&session.journal)
            .read_job(SAVE)
            .unwrap()
            .1
            .steps
            .iter()
            .any(|step| step.is_undone())
    );
    session.holds.release(session.fs.as_ref());
    assert_eq!(session.present(&old), None);
    let report = crate::jobs::recover_profile(
        &session.journal,
        session.fs.as_ref(),
        (&session.dirs.data, &[]),
        false,
    )
    .unwrap();
    assert!(report.pending.is_empty());
    assert_eq!(session.fs.metadata(&session.at("dest/a")).unwrap().size, 7);
}

#[test]
fn a_compare_copy_refuses_an_edit_with_unchanged_size_and_mtime() {
    let session = Session::new(OFF);
    copy_with(&session, &session.holds.compare_factory(None));
    let old = session.held().remove(0);
    let target = session.at("dest/a");
    let entry = session.fs.metadata(&target).unwrap();
    session.fs.set_hash("dest/a", [9; 32]);
    assert_eq!(session.restore(), Err(RestoreError::Changed));
    assert_eq!(session.fs.metadata(&target).unwrap(), entry);
    assert_eq!(session.present(&target), Some([9; 32]));
    assert_eq!(session.present(&old), Some(OLD_HASH));
    assert_eq!(session.state(), JobState::Done);
    assert!(!super::super::super::held::is_restoring(&session.journal, SAVE).unwrap());
}
