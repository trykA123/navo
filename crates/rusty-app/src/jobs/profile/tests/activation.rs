use super::*;

#[test]
fn default_jobs_never_acquire_a_user_wide_lease() {
    let _isolation = crate::jobs::test_mutation_isolation();
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
        let mut log = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
        check(log.as_mut(), &plan, &fs, &CancelToken::new()).unwrap();
        let other = super::super::super::mutation_lock::test_file();
        assert!(
            other.try_lock().is_ok(),
            "default {:?} held a global lease",
            plan.kind
        );
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
fn default_writers_and_recovery_ignore_an_exclusive_experimental_lease() {
    let _isolation = crate::jobs::test_mutation_isolation();
    let (root, dirs, _journal) = setup();
    let exclusive = crate::jobs::test_mutation_file();
    exclusive.try_lock().unwrap();
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    assert!(claim_write(&dirs, &target, false).unwrap().is_none());
    let _create = write_factory(dirs.clone(), false)(&target).unwrap();
    LocalFs::new().create_file(&target).unwrap();
    assert!(crate::jobs::claim_recovery(false).unwrap().is_none());
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
fn default_rename_undo_never_acquires_a_user_wide_lease() {
    let _isolation = crate::jobs::test_mutation_isolation();
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
        .finish(job, rusty_store::JobState::Done)
        .unwrap();
    let mut undo = crate::jobs::profile_undo(&journal, job, &dirs, false).unwrap();
    check(undo.as_mut(), &plan, &fs, &CancelToken::new()).unwrap();
    assert!(crate::jobs::test_mutation_file().try_lock().is_ok());
}
