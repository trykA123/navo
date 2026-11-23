use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::{AsRawHandle, BorrowedHandle};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

const LIMITS: HelperLimits = HelperLimits {
    memory_bytes: 256 << 20,
};

fn waiting_child() -> Child {
    Command::new("cmd")
        .args(["/c", "pause"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn a_confined_child_is_in_the_job_with_its_memory_limit() {
    let mut child = waiting_child();
    let job = confine_helper(&child, LIMITS).unwrap();
    assert!(job.contains(&child).unwrap());
    assert_eq!(job.memory_limit().unwrap(), LIMITS.memory_bytes);
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn closing_the_job_kills_the_child() {
    let mut child = waiting_child();
    let job = confine_helper(&child, LIMITS).unwrap();
    assert!(child.try_wait().unwrap().is_none());
    drop(job);
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the child outlived its job");
        thread::sleep(Duration::from_millis(20));
    }
}

/// Writes `bytes` to a fresh temp file and reopens it read-only.
fn temp_file(name: &str, bytes: &[u8]) -> (std::path::PathBuf, File) {
    let path = std::env::temp_dir().join(format!("navo-share-{}-{name}", std::process::id()));
    File::create(&path).unwrap().write_all(bytes).unwrap();
    let file = File::open(&path).unwrap();
    (path, file)
}

/// The current process as a share target: its pseudo handle, which `BorrowedHandle` allows as -1.
fn current_process() -> BorrowedHandle<'static> {
    // SAFETY: the current-process pseudo handle is valid for the whole process lifetime and needs no close.
    unsafe {
        BorrowedHandle::borrow_raw(windows_sys::Win32::System::Threading::GetCurrentProcess())
    }
}

#[test]
fn a_shared_handle_reads_the_file_after_the_original_closes() {
    let (path, file) = temp_file("read", b"navo pdf bytes");
    let value = share_read_handle(&file, current_process()).unwrap();
    drop(file);
    // SAFETY: `value` was duplicated into this process by the line above and is adopted once.
    let mut adopted = unsafe { adopt_shared_file(value) }.unwrap();
    let mut text = String::new();
    adopted.read_to_string(&mut text).unwrap();
    assert_eq!(text, "navo pdf bytes");
    drop(adopted);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_shared_handle_cannot_write() {
    let (path, file) = temp_file("ro", b"x");
    let writable = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    drop(file);
    let value = share_read_handle(&writable, current_process()).unwrap();
    // SAFETY: `value` was duplicated into this process by the line above and is adopted once.
    let mut adopted = unsafe { adopt_shared_file(value) }.unwrap();
    assert!(adopted.write_all(b"y").is_err());
    drop((adopted, writable));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn empty_handle_values_are_refused() {
    for value in [0, u64::MAX] {
        // SAFETY: refused values are never adopted or closed.
        let err = unsafe { adopt_shared_file(value) }.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[test]
fn a_pipe_handle_is_refused_and_stays_open() {
    let (mut reader, mut writer) = std::io::pipe().unwrap();
    let value = reader.as_raw_handle().expose_provenance() as u64;
    // SAFETY: the pipe is refused before adoption, so `reader` keeps sole ownership.
    let err = unsafe { adopt_shared_file(value) }.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    writer.write_all(b"ok").unwrap();
    drop(writer);
    let mut text = String::new();
    reader.read_to_string(&mut text).unwrap();
    assert_eq!(text, "ok");
}
