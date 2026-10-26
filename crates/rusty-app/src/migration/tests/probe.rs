use std::fs::File;
use std::time::{Duration, Instant};

use rusty_store::StoreDirs;

use super::super::probe::{Legacy, is_locked, probe};
use super::super::{Outcome, RealFs};
use super::fixture::Fixture;

const AT_ONCE: Duration = Duration::from_secs(1);

fn closed_instance(_: &str) -> bool {
    false
}

fn hold(path: &std::path::Path) -> File {
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .unwrap();
    file.lock().unwrap();
    file
}

#[test]
fn a_held_job_lock_means_ferrite_runs() {
    let fixture = Fixture::user();
    let lock = fixture.old.legacy_lock_file();
    assert_eq!(probe(&fixture.old, closed_instance), Legacy::Closed);
    let held = hold(&lock);
    assert_eq!(probe(&fixture.old, closed_instance), Legacy::Running);
    drop(held);
    let shared = File::open(&lock).unwrap();
    shared.lock_shared().unwrap();
    assert!(is_locked(&lock), "a second Ferrite holds the lock shared");
}

#[test]
fn a_running_instance_is_reported_with_the_old_profile_hash() {
    let fixture = Fixture::user();
    let expected = crate::instance::profile_hash(&fixture.old.data).unwrap();
    let running = probe(&fixture.old, |hash| hash == expected);
    assert_eq!(running, Legacy::Running);
}

#[test]
fn probing_a_missing_profile_creates_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let old = StoreDirs {
        config: temp.path().join("roaming/Ferrite"),
        data: temp.path().join("local/Ferrite"),
    };
    assert_eq!(probe(&old, closed_instance), Legacy::Closed);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn blocked_returns_at_once_and_changes_nothing() {
    for make in super::fixture::layouts() {
        let fixture = make();
        let before = fixture.tree();
        let held = hold(&fixture.old.legacy_lock_file());
        let started = Instant::now();
        let outcome = fixture
            .migration(&RealFs)
            .run(|| probe(&fixture.old, closed_instance) == Legacy::Running);
        assert!(started.elapsed() < AT_ONCE, "{:?}", started.elapsed());
        assert!(matches!(outcome, Outcome::Blocked), "{outcome:?}");
        drop(held);
        assert!(fixture.tree() == before);
    }
}

#[cfg(unix)]
#[test]
fn a_held_old_instance_lock_means_ferrite_runs() {
    use super::super::probe::lock_in;

    let fixture = Fixture::user();
    let runtime = fixture.path("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    let hash = crate::instance::profile_hash(&fixture.old.data).unwrap();
    let in_runtime = |hash: &str| is_locked(&lock_in(&runtime, hash));
    assert_eq!(probe(&fixture.old, in_runtime), Legacy::Closed);
    let held = hold(&lock_in(&runtime, &hash));
    let started = Instant::now();
    assert_eq!(probe(&fixture.old, in_runtime), Legacy::Running);
    assert!(started.elapsed() < AT_ONCE);
    drop(held);
    assert_eq!(probe(&fixture.old, in_runtime), Legacy::Closed);
}

#[cfg(windows)]
#[test]
fn a_held_old_mutex_blocks_without_waiting() {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    use windows_sys::Win32::System::Threading::CreateMutexW;

    use super::super::probe::{legacy, mutex_name};

    let fixture = Fixture::user();
    let before = fixture.tree();
    let hash = crate::instance::profile_hash(&fixture.old.data).unwrap();
    let name = mutex_name(&hash);
    assert_eq!(legacy(&fixture.old), Legacy::Closed);
    // SAFETY: the name is NUL-terminated; default security and no initial ownership.
    let raw = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    assert!(!raw.is_null());
    // SAFETY: CreateMutexW returned a valid handle that this test owns.
    let mutex = unsafe { OwnedHandle::from_raw_handle(raw) };
    let started = Instant::now();
    let outcome = fixture
        .migration(&RealFs)
        .run(|| legacy(&fixture.old) == Legacy::Running);
    assert!(started.elapsed() < AT_ONCE, "{:?}", started.elapsed());
    assert!(matches!(outcome, Outcome::Blocked), "{outcome:?}");
    assert!(fixture.tree() == before);
    drop(mutex);
    assert_eq!(legacy(&fixture.old), Legacy::Closed);
}
