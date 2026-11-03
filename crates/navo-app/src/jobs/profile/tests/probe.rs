use super::*;
use navo_core::FsErrorKind;
use navo_fs::{FakeOp, Fault};
use navo_ui::browser::LogRefusal;

fn overwrite(fs: &FakeFs) -> OperationPlan {
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 3);
    let cancel = CancelToken::new();
    let items = scan(fs, &[fs.location("source/a")], &cancel).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        existing: navo_fs::existing_at(fs, &items, &fs.location("dest"), &cancel).unwrap(),
        items,
        target: Some(fs.location("dest")),
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: vec![],
    })
}

fn fail_identity(fs: &FakeFs, path: &str, calls: std::ops::RangeInclusive<u32>) {
    for call in calls {
        fs.fault(
            path,
            FakeOp::Identity,
            call,
            Fault::Fail(FsErrorKind::Timeout),
        );
    }
}

fn retained_write(kind: OpKind, fs: &FakeFs) -> OperationPlan {
    let mut plan = overwrite(fs);
    plan.kind = kind;
    if kind == OpKind::Save {
        plan = navo_core::write_plan(
            &fs.location("dest/a"),
            &fs.location("source/a"),
            navo_core::FileStamp {
                size: 3,
                modified_ms: None,
                hash: [0; 32],
            },
            7,
        );
    }
    plan
}

const WRITES: [(OpKind, &str); 4] = [
    (OpKind::Copy, "dest/a"),
    (OpKind::Copy, "dest"),
    (OpKind::Move, "dest/a"),
    (OpKind::Save, "dest/a"),
];

#[test]
fn retention_probe_retries_a_transient_failure_and_keeps_retention() {
    for (kind, path) in WRITES {
        let (_root, dirs, journal) = setup();
        let fs = Arc::new(FakeFs::new());
        let plan = retained_write(kind, &fs);
        fail_identity(&fs, path, 1..=2);
        let log = retaining_factory(journal, dirs, fs.clone())(&plan).unwrap();
        assert!(log.preserve_overwrites(), "{kind:?} at {path}");
    }
}

#[test]
fn retention_probe_that_keeps_failing_refuses_the_job_as_unknown() {
    for (kind, path) in WRITES {
        let (_root, dirs, journal) = setup();
        let fs = Arc::new(FakeFs::new());
        let plan = retained_write(kind, &fs);
        fail_identity(&fs, path, 1..=3);
        let factory = retaining_factory(journal.clone(), dirs, fs.clone());
        assert_eq!(
            factory(&plan).err(),
            Some(LogRefusal::RetentionUnknown),
            "{kind:?} at {path}"
        );
        assert!(
            super::super::super::lock(&journal)
                .unfinished()
                .unwrap()
                .is_empty(),
            "a refused job leaves no journal entry"
        );
        assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 3);
        assert!(factory(&plan).unwrap().preserve_overwrites());
    }
}

#[test]
fn a_job_the_user_runs_without_retention_skips_the_probe() {
    for (kind, path) in WRITES {
        let (_root, dirs, journal) = setup();
        let fs = Arc::new(FakeFs::new());
        let plan = retained_write(kind, &fs);
        fail_identity(&fs, path, 1..=10);
        let log = declined_factory(journal, dirs)(&plan).unwrap();
        assert!(!log.preserve_overwrites(), "{kind:?} at {path}");
        assert_eq!(fs.calls(path, FakeOp::Identity), 0);
    }
}

#[test]
fn retention_probe_failure_does_not_treat_a_missing_parent_as_unsupported() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = overwrite(&fs);
    for call in 1..=3 {
        fs.fault(
            "dest",
            FakeOp::Identity,
            call,
            Fault::Fail(FsErrorKind::NotFound),
        );
    }
    let refused = retaining_factory(journal, dirs, fs)(&plan);
    assert_eq!(refused.err(), Some(LogRefusal::RetentionUnknown));
}

#[test]
fn retention_probe_without_identity_uses_ordinary_mode() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = overwrite(&fs);
    fs.set_volume_serial("dest", None);
    let log = retaining_factory(journal, dirs, fs)(&plan).unwrap();
    assert!(!log.preserve_overwrites());
}

