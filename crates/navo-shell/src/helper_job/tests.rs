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

/// `cmd.exe` by full path, because `spawn_confined` never searches `PATH`.
fn cmd_exe() -> std::path::PathBuf {
    std::env::var_os("ComSpec").map_or_else(|| r"C:\Windows\System32\cmd.exe".into(), Into::into)
}

/// Starts `cmd.exe` with `args` through `spawn_confined`.
fn confined_cmd(args: &[&str]) -> std::io::Result<ConfinedChild> {
    let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
    spawn_confined(&cmd_exe(), &args, LIMITS)
}

/// Polls `child` until it reports exited; panics after 5 s.
fn wait_exited(child: &ConfinedChild) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !child.try_wait().unwrap() {
        assert!(Instant::now() < deadline, "the confined child did not exit");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn spawn_confined_child_that_exits_reports_exited() {
    let child = confined_cmd(&["/c", "exit", "0"]).unwrap();
    wait_exited(&child);
}

#[test]
fn spawn_confined_child_runs_in_its_job_and_kill_ends_it() {
    let child = confined_cmd(&["/c", "pause"]).unwrap();
    let job = child.process.job();
    assert!(job.contains(&child.process_handle()).unwrap());
    assert_eq!(job.memory_limit().unwrap(), LIMITS.memory_bytes);
    assert!(!child.try_wait().unwrap());
    child.kill().unwrap();
    wait_exited(&child);
    child.kill().unwrap();
}

#[test]
fn spawn_confined_pipes_reach_eof_when_the_child_exits() {
    let mut child = confined_cmd(&["/c", "echo", "hi"]).unwrap();
    let mut text = String::new();
    child.stdout.read_to_string(&mut text).unwrap();
    assert_eq!(text.trim_end(), "hi");
    wait_exited(&child);
}

#[test]
fn spawn_confined_bad_program_is_an_error() {
    let missing = std::env::temp_dir().join("navo-no-such-helper-S2a1.exe");
    let err = spawn_confined(&missing, &[], LIMITS).err().unwrap();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn spawn_confined_refuses_a_nul_in_an_argument() {
    let err = confined_cmd(&["/c", "exit\0"]).err().unwrap();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}
