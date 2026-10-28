use super::*;
use rusty_fs::existing_at;

const BROKEN: &str = "# mine\n# notes\n[retention]\nretain-deletes = \n";
const FIXED: &str = "# mine\n# notes\n[retention]\nretain-deletes = true\n";

fn broken() -> (tempfile::TempDir, StoreDirs, SharedJournal) {
    let (root, dirs, journal) = setup();
    std::fs::write(dirs.settings_file(), BROKEN).unwrap();
    (root, dirs, journal)
}

fn transfer(fs: &FakeFs, kind: OpKind, policy: ConflictPolicy) -> OperationPlan {
    let items = scan(fs, &[fs.location("source/a")], &CancelToken::new()).unwrap();
    let target = fs.location("dest");
    let existing = existing_at(fs, &items, &target, &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind,
        items,
        target: Some(target),
        existing,
        policy,
        same_volume: true,
        protected: vec![],
    })
}

fn clashing() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("source/a", 7);
    fs.add_file("dest/a", 3);
    fs
}

fn deletion(fs: &FakeFs) -> OperationPlan {
    fs.add_file("gone", 1);
    let mut plan = copy(fs);
    plan.kind = OpKind::Delete;
    plan.steps = vec![Step::RemoveFile {
        location: fs.location("gone"),
    }];
    plan.conflicts.clear();
    plan
}

fn refused(dirs: &StoreDirs, journal: &SharedJournal, plan: &OperationPlan) -> String {
    match experimental_factory(journal.clone(), dirs.clone())(plan) {
        Ok(_) => panic!("{:?} was allowed with a broken navo.toml", plan.kind),
        Err(message) => message,
    }
}

fn names_file_and_line(message: &str, dirs: &StoreDirs) {
    let file = dirs.settings_file().display().to_string();
    assert!(message.contains(&file), "{message}");
    assert!(message.replace(&file, "").contains('4'), "{message}");
}

#[test]
fn a_broken_file_refuses_delete_naming_the_file_and_line() {
    let (_root, dirs, journal) = broken();
    let fs = FakeFs::new();
    let message = refused(&dirs, &journal, &deletion(&fs));
    names_file_and_line(&message, &dirs);
    assert!(
        super::super::super::lock(&journal)
            .unfinished()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_broken_file_refuses_overwriting_copy_and_move() {
    let (_root, dirs, journal) = broken();
    for kind in [OpKind::Copy, OpKind::Move] {
        let fs = clashing();
        let plan = transfer(&fs, kind, ConflictPolicy::OverwriteAll);
        assert!(!plan.conflicts.is_empty());
        names_file_and_line(&refused(&dirs, &journal, &plan), &dirs);
    }
    assert!(
        super::super::super::lock(&journal)
            .unfinished()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_broken_file_allows_plain_copy_move_and_skipped_clashes() {
    let (_root, dirs, journal) = broken();
    let factory = experimental_factory(journal, dirs);
    let cancel = CancelToken::new();
    for kind in [OpKind::Copy, OpKind::Move] {
        let fs = FakeFs::new();
        fs.add_file("source/a", 7);
        fs.add_folder("dest");
        let plan = transfer(&fs, kind, ConflictPolicy::Ask);
        let mut log = factory(&plan).unwrap();
        assert!(!log.preserve_overwrites());
        check(log.as_mut(), &plan, &fs, &cancel).unwrap();
        drop(log);
        let fs = clashing();
        for policy in [ConflictPolicy::Skip, ConflictPolicy::Rename] {
            let plan = transfer(&fs, kind, policy);
            let log = factory(&plan).unwrap();
            drop(log);
        }
    }
}

#[test]
fn a_broken_file_allows_rename_new_file_and_editor_save() {
    let (root, dirs, journal) = broken();
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
    drop(log);
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    let created = write_factory(dirs.clone(), true)(&target).unwrap();
    drop(created);
    assert!(claim_write(&dirs, &target, true).is_ok());
}

#[test]
fn fixing_the_file_lifts_the_block_for_the_next_job() {
    let (_root, dirs, journal) = broken();
    let fs = clashing();
    let plan = transfer(&fs, OpKind::Copy, ConflictPolicy::OverwriteAll);
    let factory = experimental_factory(journal, dirs.clone());
    assert!(factory(&plan).is_err());
    std::fs::write(dirs.settings_file(), FIXED).unwrap();
    let log = factory(&plan).unwrap();
    drop(log);
    let log = factory(&deletion(&FakeFs::new())).unwrap();
    drop(log);
}

#[test]
fn safe_mode_never_reads_the_broken_file() {
    let (_root, dirs, journal) = broken();
    let fs = clashing();
    let factory = profile_factory(journal, dirs);
    let log = factory(&transfer(&fs, OpKind::Copy, ConflictPolicy::OverwriteAll)).unwrap();
    drop(log);
    let log = factory(&deletion(&FakeFs::new())).unwrap();
    drop(log);
}