#[test]
fn retention_probe_with_identity_keeps_retention_enabled() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = overwrite(&fs);
    let log = retaining_factory(journal, dirs, fs)(&plan).unwrap();
    assert!(log.preserve_overwrites());
}

fn two_overwrites(fs: &FakeFs) -> OperationPlan {
    let mut sources = Vec::new();
    for name in ["a", "b"] {
        fs.add_file(&format!("source/{name}"), 7);
        fs.add_file(&format!("dest/{name}"), 3);
        sources.push(fs.location(&format!("source/{name}")));
    }
    let cancel = CancelToken::new();
    let items = scan(fs, &sources, &cancel).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        existing: navo_fs::existing_at(fs, &items, &fs.location("dest"), &cancel).unwrap(),
        items,
        target: Some(fs.location("dest")),
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: vec![],
    })
}

#[test]
fn retention_probe_skips_a_vanished_overwrite_target_and_probes_the_next() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = two_overwrites(&fs);
    assert_eq!(plan.conflicts.len(), 2, "both targets are overwritten");
    let first = plan.conflicts[0].target.clone();
    let second = plan.conflicts[1].target.clone();
    fs.remove_file(&first).unwrap();
    let log = retaining_factory(journal, dirs, fs.clone())(&plan).unwrap();
    assert!(log.preserve_overwrites());
    assert!(fs.calls(&second.path().to_text(), FakeOp::Identity) > 0);
}

#[test]
fn retention_probe_keeps_retention_when_every_overwrite_target_vanished() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = two_overwrites(&fs);
    for conflict in &plan.conflicts {
        fs.remove_file(&conflict.target).unwrap();
    }
    let log = retaining_factory(journal, dirs, fs)(&plan).unwrap();
    assert!(log.preserve_overwrites());
}

#[test]
fn retention_probe_after_a_vanished_target_still_uses_ordinary_mode_without_identity() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = two_overwrites(&fs);
    fs.remove_file(&plan.conflicts[0].target).unwrap();
    fs.set_volume_serial("dest", None);
    let log = retaining_factory(journal, dirs, fs)(&plan).unwrap();
    assert!(!log.preserve_overwrites());
}

fn retained_delete(fs: &FakeFs, dirs: &StoreDirs) -> OperationPlan {
    std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = true\n").unwrap();
    let mut plan = overwrite(fs);
    plan.kind = OpKind::Delete;
    plan.conflicts.clear();
    plan.steps = vec![Step::RemoveFile {
        location: fs.location("dest/a"),
    }];
    plan
}

fn journals_retained_delete(journal: &SharedJournal) -> bool {
    let job = super::super::super::lock(journal)
        .unfinished()
        .unwrap()
        .remove(0);
    serde_json::from_slice::<navo_core::retention::RetainedDeletePlan>(&job.plan).is_ok()
}

#[test]
fn retention_probe_failure_also_refuses_retained_deletes() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = retained_delete(&fs, &dirs);
    fail_identity(&fs, "dest/a", 1..=3);
    let refused = retaining_factory(journal.clone(), dirs, fs.clone())(&plan);
    assert_eq!(refused.err(), Some(LogRefusal::RetentionUnknown));
    assert!(
        super::super::super::lock(&journal)
            .unfinished()
            .unwrap()
            .is_empty()
    );
    assert!(fs.metadata(&fs.location("dest/a")).is_ok());
}

#[test]
fn retention_probe_retries_a_transient_failure_for_deletes() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = retained_delete(&fs, &dirs);
    fail_identity(&fs, "dest/a", 1..=2);
    let log = retaining_factory(journal.clone(), dirs, fs)(&plan).unwrap();
    assert!(journals_retained_delete(&journal));
    drop(log);
}

#[test]
fn a_delete_the_user_runs_without_retention_is_permanent() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = retained_delete(&fs, &dirs);
    fail_identity(&fs, "dest/a", 1..=10);
    let log = declined_factory(journal.clone(), dirs)(&plan).unwrap();
    assert!(!journals_retained_delete(&journal));
    drop(log);
}

