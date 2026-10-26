use super::*;

fn single(fs: &FakeFs) -> OperationPlan {
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 2);
    overwrite(
        fs,
        vec![fs.location("source/a")],
        fs.location("dest"),
        OpKind::Copy,
    )
}

fn no_partial(fs: &FakeFs) {
    let files = scan(fs, &[fs.location("dest")], &CancelToken::new()).unwrap();
    assert!(
        files.iter().all(|item| !item
            .location
            .name()
            .is_some_and(|name| name.to_text().contains(rusty_fs::TEMP_SUFFIX))),
        "a partial copy remains"
    );
}

#[test]
fn review_a_failed_retained_copy_can_be_retried() {
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
    let mut first = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let outcome = execute(first.as_mut(), &plan, &fs);
    assert!(matches!(outcome, JobOutcome::Failed { .. }));
    first.finish(&outcome, &fs).unwrap();
    drop(first);
    no_partial(&fs);
    let mut second = profile_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(second.as_mut(), &plan, &fs), JobOutcome::Done);
    second.finish(&JobOutcome::Done, &fs).unwrap();
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 7);
}

#[test]
fn cancellation_cleans_the_owned_partial_and_allows_another_retained_copy() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = single(&fs);
    let mut first = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    fs.fault(
        "source/a",
        rusty_fs::FakeOp::Copy,
        1,
        rusty_fs::Fault::FailAfterBytes {
            kind: rusty_core::FsErrorKind::Cancelled,
            bytes: 3,
        },
    );
    let outcome = execute(first.as_mut(), &plan, &fs);
    assert_eq!(outcome, JobOutcome::Cancelled);
    first.finish(&outcome, &fs).unwrap();
    drop(first);
    no_partial(&fs);
    let mut second = profile_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(second.as_mut(), &plan, &fs), JobOutcome::Done);
    second.finish(&JobOutcome::Done, &fs).unwrap();
}

#[test]
fn startup_cleans_a_proved_mid_copy_partial_before_another_attempt() {
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
    let mut interrupted = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    assert!(matches!(
        execute(interrupted.as_mut(), &plan, &fs),
        JobOutcome::Failed { .. }
    ));
    drop(interrupted);
    let _report = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    no_partial(&fs);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 2);
    let mut second = profile_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(second.as_mut(), &plan, &fs), JobOutcome::Done);
    second.finish(&JobOutcome::Done, &fs).unwrap();
}

struct Interrupted {
    dirs: StoreDirs,
    journal: SharedJournal,
    fs: FakeFs,
    plan: OperationPlan,
    log: Box<dyn OpLog>,
    proof: rusty_core::retention::CopyTemporary,
    _root: tempfile::TempDir,
}

impl Interrupted {
    fn new() -> Self {
        let (root, dirs, journal) = setup();
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
        assert!(matches!(
            execute(log.as_mut(), &plan, &fs),
            JobOutcome::Failed { .. }
        ));
        let job = lock(&journal).unfinished().unwrap().remove(0);
        let proof = serde_json::from_slice(job.steps[0].detail.as_deref().unwrap()).unwrap();
        Self {
            _root: root,
            dirs,
            journal,
            fs,
            plan,
            log,
            proof,
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        self.log.finish(
            &JobOutcome::Failed {
                seq: 0,
                error: rusty_core::FsError::new(rusty_core::FsErrorKind::Unknown),
            },
            &self.fs,
        )
    }
}

fn key(location: &Location) -> String {
    location.path().to_text().trim_start_matches('/').to_owned()
}

#[test]
fn a_replaced_temporary_survives_failure_recovery_and_another_attempt() {
    let mut interrupted = Interrupted::new();
    let temp = interrupted.proof.temporary.clone();
    interrupted
        .fs
        .rename(&temp, &interrupted.fs.location("dest/owned-partial"))
        .unwrap();
    interrupted.fs.add_file(&key(&temp), 99);
    assert!(interrupted.finish().is_err());
    assert_eq!(interrupted.fs.metadata(&temp).unwrap().size, 99);
    let Interrupted {
        _root,
        dirs,
        journal,
        fs,
        plan,
        log,
        ..
    } = interrupted;
    drop(log);
    let report = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(
        report
            .pending
            .iter()
            .any(|pending| pending.left.contains(&temp))
    );
    let mut retry = profile_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(retry.as_mut(), &plan, &fs), JobOutcome::Done);
    retry.finish(&JobOutcome::Done, &fs).unwrap();
    assert_eq!(fs.metadata(&temp).unwrap().size, 99);
    assert_eq!(
        fs.metadata(&fs.location("dest/owned-partial"))
            .unwrap()
            .size,
        3
    );
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 7);
}

