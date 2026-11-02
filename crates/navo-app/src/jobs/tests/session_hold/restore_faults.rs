use super::*;

fn versions(session: &Session) -> Vec<[u8; 32]> {
    let mut files = session.held();
    files.push(session.at(TARGET));
    let mut hashes: Vec<_> = files.iter().filter_map(|at| session.present(at)).collect();
    hashes.sort_unstable();
    hashes
}

fn fail_flushes(session: &Session, path: &str, offset: u32, persistent: bool) {
    let first = session.fs.calls(path, FakeOp::Flush) + offset;
    let last = if persistent { first + 16 } else { first };
    for call in first..=last {
        session
            .fs
            .fault(path, FakeOp::Flush, call, Fault::Fail(FsErrorKind::Unknown));
    }
}

fn assert_pending_through_exit_and_restart(session: &Session) {
    assert_eq!(session.state(), JobState::RestorePending);
    assert_eq!(versions(session), [OLD_HASH, NEW_HASH]);
    session.holds.release(session.fs.as_ref());
    assert_eq!(session.state(), JobState::RestorePending);
    assert_eq!(versions(session), [OLD_HASH, NEW_HASH]);
    let reopened = open(&session.dirs.journal_file()).unwrap();
    for _ in 0..2 {
        crate::jobs::recover_profile(
            &reopened,
            session.fs.as_ref(),
            (&session.dirs.data, &[]),
            false,
        )
        .unwrap();
        assert_eq!(
            lock(&reopened).read_job(SAVE).unwrap().0,
            JobState::RestorePending
        );
        assert_eq!(versions(session), [OLD_HASH, NEW_HASH]);
        assert_eq!(lock(&reopened).held(SAVE).unwrap().len(), 2);
    }
}

#[test]
fn failed_final_restore_flushes_keep_both_versions_pending_at_exit_and_restart() {
    for held_folder in [false, true] {
        for persistent in [false, true] {
            let session = Session::new(OFF);
            session.save();
            let path = flush_path(&session, held_folder);
            fail_flushes(&session, &path, 2, persistent);
            assert_eq!(
                session.restore(),
                Err(RestoreError::Fs(FsErrorKind::Unknown))
            );
            assert_eq!(session.file(TARGET), ORIGINAL);
            assert_pending_through_exit_and_restart(&session);
        }
    }
}

#[test]
fn failed_restore_checks_settle_only_after_a_flushed_rollback() {
    for held_folder in [false, true] {
        for persistent in [false, true] {
            let session = Session::new(OFF);
            session.save();
            let path = flush_path(&session, held_folder);
            fail_flushes(&session, &path, 1, persistent);
            assert_eq!(
                session.restore(),
                Err(RestoreError::Fs(FsErrorKind::Unknown))
            );
            assert_eq!(session.file(TARGET), SAVED);
            if persistent {
                assert_pending_through_exit_and_restart(&session);
            } else {
                assert_eq!(session.state(), JobState::Done);
                assert!(!super::super::super::held::is_restoring(&session.journal, SAVE).unwrap());
                let old = session.held();
                session.holds.release(session.fs.as_ref());
                assert!(old.iter().all(|at| session.present(at).is_none()));
                assert_eq!(session.file(TARGET), SAVED);
            }
        }
    }
}

fn flush_path(session: &Session, held_folder: bool) -> String {
    if held_folder {
        session.held()[0]
            .parent()
            .unwrap()
            .path()
            .to_text()
            .to_string()
    } else {
        "dest".to_owned()
    }
}

#[test]
fn a_missing_old_file_refuses_restore_without_leaving_it_pending() {
    let session = Session::new(OFF);
    session.save();
    let old = session.held().remove(0);
    session.fs.remove_file(&old).unwrap();
    assert_eq!(
        session.restore(),
        Err(RestoreError::Fs(FsErrorKind::NotFound))
    );
    assert_eq!(session.state(), JobState::Done);
    session.holds.release(session.fs.as_ref());
    assert_eq!(session.file(TARGET), SAVED);
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
}

#[test]
fn a_completed_restore_discards_the_newer_version_even_if_the_target_disappears() {
    for cleanup_fails in [false, true] {
        let session = Session::new(OFF);
        session.save();
        let old = session.held().remove(0);
        let current_path = format!("{}.current", old.path().to_text());
        if cleanup_fails {
            session.fs.fault(
                &current_path,
                FakeOp::Remove,
                1,
                Fault::Fail(FsErrorKind::SharingViolation),
            );
        }
        assert!(session.restore().is_ok());
        assert_eq!(session.state(), JobState::Undone);
        assert_eq!(session.file(TARGET), ORIGINAL);
        assert_eq!(
            session.present(&session.at(&current_path)),
            cleanup_fails.then_some(NEW_HASH)
        );
        assert_eq!(
            lock(&session.journal).held(SAVE).unwrap().is_empty(),
            !cleanup_fails
        );
        session.fs.remove_file(&session.at(TARGET)).unwrap();
        recover(&session.journal, session.fs.as_ref()).unwrap();
        assert_eq!(session.file(TARGET), None);
        assert_eq!(session.present(&session.at(&current_path)), None);
        assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
    }
}
