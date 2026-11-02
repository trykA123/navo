use super::*;

fn check_without_waiting(log: &mut dyn OpLog, plan: &OperationPlan, fs: &FakeFs) {
    log.check(LoggedOperation {
        plan,
        ops: fs,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |waiting| assert!(!waiting, "default jobs must ignore the lease"),
        retain_deletes: None,
    })
    .unwrap();
}

#[test]
fn default_jobs_ignore_the_experimental_lease() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let copy = copy(&fs);
    let plans = [
        copy.clone(),
        OperationPlan {
            kind: OpKind::Rename,
            steps: vec![Step::Rename {
                source: fs.location("source/a"),
                target: fs.location("source/b"),
            }],
            ..copy.clone()
        },
        OperationPlan {
            kind: OpKind::Recycle,
            steps: vec![Step::Recycle {
                location: fs.location("source/a"),
            }],
            ..copy.clone()
        },
        OperationPlan {
            kind: OpKind::Move,
            steps: vec![Step::RemoveFolder {
                location: fs.location("dest"),
            }],
            ..copy
        },
    ];
    for plan in plans {
        for factory in [
            profile_factory(journal.clone(), dirs.clone()),
            crate::jobs::guard_factory(Some(dirs.clone())),
        ] {
            let mut log = factory(&plan).unwrap();
            let other = super::super::super::mutation_lock::test_file(&dirs.data);
            other.try_lock().unwrap();
            check_without_waiting(log.as_mut(), &plan, &fs);
            drop(other);
            assert!(
                crate::jobs::test_mutation_file(&dirs.data)
                    .try_lock()
                    .is_ok(),
                "default {:?} acquired the experimental lease",
                plan.kind
            );
        }
    }
}

#[test]
fn default_guard_ignores_unavailable_profile_directories_and_database() {
    let (root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let plan = copy(&fs);
    std::fs::rename(&dirs.config, root.path().join("parked-config")).unwrap();
    let mut log = profile_factory(journal, dirs)(&plan).unwrap();
    check(log.as_mut(), &plan, &fs, &CancelToken::new()).unwrap();
}

#[test]
fn default_writers_ignore_an_exclusive_experimental_lease() {
    let (root, dirs, _journal) = setup();
    let exclusive = crate::jobs::test_mutation_file(&dirs.data);
    exclusive.try_lock().unwrap();
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    let _save = write_lease(&dirs, &target, false, waiting::short()).unwrap();
    assert!(claim_write(&dirs, &target, false).unwrap().is_some());
    let _create = write_factory(dirs.clone(), false)(&target).unwrap();
    LocalFs::new().create_file(&target).unwrap();
    drop(exclusive);
    assert!(
        crate::jobs::test_mutation_file(&dirs.data)
            .try_lock()
            .is_ok()
    );
    assert!(
        crate::jobs::claim_recovery(&dirs.data, false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn default_guard_does_not_open_an_unavailable_catalog() {
    let (root, dirs, journal) = setup();
    let dirs = StoreDirs {
        config: dirs.config,
        data: root.path().join("unavailable-data"),
    };
    std::fs::create_dir_all(dirs.journal_file()).unwrap();
    let fs = FakeFs::new();
    let plan = copy(&fs);
    let mut log = profile_factory(journal, dirs.clone())(&plan).unwrap();
    check(log.as_mut(), &plan, &fs, &CancelToken::new()).unwrap();
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    claim_write(&dirs, &target, false).unwrap();
}

#[test]
fn experimental_mode_protects_reserved_directories() {
    let (root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let mut plan = copy(&fs);
    for name in [".ferrite-retained-ab", ".ferrite-held-ab"] {
        let target =
            crate::startup::path_location(&root.path().join(name).join("new.txt")).unwrap();
        plan.steps = vec![Step::MakeFolder {
            target: target.clone(),
        }];
        assert!(experimental_factory(journal.clone(), dirs.clone())(&plan).is_err());
        assert!(claim_write(&dirs, &target, true).is_err());
    }
}

#[test]
fn default_rename_undo_ignores_the_experimental_lease() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    let mut plan = copy(&fs);
    plan.kind = OpKind::Rename;
    plan.steps = vec![Step::Rename {
        source: fs.location("source/a"),
        target: fs.location("source/b"),
    }];
    let job = crate::jobs::lock(&journal)
        .create_job("rename", &serde_json::to_vec(&plan).unwrap(), 1, 0)
        .unwrap();
    crate::jobs::lock(&journal)
        .mark_step(job, 0, navo_store::StepState::Done, None)
        .unwrap();
    crate::jobs::lock(&journal)
        .finish(job, navo_store::JobState::Done)
        .unwrap();
    let mut undo = crate::jobs::profile_undo(&journal, job, &dirs, false).unwrap();
    let exclusive = crate::jobs::test_mutation_file(&dirs.data);
    exclusive.try_lock().unwrap();
    check_without_waiting(undo.as_mut(), &plan, &fs);
    drop(exclusive);
    assert!(
        crate::jobs::test_mutation_file(&dirs.data)
            .try_lock()
            .is_ok()
    );
}
