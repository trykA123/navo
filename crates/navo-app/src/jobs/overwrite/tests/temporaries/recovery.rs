use super::*;

#[test]
fn damaged_owner_metadata_never_sends_temporary_evidence_to_legacy_cleanup() {
    for corrupt_detail in [false, true] {
        let interrupted = Interrupted::new();
        let job = lock(&interrupted.journal).unfinished().unwrap().remove(0);
        let (db, _) = Db::open(&interrupted.dirs.journal_file(), false).unwrap();
        db.connection()
            .execute(
                "UPDATE job SET kind = 'copy', plan = ?2 WHERE id = ?1",
                (job.id.0, serde_json::to_vec(&interrupted.plan).unwrap()),
            )
            .unwrap();
        if corrupt_detail {
            db.connection()
                .execute(
                    "UPDATE job_step SET detail = ?2 WHERE job_id = ?1",
                    (job.id.0, b"truncated temporary record".as_slice()),
                )
                .unwrap();
        }
        let Interrupted {
            journal,
            dirs,
            fs,
            proof,
            log,
            _root,
            ..
        } = interrupted;
        drop(log);
        let report = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
        assert!(report.pending.iter().any(|pending| pending.job == job.id));
        assert_eq!(fs.metadata(&proof.temporary).unwrap().size, 3);
        assert_eq!(
            lock(&journal).read_job(job.id).unwrap().0,
            JobState::RestorePending
        );
    }
}

#[test]
fn legacy_unproved_partial_is_preserved_while_a_new_namespace_can_complete() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = single(&fs);
    let old =
        navo_core::retention::RetainedWritePlan::new(plan.clone(), Default::default()).unwrap();
    let bytes = serde_json::to_vec(&old).unwrap();
    lock(&journal)
        .create_job(old.journal_kind(), &bytes, 1, 100)
        .unwrap();
    let legacy = fs.location(&format!("dest/a{}", navo_fs::TEMP_SUFFIX));
    fs.add_file(&key(&legacy), 3);
    recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert_eq!(fs.metadata(&legacy).unwrap().size, 3);
    let mut retry = profile_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(retry.as_mut(), &plan, &fs), JobOutcome::Done);
    retry.finish(&JobOutcome::Done, &fs).unwrap();
    assert_eq!(fs.metadata(&legacy).unwrap().size, 3);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 7);
}

#[test]
fn a_returned_copy_whose_folder_flush_fails_keeps_its_proof_until_the_next_start() {
    let mut interrupted = Interrupted::new();
    let evidence = strand_original(&mut interrupted);
    let Interrupted {
        journal,
        dirs,
        fs,
        log,
        _root,
        ..
    } = interrupted;
    let job = lock(&journal).unfinished().unwrap().remove(0);
    drop(log);
    let next = fs.calls("dest", navo_fs::FakeOp::Flush) + 1;
    let fails = navo_fs::Fault::Fail(navo_core::FsErrorKind::Unknown);
    for call in [next, next + 1] {
        fs.fault("dest", navo_fs::FakeOp::Flush, call, fails.clone());
    }
    let report = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    let original = fs.location("dest/a");
    assert_eq!(
        fs.file_identity(&original).unwrap(),
        Some(evidence.node.identity)
    );
    assert!(report.pending.iter().any(|pending| pending.job == job.id));
    let (state, _) = lock(&journal).read_job(job.id).unwrap();
    assert_eq!(state, JobState::RestorePending);
    assert_eq!(lock(&journal).held(job.id).unwrap().len(), 1);
    let again = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(again.pending.is_empty());
    assert_eq!(lock(&journal).read_job(job.id).unwrap().0, JobState::Failed);
    assert!(fs.metadata(&fs.location("dest/.ferrite-held-ab")).is_err());
}
