//! Negative controls: each checker is shown to fire on a deliberate leak.

use super::super::tests::{Rig, on};
use super::{Hay, Seen, Spy, files_in, leaks, wait_until_len};

const CONTROL: &str = "CONTROL-5e1d20-clip";

fn caught(hays: &[Hay]) {
    assert!(
        !leaks(CONTROL, hays).is_empty(),
        "the checker missed a deliberate leak"
    );
}

fn logged_spy(app: &tempfile::TempDir) -> Spy {
    let spy = Spy::install(app.path());
    tracing::info!(text = CONTROL, "a deliberate leak");
    spy
}

#[test]
fn privacy_control_raw_log_checker_fires() {
    let app = tempfile::tempdir().unwrap();
    caught(&logged_spy(&app).raw());
}

#[test]
fn privacy_control_ring_checker_fires() {
    let app = tempfile::tempdir().unwrap();
    caught(&logged_spy(&app).ring());
}

#[test]
fn privacy_control_log_file_checker_fires() {
    let app = tempfile::tempdir().unwrap();
    caught(&logged_spy(&app).files());
}

#[test]
fn privacy_control_report_checker_fires() {
    let app = tempfile::tempdir().unwrap();
    caught(&logged_spy(&app).report());
}

#[test]
fn privacy_control_crash_checker_fires() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    spy.crash(CONTROL);
    caught(&spy.crash_records());
}

#[test]
fn privacy_control_debug_checker_fires() {
    #[derive(Debug)]
    struct Leaky {
        _text: &'static str,
    }
    let mut seen = Seen::default();
    seen.note("leaky", &Leaky { _text: CONTROL });
    caught(&seen.0);
}

#[test]
fn privacy_control_database_checker_fires() {
    let rig = Rig::new();
    rig.apply(on());
    assert!(rig.emit(CONTROL));
    wait_until_len(&rig, 1);
    caught(&files_in(&rig.data));
}

#[cfg(windows)]
#[test]
fn privacy_control_locked_database_files_checker_fires() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    for name in [
        "clipboard.db",
        "clipboard.db-wal",
        "clipboard.db-shm",
        "clipboard.db.corrupt-1",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, CONTROL).unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let mut lock = OVERLAPPED::default();
        // SAFETY: file is live, lock is zero-initialized, and closing file releases the lock.
        let locked = unsafe {
            LockFileEx(
                file.as_raw_handle(),
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                CONTROL.len() as u32,
                0,
                &raw mut lock,
            )
        };
        assert_ne!(locked, 0);
        assert_eq!(std::fs::read(&path).unwrap_err().raw_os_error(), Some(33));
        caught(&files_in(dir.path()));
    }
}
