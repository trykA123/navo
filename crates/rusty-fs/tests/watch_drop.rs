#![cfg(any(windows, target_os = "linux"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on a broken fixture"
)]

mod support;

use std::fs;

use rusty_fs::WatchChange;
use support::{name, start_watch, wait_for_change, wait_until_live};

const WATCHES: usize = 16;

#[cfg(target_os = "linux")]
fn resources() -> (usize, usize) {
    let count = |dir: &str| fs::read_dir(dir).unwrap().count();
    (count("/proc/self/fd"), count("/proc/self/task"))
}

#[cfg(windows)]
fn resources() -> (usize, usize) {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
    let mut handles = 0u32;
    // SAFETY: the pseudo handle of this process is always valid and `handles` is writable.
    let read = unsafe { GetProcessHandleCount(GetCurrentProcess(), &raw mut handles) };
    assert_ne!(read, 0);
    (handles as usize, 0)
}

fn watch_and_drop(count: usize) {
    let dirs: Vec<_> = (0..count).map(|_| tempfile::tempdir().unwrap()).collect();
    let watches: Vec<_> = dirs.iter().map(|dir| start_watch(dir.path())).collect();
    for (dir, (_, events)) in dirs.iter().zip(&watches) {
        wait_until_live(dir.path(), events);
        fs::write(dir.path().join("ping.txt"), b"x").unwrap();
        wait_for_change(events, &WatchChange::Created(name("ping.txt")));
    }
    drop(watches);
}

#[test]
fn dropped_watches_leave_no_thread_or_handle_behind() {
    watch_and_drop(1);
    let before = resources();
    watch_and_drop(WATCHES);
    assert_eq!(resources(), before);
}
