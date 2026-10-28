use std::sync::mpsc;

use super::*;

fn limited(limit: Duration) -> Wait<'static> {
    Wait {
        limit,
        ..Wait::bounded()
    }
}

#[test]
fn ordinary_mutations_share_the_lease_and_an_exclusive_waiter_times_out() {
    let root = tempfile::tempdir().unwrap();
    let first = lease(root.path(), Mode::Shared, Wait::bounded()).unwrap();
    let second = lease(root.path(), Mode::Shared, Wait::bounded()).unwrap();
    let started = Instant::now();
    let error = lease(
        root.path(),
        Mode::Exclusive,
        limited(Duration::from_millis(150)),
    )
    .unwrap_err();
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(error, LeaseError::Busy);
    assert_eq!(error.kind(), FsErrorKind::Timeout);
    assert_eq!(String::from(error), rusty_l10n::tr("job-profile-busy"));
    drop((first, second));
    assert!(lease(root.path(), Mode::Exclusive, limited(Duration::ZERO)).is_ok());
}

#[test]
fn a_second_exclusive_waiter_times_out_while_the_first_holds() {
    let root = tempfile::tempdir().unwrap();
    let _held = lease(root.path(), Mode::Exclusive, Wait::bounded()).unwrap();
    for mode in [Mode::Exclusive, Mode::Shared] {
        let waited = lease(root.path(), mode, limited(Duration::from_millis(100)));
        assert_eq!(waited.unwrap_err(), LeaseError::Busy);
    }
}

#[test]
fn a_waiter_obtains_the_lease_when_the_holder_releases_in_time() {
    let root = tempfile::tempdir().unwrap();
    let held = lease(root.path(), Mode::Exclusive, Wait::bounded()).unwrap();
    let data = root.path().to_owned();
    let (sender, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let mut report = |waiting| sender.send(waiting).unwrap();
        let wait = Wait {
            waiting: Some(&mut report),
            ..Wait::bounded()
        };
        lease(&data, Mode::Shared, wait).is_ok()
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)), Ok(true));
    drop(held);
    assert!(waiter.join().unwrap());
    assert_eq!(receiver.try_iter().collect::<Vec<_>>(), [false]);
}

#[test]
fn a_waiting_lease_reports_waiting_and_cancel_ends_it() {
    let root = tempfile::tempdir().unwrap();
    let _held = lease(root.path(), Mode::Exclusive, Wait::bounded()).unwrap();
    let data = root.path().to_owned();
    let cancel = CancelToken::new();
    let worker_cancel = cancel.clone();
    let (sender, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let mut report = |waiting| sender.send(waiting).unwrap();
        let wait = Wait {
            cancel: Some(&worker_cancel),
            waiting: Some(&mut report),
            limit: WAIT,
        };
        lease(&data, Mode::Exclusive, wait)
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)), Ok(true));
    let started = Instant::now();
    cancel.cancel();
    assert_eq!(waiter.join().unwrap().unwrap_err(), LeaseError::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(receiver.try_iter().collect::<Vec<_>>(), [false]);
}

#[test]
fn an_immediate_lease_never_reports_waiting() {
    let root = tempfile::tempdir().unwrap();
    let mut reports = Vec::new();
    let mut report = |waiting| reports.push(waiting);
    let wait = Wait {
        waiting: Some(&mut report),
        ..Wait::bounded()
    };
    lease(root.path(), Mode::Exclusive, wait).unwrap();
    assert!(reports.is_empty());
}

#[test]
fn two_profiles_lock_independently() {
    let root = tempfile::tempdir().unwrap();
    let (first, second) = (root.path().join("first"), root.path().join("second"));
    let _held = lease(&first, Mode::Exclusive, Wait::bounded()).unwrap();
    assert!(lease(&second, Mode::Exclusive, limited(Duration::ZERO)).is_ok());
    assert_eq!(
        lease(&first, Mode::Shared, limited(Duration::ZERO)).unwrap_err(),
        LeaseError::Busy
    );
}
