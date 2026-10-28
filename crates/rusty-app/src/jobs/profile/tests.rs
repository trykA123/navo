use super::*;
use rusty_core::{ConflictPolicy, PlanRequest, ProviderPath, Step, plan};
use rusty_fs::{CancelToken, FakeFs, LocalFs, Provider, ProviderOps, scan};
use rusty_ui::browser::LoggedOperation;

fn setup() -> (tempfile::TempDir, StoreDirs, SharedJournal) {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    dirs.ensure().unwrap();
    let journal = super::super::open(&dirs.journal_file()).unwrap();
    (root, dirs, journal)
}

fn copy(fs: &FakeFs) -> OperationPlan {
    fs.add_file("source/a", 7);
    fs.add_folder("dest");
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items: scan(fs, &[fs.location("source/a")], &CancelToken::new()).unwrap(),
        target: Some(fs.location("dest")),
        existing: vec![],
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: vec![],
    })
}

fn check(
    log: &mut dyn OpLog,
    plan: &OperationPlan,
    fs: &FakeFs,
    cancel: &CancelToken,
) -> Result<(), String> {
    log.check(LoggedOperation {
        plan,
        ops: fs,
        cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
}

#[test]
fn ordinary_copy_jobs_save_and_f7_share_access_with_invalid_settings() {
    let (root, dirs, journal) = setup();
    std::fs::write(
        dirs.settings_file(),
        "[automotive]\nmodules = [1]\n[retention]\ndays = 8\n",
    )
    .unwrap();
    let fs = FakeFs::new();
    let plan = copy(&fs);
    let factory = profile_factory(journal, dirs.clone());
    let mut first = factory(&plan).unwrap();
    let mut second = factory(&plan).unwrap();
    let cancel = CancelToken::new();
    check(first.as_mut(), &plan, &fs, &cancel).unwrap();
    check(second.as_mut(), &plan, &fs, &cancel).unwrap();
    let target = Location::local(ProviderPath::from_text(
        root.path().join("new.txt").to_str().unwrap(),
    ))
    .unwrap();
    let _save = claim_write(&dirs, &target, false).unwrap();
    let _f7 = write_factory(dirs, false)(&target).unwrap();
    LocalFs::new().create_file(&target).unwrap();
    assert_eq!(LocalFs::new().metadata(&target).unwrap().size, 0);
}

#[test]
fn overwrites_are_retained_by_default_only_outside_safe_mode() {
    for settings in [None, Some("[retention]\nretain-overwrites = true\n")] {
        let (_root, dirs, journal) = setup();
        if let Some(text) = settings {
            std::fs::write(dirs.settings_file(), text).unwrap();
        }
        let fs = FakeFs::new();
        let plan = copy(&fs);
        let safe = profile_factory(journal.clone(), dirs.clone())(&plan).unwrap();
        assert!(!safe.preserve_overwrites(), "{settings:?}");
        let coordinated = experimental_factory(journal, dirs)(&plan).unwrap();
        assert!(coordinated.preserve_overwrites(), "{settings:?}");
    }
}

#[test]
fn queued_retention_can_cancel_behind_an_experimental_shared_job() {
    let (_root, dirs, journal) = setup();
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-overwrites = true\n",
    )
    .unwrap();
    let fs = FakeFs::new();
    let plan = copy(&fs);
    std::fs::write(dirs.settings_file(), "").unwrap();
    let mut ordinary = experimental_factory(journal.clone(), dirs.clone())(&plan).unwrap();
    check(ordinary.as_mut(), &plan, &fs, &CancelToken::new()).unwrap();
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-overwrites = true\n",
    )
    .unwrap();
    let mut retained = experimental_factory(journal, dirs)(&plan).unwrap();
    let cancel = CancelToken::new();
    let worker_cancel = cancel.clone();
    let worker = std::thread::spawn(move || check(retained.as_mut(), &plan, &fs, &worker_cancel));
    cancel.cancel();
    assert!(worker.join().unwrap().is_err());
}

#[test]
fn structural_renames_exclude_writers_until_the_holder_cleanup_finishes() {
    let (root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_folder("dest");
    let mut plan = copy(&fs);
    plan.kind = OpKind::Rename;
    plan.steps = vec![Step::Rename {
        source: fs.location("dest"),
        target: fs.location("moved"),
    }];
    let mut log = experimental_factory(journal, dirs.clone())(&plan).unwrap();
    check(log.as_mut(), &plan, &fs, &CancelToken::new()).unwrap();
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    assert_eq!(
        write_lease(&dirs, &target, true, waiting::short()).unwrap_err(),
        LeaseError::Busy
    );
    drop(log);
    assert!(claim_write(&dirs, &target, true).is_ok());
}

mod broken_settings;

mod large_delete;

mod activation;

mod main_compat;

mod safe_mode;

mod waiting;

mod cache;
