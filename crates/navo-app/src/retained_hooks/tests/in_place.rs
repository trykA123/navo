use super::*;
use navo_core::FsErrorKind;
use navo_fs::{FakeOp, Fault, JobContext, existing_at, run_job_parallel};

fn overwrite_profile() -> Profile {
    let profile = Profile::new();
    std::fs::write(
        profile.dirs.settings_file(),
        "[retention]\nretain-deletes = true\nretain-overwrites = true\n",
    )
    .unwrap();
    profile
}

fn overwrite_job(profile: &Profile, kind: OpKind, source: &str, target_dir: &str) -> RetentionId {
    let ops = profile.fs.as_ref() as &dyn ProviderOps;
    let sources = [profile.at(source)];
    let target = profile.at(target_dir);
    let items = scan(ops, &sources, &CancelToken::new()).unwrap();
    let existing = existing_at(ops, &items, &target, &CancelToken::new()).unwrap();
    let job_plan = plan(&PlanRequest {
        kind,
        items,
        target: Some(target),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: vec![],
    });
    let mut log = experimental_factory(profile.journal.clone(), profile.dirs.clone())(&job_plan)
        .expect("overwrite log");
    let cancel = CancelToken::new();
    log.check(LoggedOperation {
        plan: &job_plan,
        ops,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    let outcome = run_job_parallel(
        &job_plan,
        &[],
        &mut JobContext {
            ops,
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| {},
        },
        2,
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&JobOutcome::Done, ops).unwrap();
    profile.hooks().list(None, 10).unwrap()[0].id
}

#[test]
fn undo_of_an_overwriting_copy_restores_in_place() {
    let profile = overwrite_profile();
    profile.fs.add_file("home/a", 8);
    profile.fs.add_file("src/a", 16);
    let original_id = profile.fs.file_identity(&profile.at("home/a")).unwrap();
    let id = overwrite_job(&profile, OpKind::Copy, "src/a", "home");
    assert_eq!(profile.fs.metadata(&profile.at("home/a")).unwrap().size, 16);

    let landed = profile.hooks().restore(id).unwrap();
    assert_eq!(landed, profile.at("home/a"));
    assert_eq!(profile.fs.metadata(&profile.at("home/a")).unwrap().size, 8);
    assert_eq!(
        profile.fs.file_identity(&profile.at("home/a")).unwrap(),
        original_id
    );
    assert_eq!(profile.fs.metadata(&profile.at("src/a")).unwrap().size, 16);
    assert!(!profile.exists("home/a (restored)"));
}

#[test]
fn edited_copy_restores_beside_and_keeps_the_edit() {
    let profile = overwrite_profile();
    profile.fs.add_file("home/a", 8);
    profile.fs.add_file("src/a", 16);
    let id = overwrite_job(&profile, OpKind::Copy, "src/a", "home");
    profile.fs.remove("home/a");
    profile.fs.add_file("home/a", 24);

    let landed = profile.hooks().restore(id).unwrap();
    assert_eq!(landed, profile.at("home/a (restored)"));
    assert_eq!(profile.fs.metadata(&profile.at("home/a")).unwrap().size, 24);
    assert_eq!(
        profile
            .fs
            .metadata(&profile.at("home/a (restored)"))
            .unwrap()
            .size,
        8
    );
}

#[test]
fn deleted_copy_source_restores_beside() {
    let profile = overwrite_profile();
    profile.fs.add_file("home/a", 8);
    profile.fs.add_file("src/a", 16);
    let id = overwrite_job(&profile, OpKind::Copy, "src/a", "home");
    profile.fs.remove("src/a");

    let landed = profile.hooks().restore(id).unwrap();
    assert_eq!(landed, profile.at("home/a (restored)"));
    assert_eq!(profile.fs.metadata(&profile.at("home/a")).unwrap().size, 16);
    assert_eq!(
        profile
            .fs
            .metadata(&profile.at("home/a (restored)"))
            .unwrap()
            .size,
        8
    );
}

#[test]
fn locked_copy_at_step_2_falls_back_to_beside() {
    let profile = overwrite_profile();
    profile.fs.add_file("home/a", 8);
    profile.fs.add_file("src/a", 16);
    let id = overwrite_job(&profile, OpKind::Copy, "src/a", "home");
    let next = profile.fs.calls("home/a", FakeOp::Rename) + 1;
    profile.fs.fault(
        "home/a",
        FakeOp::Rename,
        next,
        Fault::Fail(FsErrorKind::Locked),
    );

    let landed = profile.hooks().restore(id).unwrap();
    assert_eq!(landed, profile.at("home/a (restored)"));
    assert_eq!(profile.fs.metadata(&profile.at("home/a")).unwrap().size, 16);
    assert_eq!(
        profile
            .fs
            .metadata(&profile.at("home/a (restored)"))
            .unwrap()
            .size,
        8
    );
}

#[test]
fn overwriting_same_drive_move_still_restores_beside() {
    let profile = overwrite_profile();
    profile.fs.add_file("home/a", 8);
    profile.fs.add_file("src/a", 16);
    let id = overwrite_job(&profile, OpKind::Move, "src/a", "home");

    let landed = profile.hooks().restore(id).unwrap();
    assert_eq!(landed, profile.at("home/a (restored)"));
    assert_eq!(profile.fs.metadata(&profile.at("home/a")).unwrap().size, 16);
    assert!(!profile.exists("src/a"));
}