#[test]
fn a_changed_parent_preserves_the_moved_partial_and_the_replacement_tree() {
    let mut interrupted = Interrupted::new();
    let temp = interrupted.proof.temporary.clone();
    let name = temp.name().unwrap();
    interrupted
        .fs
        .rename(
            &interrupted.fs.location("dest"),
            &interrupted.fs.location("moved"),
        )
        .unwrap();
    interrupted.fs.add_folder("dest");
    interrupted.fs.add_file(&key(&temp), 99);
    assert!(interrupted.finish().is_err());
    assert_eq!(interrupted.fs.metadata(&temp).unwrap().size, 99);
    assert_eq!(
        interrupted
            .fs
            .metadata(&interrupted.fs.location("moved").join(&name).unwrap())
            .unwrap()
            .size,
        3
    );
    assert_eq!(
        interrupted
            .fs
            .metadata(&interrupted.fs.location("moved/a"))
            .unwrap()
            .size,
        2
    );
}

#[test]
fn an_unproved_create_gap_remains_pending_and_does_not_poison_the_next_namespace() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = single(&fs);
    let mut interrupted = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let job = lock(&journal).unfinished().unwrap().remove(0);
    let owner: rusty_core::retention::RetainedWritePlan =
        serde_json::from_slice(&job.plan).unwrap();
    let temp = rusty_core::retention::copy_temporary(
        &fs.location("dest/a"),
        0,
        owner.temporary_namespace(),
    )
    .unwrap();
    fs.fault(
        &key(&temp),
        rusty_fs::FakeOp::Identity,
        1,
        rusty_fs::Fault::Fail(rusty_core::FsErrorKind::Unknown),
    );
    let failed = execute(interrupted.as_mut(), &plan, &fs);
    assert!(matches!(failed, JobOutcome::Failed { .. }));
    assert!(interrupted.finish(&failed, &fs).is_err());
    drop(interrupted);
    assert_eq!(fs.metadata(&temp).unwrap().size, 0);
    let report = recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert!(
        report
            .pending
            .iter()
            .any(|pending| pending.left.contains(&temp))
    );
    let mut retry = profile_factory(journal, dirs)(&plan).unwrap();
    assert_eq!(execute(retry.as_mut(), &plan, &fs), JobOutcome::Done);
    retry.finish(&JobOutcome::Done, &fs).unwrap();
    assert_eq!(fs.metadata(&temp).unwrap().size, 0);
}

#[test]
fn identity_registration_failure_leaves_no_unregistered_copied_bytes() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = single(&fs);
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection().execute_batch("CREATE TRIGGER refuse_created_temporary BEFORE UPDATE OF detail ON job_step WHEN NEW.detail IS NOT NULL AND json_valid(NEW.detail) AND json_extract(NEW.detail, '$.identity') IS NOT NULL BEGIN SELECT RAISE(ABORT, 'registration refused'); END;").unwrap();
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let failed = execute(log.as_mut(), &plan, &fs);
    assert!(matches!(failed, JobOutcome::Failed { .. }));
    assert!(log.finish(&failed, &fs).is_err());
    let job = lock(&journal).restore_pending().unwrap().remove(0);
    let proof: rusty_core::retention::CopyTemporary =
        serde_json::from_slice(job.steps[0].detail.as_deref().unwrap()).unwrap();
    assert!(proof.identity.is_none());
    assert_eq!(fs.metadata(&proof.temporary).unwrap().size, 0);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 2);
}

mod coordination;
mod recovery;

mod native;

