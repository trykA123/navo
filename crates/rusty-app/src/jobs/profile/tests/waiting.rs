use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;

pub(super) fn short() -> Wait<'static> {
    Wait {
        limit: Duration::from_millis(100),
        ..Wait::bounded()
    }
}

fn rename(fs: &FakeFs) -> OperationPlan {
    fs.add_folder("dest");
    let mut plan = copy(fs);
    plan.kind = OpKind::Rename;
    plan.steps = vec![Step::Rename {
        source: fs.location("dest"),
        target: fs.location("moved"),
    }];
    plan
}

fn hold(dirs: &StoreDirs) -> File {
    let held = crate::jobs::test_mutation_file(&dirs.data);
    held.try_lock().unwrap();
    held
}

fn release_later(held: File) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(held);
    })
}

#[test]
fn a_job_waiting_for_the_profile_reports_it_and_cancel_ends_the_wait() {
    let (_root, dirs, journal) = setup();
    let _held = hold(&dirs);
    let fs = FakeFs::new();
    let plan = rename(&fs);
    let mut log = experimental_factory(journal, dirs)(&plan).unwrap();
    let cancel = CancelToken::new();
    let worker_cancel = cancel.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        log.check(LoggedOperation {
            plan: &plan,
            ops: &fs,
            cancel: &worker_cancel,
            progress: &mut |_| {},
            waiting: &mut |is_waiting| sender.send(is_waiting).unwrap(),
            retain_deletes: None,
        })
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)), Ok(true));
    cancel.cancel();
    let error = worker.join().unwrap().unwrap_err();
    assert_eq!(error, rusty_l10n::tr("job-wait-cancelled"));
    assert_eq!(receiver.try_iter().collect::<Vec<_>>(), [false]);
}

#[test]
fn a_job_in_another_profile_never_waits() {
    let (_first_root, first, _) = setup();
    let _held = hold(&first);
    let (_second_root, second, journal) = setup();
    let fs = FakeFs::new();
    let plan = rename(&fs);
    let mut log = experimental_factory(journal, second.clone())(&plan).unwrap();
    let mut reports = Vec::new();
    log.check(LoggedOperation {
        plan: &plan,
        ops: &fs,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |is_waiting| reports.push(is_waiting),
        retain_deletes: None,
    })
    .unwrap();
    assert!(reports.is_empty());
    assert!(
        crate::jobs::test_mutation_file(&second.data)
            .try_lock()
            .is_err()
    );
    drop(log);
}

#[test]
fn a_new_file_waits_for_a_held_exclusive_lease_then_proceeds() {
    let (root, dirs, _journal) = setup();
    let release = release_later(hold(&dirs));
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    let started = Instant::now();
    let lease = write_factory(dirs, true)(&target).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(200));
    release.join().unwrap();
    drop(lease);
}

#[test]
fn a_writer_that_outwaits_the_deadline_reports_busy_not_access_denied() {
    let (root, dirs, _journal) = setup();
    let _held = hold(&dirs);
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    let error = write_lease(&dirs, &target, true, short()).unwrap_err();
    assert_eq!(error, LeaseError::Busy);
    assert_eq!(error.kind(), rusty_core::FsErrorKind::Timeout);
    assert_eq!(String::from(error), rusty_l10n::tr("job-profile-busy"));
}

#[test]
fn safe_mode_writers_take_no_lease_and_never_wait() {
    let (root, dirs, _journal) = setup();
    let _held = hold(&dirs);
    let target = crate::startup::path_location(&root.path().join("new.txt")).unwrap();
    let started = Instant::now();
    assert!(claim_write(&dirs, &target, false).unwrap().is_none());
    assert!(started.elapsed() < Duration::from_secs(1));
}
