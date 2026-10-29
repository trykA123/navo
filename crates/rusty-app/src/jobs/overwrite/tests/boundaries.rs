use super::*;

#[test]
fn review_a_second_job_can_start_while_another_runs() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = pair(&fs);
    let factory = profile_factory(journal, dirs);
    let _running = factory(&plan).unwrap();
    assert!(factory(&plan).is_ok());
}

#[test]
fn review_an_unrelated_bad_setting_does_not_block_file_operations() {
    let (_root, dirs, journal) = setup();
    std::fs::write(dirs.settings_file(), "[automotive]\nmodules = [1]\n").unwrap();
    let fs = FakeFs::new();
    let made = profile_factory(journal, dirs)(&pair(&fs));
    assert!(made.is_ok(), "{:?}", made.err());
}

fn copied_pair() -> (
    tempfile::TempDir,
    StoreDirs,
    SharedJournal,
    FakeFs,
    Box<dyn OpLog>,
) {
    let (root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = pair(&fs);
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    (root, dirs, journal, fs, log)
}

#[test]
fn partial_adoption_cannot_discard_other_durable_overwrite_proofs() {
    let (_root, dirs, journal, fs, _log) = copied_pair();
    let job = lock(&journal).unfinished().unwrap().remove(0);
    let held = super::super::super::held::held_of(&journal, job.id).unwrap();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    let mut vault = Vault::new(db, &dirs.data, &fs).unwrap();
    let result = vault.adopt_job(
        &rusty_retention::AdoptJob {
            job: job.id,
            state: JobState::Done,
            files: &held.proved[..1],
            policy: Default::default(),
            created_at: job.created_at,
        },
        &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
    );
    assert!(result.is_err());
    assert!(entries(&dirs).is_empty());
    assert_eq!(
        lock(&journal).read_job(job.id).unwrap().0,
        JobState::Running
    );
    assert_eq!(lock(&journal).held(job.id).unwrap().len(), 2);
}

#[test]
fn repeated_completion_keeps_the_completed_state_and_single_catalog_batch() {
    let (_root, dirs, journal, fs, mut log) = copied_pair();
    let job = lock(&journal).unfinished().unwrap().remove(0);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    log.finish(&JobOutcome::Done, &fs).unwrap();
    assert_eq!(entries(&dirs).len(), 2);
    assert_eq!(lock(&journal).read_job(job.id).unwrap().0, JobState::Done);
    assert!(lock(&journal).held(job.id).unwrap().is_empty());
}

#[test]
fn returned_originals_leave_no_empty_holder_after_failed_placement() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 2);
    fs.fault(
        "source/a",
        rusty_fs::FakeOp::Rename,
        1,
        rusty_fs::Fault::Fail(rusty_core::FsErrorKind::Locked),
    );
    let original = fs.file_identity(&fs.location("dest/a")).unwrap();
    let plan = overwrite(
        &fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Move,
    );
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let outcome = execute(log.as_mut(), &plan, &fs);
    assert!(matches!(outcome, JobOutcome::Failed { .. }));
    let job = lock(&journal).unfinished().unwrap().remove(0);
    let held = super::super::super::held::held_of(&journal, job.id).unwrap();
    let container = held.proved[0].0.held.parent().unwrap();
    log.finish(&outcome, &fs).unwrap();
    assert_eq!(fs.file_identity(&fs.location("dest/a")).unwrap(), original);
    assert!(fs.metadata(&container).is_err());
    assert!(entries(&dirs).is_empty());
    assert!(lock(&journal).held(job.id).unwrap().is_empty());
}

#[test]
fn damaged_owner_kind_with_stripped_policy_keeps_proved_versions_pending() {
    let (_root, dirs, journal, fs, log) = copied_pair();
    let job = lock(&journal).unfinished().unwrap().remove(0);
    let held = super::super::super::held::held_of(&journal, job.id).unwrap();
    drop(log);
    let mut plan: serde_json::Value = serde_json::from_slice(&job.plan).unwrap();
    plan.as_object_mut().unwrap().remove("retention");
    plan.as_object_mut()
        .unwrap()
        .remove("overwrite_retention_version");
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .execute(
            "UPDATE job SET kind = 'copy', plan = ?1 WHERE id = ?2",
            (serde_json::to_vec(&plan).unwrap(), job.id.0),
        )
        .unwrap();
    let report = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert_eq!(report.pending.len(), 1);
    assert!(report.restored.is_empty());
    assert_eq!(lock(&journal).held(job.id).unwrap().len(), 2);
    for (file, proof) in held.proved {
        assert_eq!(
            fs.file_identity(&file.held).unwrap(),
            Some(proof.node.identity)
        );
        assert_eq!(fs.metadata(&file.original).unwrap().size, 7);
    }
}

