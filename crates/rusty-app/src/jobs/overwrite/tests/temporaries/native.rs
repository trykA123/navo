use super::*;

fn at(path: &std::path::Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn native_cancel_leaves_only_a_proved_partial_and_recovery_cleans_it_before_retry() {
    let (_root, dirs, journal) = setup();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("source");
    let dest = files.path().join("dest");
    std::fs::write(&source, vec![7; 4 << 20]).unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("source"), b"old bytes").unwrap();
    let fs = LocalFs::new();
    let plan = overwrite(&fs, vec![at(&source)], at(&dest), OpKind::Copy);
    let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    let outcome = cancel_copy(log.as_mut(), &plan, &fs);
    assert_eq!(outcome, JobOutcome::Cancelled);
    let job = lock(&journal).unfinished().unwrap().remove(0);
    let proof: rusty_core::retention::CopyTemporary =
        serde_json::from_slice(job.steps[0].detail.as_deref().unwrap()).unwrap();
    assert!(proof.identity.is_some());
    #[cfg(not(windows))]
    assert!(fs.metadata(&proof.temporary).is_ok());
    #[cfg(windows)]
    assert_eq!(
        fs.metadata(&proof.temporary).unwrap_err().kind,
        rusty_core::FsErrorKind::NotFound
    );
    drop((log, journal));
    let journal = open(&dirs.journal_file()).unwrap();
    recover_profile(&journal, &fs, (&dirs.data, &[]), true).unwrap();
    assert_eq!(
        fs.metadata(&proof.temporary).unwrap_err().kind,
        rusty_core::FsErrorKind::NotFound
    );
    assert_eq!(std::fs::read(dest.join("source")).unwrap(), b"old bytes");
    let mut retry = profile_factory(journal, dirs.clone())(&plan).unwrap();
    assert_eq!(execute(retry.as_mut(), &plan, &fs), JobOutcome::Done);
    retry.finish(&JobOutcome::Done, &fs).unwrap();
    assert_eq!(
        std::fs::read(dest.join("source")).unwrap(),
        vec![7; 4 << 20]
    );
    assert_eq!(entries(&dirs).len(), 1);
}

#[cfg(windows)]
#[test]
fn retained_copy_then_remove_move_preserves_new_and_old_alternate_streams() {
    let (_root, dirs, journal) = setup();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("source");
    let dest = files.path().join("dest");
    std::fs::write(&source, b"new data").unwrap();
    std::fs::write(files.path().join("source:details"), b"new stream").unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("source"), b"old data").unwrap();
    std::fs::write(dest.join("source:details"), b"old stream").unwrap();
    let fs = LocalFs::new();
    let items = scan(&fs, &[at(&source)], &CancelToken::new()).unwrap();
    let existing = existing_at(&fs, &items, &at(&dest), &CancelToken::new()).unwrap();
    let plan = plan(&PlanRequest {
        kind: OpKind::Move,
        items,
        target: Some(at(&dest)),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: false,
        protected: vec![],
    });
    assert!(
        plan.steps
            .iter()
            .any(|step| matches!(step, rusty_core::Step::CopyFile { .. }))
    );
    let mut log = profile_factory(journal, dirs.clone())(&plan).unwrap();
    assert_eq!(execute(log.as_mut(), &plan, &fs), JobOutcome::Done);
    log.finish(&JobOutcome::Done, &fs).unwrap();
    assert!(!source.exists());
    assert_eq!(
        std::fs::read(dest.join("source:details")).unwrap(),
        b"new stream"
    );
    let saved = entries(&dirs);
    assert_eq!(saved.len(), 1);
    let held = saved[0].record.held().unwrap().path().to_text();
    assert_eq!(
        std::fs::read(format!("{held}:details")).unwrap(),
        b"old stream"
    );
}

fn cancel_copy(log: &mut dyn OpLog, plan: &OperationPlan, fs: &LocalFs) -> JobOutcome {
    let cancel = CancelToken::new();
    log.check(LoggedOperation {
        plan,
        ops: fs,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
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
            progress: &mut |progress| {
                if progress.bytes_done > 0 {
                    cancel.cancel();
                }
            },
        },
    )
}
