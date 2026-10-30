use rusty_core::FsErrorKind;
use rusty_fs::{FakeFs, FakeOp, Fault, Provider, run_job};

use super::*;

fn fake_move(prepare: impl FnOnce(&FakeFs)) -> (tempfile::TempDir, SharedJournal, JobOutcome) {
    let fs = FakeFs::new();
    fs.add_file("src/a.txt", 3);
    fs.add_folder("dst");
    let cancel = CancelToken::new();
    let items = scan(&fs, &[fs.location("src/a.txt")], &cancel).unwrap();
    let folder = fs.location("dst");
    let existing = existing_at(&fs, &items, &folder, &cancel).unwrap();
    let plan = plan(&PlanRequest {
        kind: OpKind::Move,
        items,
        target: Some(folder),
        existing,
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    });
    prepare(&fs);
    let dir = tempfile::tempdir().unwrap();
    let journal = open(&dir.path().join("navo.db")).unwrap();
    let mut log = log_factory(Arc::clone(&journal))(&plan).unwrap();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: &fs,
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    log.finish(&outcome, &fs).unwrap();
    assert!(fs.metadata(&fs.location("dst/a.txt")).is_ok());
    (dir, journal, outcome)
}

fn offered(journal: &SharedJournal) -> bool {
    (crate::rename_hooks::hooks(Some(Arc::clone(journal)), None).last)()
        .unwrap()
        .is_some()
}

#[test]
fn a_move_with_a_durable_identity_is_offered() {
    let (_dir, journal, outcome) = fake_move(|_| {});
    assert_eq!((outcome, offered(&journal)), (JobOutcome::Done, true));
}

#[test]
fn a_move_without_a_durable_identity_runs_but_is_never_offered() {
    let (_dir, journal, outcome) = fake_move(|fs| fs.set_volume_serial("src", None));
    assert_eq!((outcome, offered(&journal)), (JobOutcome::Done, false));
}

#[test]
fn a_move_whose_identity_cannot_be_read_runs_but_is_never_offered() {
    let (_dir, journal, outcome) = fake_move(|fs| {
        let next = fs.calls("src/a.txt", FakeOp::Identity) + 1;
        fs.fault(
            "src/a.txt",
            FakeOp::Identity,
            next,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
    });
    assert_eq!((outcome, offered(&journal)), (JobOutcome::Done, false));
}