#[test]
fn retained_completion_preserves_its_own_entries_and_the_next_job_enforces_quota() {
    let (_root, dirs, journal) = setup();
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-overwrites = true\nmax-volume-mib = 1\nmin-free-mib = 0\n",
    )
    .unwrap();
    let fs = FakeFs::new();
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 2 * 1024 * 1024);
    fs.set_available_space("dest", Some(u64::MAX));
    let plan = overwrite(
        &fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Copy,
    );
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    let job = lock(&journal).unfinished().unwrap().remove(0);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    let saved = entries(&dirs);
    assert_eq!(saved[0].state, RetainedState::Ready);
    assert!(fs.metadata(&saved[0].record.held().unwrap()).is_ok());
    drop(log);
    let mut next = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(next.as_mut(), &plan, &fs), JobOutcome::Done);
    next.finish(&JobOutcome::Done, &fs).unwrap();
    let saved = entries(&dirs);
    assert_eq!(saved[0].state, RetainedState::Purged);
    assert_eq!(saved[1].state, RetainedState::Ready);
    assert!(fs.metadata(&saved[0].record.container).is_err());
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 7);
    assert_eq!(lock(&journal).read_job(job.id).unwrap().0, JobState::Done);
}

#[test]
fn maintenance_observation_failure_preserves_success_and_retained_version() {
    let (_root, dirs, journal, fs, mut log) = copied_pair();
    fs.fault(
        "dest",
        rusty_fs::FakeOp::Space,
        1,
        rusty_fs::Fault::Fail(rusty_core::FsErrorKind::NetworkLost),
    );
    let job = lock(&journal).unfinished().unwrap().remove(0);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    assert_eq!(lock(&journal).read_job(job.id).unwrap().0, JobState::Done);
    for entry in entries(&dirs) {
        assert_eq!(entry.state, RetainedState::Ready);
    }
}

#[test]
fn maintenance_uses_the_new_cap_if_settings_change_while_a_copy_runs() {
    let (_root, dirs, journal) = setup();
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-overwrites = true\nmax-volume-mib = 1\nmin-free-mib = 0\n",
    )
    .unwrap();
    let fs = FakeFs::new();
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 2 * 1024 * 1024);
    fs.set_available_space("dest", Some(u64::MAX));
    let plan = overwrite(
        &fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Copy,
    );
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-overwrites = true\nmax-volume-mib = 1024\nmin-free-mib = 0\n",
    )
    .unwrap();
    log.finish(&JobOutcome::Done, &fs).unwrap();
    let saved = entries(&dirs);
    assert_eq!(saved[0].state, RetainedState::Ready);
    assert!(fs.metadata(&saved[0].record.held().unwrap()).is_ok());
}

#[test]
fn returned_previously_adopted_entries_settle_even_after_a_lost_completion_commit() {
    let (_root, dirs, journal, fs, log) = copied_pair();
    let job = lock(&journal).unfinished().unwrap().remove(0);
    let held = super::super::super::held::held_of(&journal, job.id).unwrap();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    let mut vault = Vault::new(db, &dirs.data, &fs).unwrap();
    vault
        .adopt_job(
            &rusty_retention::AdoptJob {
                job: job.id,
                state: JobState::RestorePending,
                files: &held.proved,
                policy: Default::default(),
                created_at: job.created_at,
            },
            &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
        )
        .unwrap();
    drop((vault, log));
    for (file, _) in &held.proved {
        fs.remove_file(&file.original).unwrap();
    }
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection().execute_batch("CREATE TRIGGER refuse_failed BEFORE UPDATE OF state ON job WHEN NEW.state = 'failed' BEGIN SELECT RAISE(ABORT, 'lost commit'); END;").unwrap();
    let first = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert_eq!(first.pending.len(), 1);
    for (file, proof) in &held.proved {
        assert_eq!(
            fs.file_identity(&file.original).unwrap(),
            Some(proof.node.identity)
        );
        assert!(fs.metadata(&file.held).is_err());
    }
    db.connection()
        .execute_batch("DROP TRIGGER refuse_failed")
        .unwrap();
    let again = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(again.pending.is_empty());
    assert_eq!(lock(&journal).read_job(job.id).unwrap().0, JobState::Failed);
    assert!(
        entries(&dirs)
            .iter()
            .all(|entry| entry.state == RetainedState::Restored)
    );
}

#[test]
fn a_terminal_owner_cannot_return_another_jobs_proved_version() {
    let (_root, dirs, journal, fs, mut first) = copied_pair();
    let owner = lock(&journal).unfinished().unwrap().remove(0).id;
    first
        .finish(
            &JobOutcome::Failed {
                seq: 0,
                error: rusty_core::FsError::new(rusty_core::FsErrorKind::Unknown),
            },
            &fs,
        )
        .unwrap();
    drop(first);
    let plan = overwrite(
        &fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Copy,
    );
    let mut second = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert_eq!(execute(second.as_mut(), &plan, &fs), JobOutcome::Done);
    let active = lock(&journal).unfinished().unwrap().remove(0).id;
    let held = super::super::super::held::held_of(&journal, active).unwrap();
    drop(second);
    let (file, proof) = &held.proved[0];
    fs.remove_file(&file.original).unwrap();
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    let mut vault = Vault::new(db, &dirs.data, &fs).unwrap();
    assert!(
        vault
            .adopt_job(
                &rusty_retention::AdoptJob {
                    job: owner,
                    state: JobState::Failed,
                    files: &held.proved,
                    policy: Default::default(),
                    created_at: lock(&journal).read_job(owner).unwrap().1.created_at,
                },
                &mut RetentionRun::new(&CancelToken::new(), &mut |_| {})
            )
            .is_err()
    );
    assert!(fs.metadata(&file.original).is_err());
    assert_eq!(
        fs.file_identity(&file.held).unwrap(),
        Some(proof.node.identity)
    );
}