fn strand_original(interrupted: &mut Interrupted) -> rusty_core::retention::OverwriteProof {
    let fs = &interrupted.fs;
    let original = fs.location("dest/a");
    let old = fs.metadata(&original).unwrap();
    let parent = fs.file_identity(&fs.location("dest")).unwrap().unwrap();
    fs.add_folder("dest/.ferrite-held-ab");
    let holder = fs.location("dest/.ferrite-held-ab");
    let held = rusty_fs::HeldFile {
        original: original.clone(),
        held: fs.location("dest/.ferrite-held-ab/a"),
        placed: Some(rusty_fs::FileStamp {
            size: 7,
            modified_ms: None,
        }),
        volume: Some(parent.volume),
    };
    let evidence = rusty_core::retention::OverwriteProof {
        parent_identity: parent,
        container_identity: fs.file_identity(&holder).unwrap().unwrap(),
        mount_identity: fs.mount_identity(&original).unwrap().unwrap(),
        node: rusty_core::retention::RetainedNode {
            path: vec![],
            kind: rusty_core::retention::RetainedKind::File,
            identity: fs.file_identity(&original).unwrap().unwrap(),
            size: old.size,
            modified_ms: old.modified_ms,
        },
        replacement_identity: fs
            .file_identity(&interrupted.proof.temporary)
            .unwrap()
            .unwrap(),
    };
    assert!(
        interrupted
            .log
            .journal_proved_held(&held, &evidence)
            .unwrap()
    );
    fs.rename_atomic(&original, &held.held).unwrap();
    evidence
}

#[test]
fn crash_before_replacement_returns_the_proved_original_and_finishes_recovery() {
    let mut interrupted = Interrupted::new();
    let evidence = strand_original(&mut interrupted);
    let fs = &interrupted.fs;
    let original = fs.location("dest/a");
    let holder = fs.location("dest/.ferrite-held-ab");
    let job = lock(&interrupted.journal).unfinished().unwrap().remove(0);
    drop(interrupted.log);
    let report = recover_profile(
        &interrupted.journal,
        fs,
        (&interrupted.dirs.data, &[]),
        true,
    )
    .unwrap();
    assert_eq!(
        fs.file_identity(&original).unwrap(),
        Some(evidence.node.identity)
    );
    assert_eq!(fs.metadata(&original).unwrap().size, 2);
    assert!(report.pending.is_empty());
    assert_eq!(
        lock(&interrupted.journal).read_job(job.id).unwrap().0,
        JobState::Failed
    );
    assert!(lock(&interrupted.journal).held(job.id).unwrap().is_empty());
    assert!(fs.metadata(&holder).is_err());
    no_partial(fs);
    let again = recover_profile(
        &interrupted.journal,
        fs,
        (&interrupted.dirs.data, &[]),
        true,
    )
    .unwrap();
    assert_eq!(again.unfinished, 0);
    assert!(again.pending.is_empty());
}

#[test]
fn an_invalid_overwrite_owner_keeps_stranded_proof_pending_without_a_move() {
    let mut interrupted = Interrupted::new();
    let evidence = strand_original(&mut interrupted);
    let job = lock(&interrupted.journal).unfinished().unwrap().remove(0);
    drop(interrupted.log);
    let (db, _) = Db::open(&interrupted.dirs.journal_file(), false).unwrap();
    let mut plan: serde_json::Value = serde_json::from_slice(&job.plan).unwrap();
    plan["conflicts"] = serde_json::json!([]);
    db.connection()
        .execute(
            "UPDATE job SET plan = ?1 WHERE id = ?2",
            (serde_json::to_vec(&plan).unwrap(), job.id.0),
        )
        .unwrap();
    let report = recover_profile(
        &interrupted.journal,
        &interrupted.fs,
        (&interrupted.dirs.data, &[]),
        true,
    )
    .unwrap();
    assert!(
        interrupted
            .fs
            .metadata(&interrupted.fs.location("dest/a"))
            .is_err()
    );
    assert_eq!(
        interrupted
            .fs
            .file_identity(&interrupted.fs.location("dest/.ferrite-held-ab/a"))
            .unwrap(),
        Some(evidence.node.identity)
    );
    assert_eq!(report.pending.len(), 1);
    assert_eq!(lock(&interrupted.journal).held(job.id).unwrap().len(), 1);
}