fn delete_two(fs: &FakeFs, dirs: &StoreDirs) -> OperationPlan {
    let mut plan = retained_delete(fs, dirs);
    fs.add_file("dest/b", 3);
    plan.steps.push(Step::RemoveFile {
        location: fs.location("dest/b"),
    });
    plan
}

#[test]
fn retention_probe_skips_a_vanished_delete_root_and_probes_the_next() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = delete_two(&fs, &dirs);
    fs.remove_file(&fs.location("dest/a")).unwrap();
    let log = retaining_factory(journal.clone(), dirs, fs.clone())(&plan).unwrap();
    assert!(journals_retained_delete(&journal));
    assert!(fs.calls("dest/b", FakeOp::Identity) > 0);
    drop(log);
}

#[test]
fn retention_probe_keeps_retention_when_every_delete_root_vanished() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = delete_two(&fs, &dirs);
    fs.remove_file(&fs.location("dest/a")).unwrap();
    fs.remove_file(&fs.location("dest/b")).unwrap();
    let log = retaining_factory(journal.clone(), dirs, fs)(&plan).unwrap();
    assert!(journals_retained_delete(&journal));
    drop(log);
}

const MANY: usize = 40;

fn many(fs: &FakeFs) -> Vec<Location> {
    fs.add_folder("dest");
    (0..MANY)
        .map(|index| {
            fs.add_file(&format!("source/f{index:02}"), 7);
            fs.location(&format!("source/f{index:02}"))
        })
        .collect()
}

fn new_targets_probed(fs: &FakeFs) -> u32 {
    (0..MANY)
        .map(|index| fs.calls(&format!("dest/f{index:02}"), FakeOp::Identity))
        .sum()
}

#[test]
fn retention_probe_of_a_fresh_copy_probes_no_target() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let sources = many(&fs);
    let cancel = CancelToken::new();
    let plan = plan(&PlanRequest {
        kind: OpKind::Copy,
        items: scan(fs.as_ref(), &sources, &cancel).unwrap(),
        target: Some(fs.location("dest")),
        existing: vec![],
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: vec![],
    });
    let log = retaining_factory(journal, dirs, fs.clone())(&plan).unwrap();
    assert!(
        log.preserve_overwrites(),
        "nothing replaced keeps retention"
    );
    assert_eq!(new_targets_probed(&fs), 0);
}

#[test]
fn retention_probe_stops_after_sixteen_new_write_targets() {
    let fs = Arc::new(FakeFs::new());
    let sources = many(&fs);
    let mut plan = retained_write(OpKind::Save, &fs);
    plan.steps = sources
        .iter()
        .enumerate()
        .map(|(index, staged)| Step::WriteFile {
            target: fs.location(&format!("dest/f{index:02}")),
            staged: staged.clone(),
            staged_len: 7,
            expect: navo_core::FileStamp {
                size: 0,
                modified_ms: None,
                hash: [0; 32],
            },
        })
        .collect();
    fail_identity(&fs, "dest/f20", 1..=10);
    let drive = Drive::Probed(fs.clone());
    assert_eq!(
        drive_retains(Some(&drive), &plan),
        Ok(true),
        "nothing found keeps retention"
    );
    assert!(fs.calls("dest/f15", FakeOp::Identity) > 0);
    assert_eq!(
        new_targets_probed(&fs),
        16,
        "one probe per target, none after the cap"
    );
}

fn delete_many(fs: &FakeFs, dirs: &StoreDirs) -> OperationPlan {
    let mut plan = retained_delete(fs, dirs);
    plan.steps = (0..MANY)
        .map(|index| Step::RemoveFile {
            location: fs.location(&format!("dest/f{index:02}")),
        })
        .collect();
    plan
}

#[test]
fn retention_probe_stops_after_sixteen_vanished_delete_roots() {
    let (_root, dirs, journal) = setup();
    let fs = Arc::new(FakeFs::new());
    let plan = delete_many(&fs, &dirs);
    fail_identity(&fs, "dest/f20", 1..=10);
    let log = retaining_factory(journal.clone(), dirs, fs.clone())(&plan).unwrap();
    assert!(journals_retained_delete(&journal));
    assert_eq!(new_targets_probed(&fs), 16);
    drop(log);
}
