//! AG18 red test for the flaky Windows rename: a pause must return only after every process of
//! the stopped Git tree has exited, so the folder it held can be renamed at once.

use std::fs;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use navo_git::cli::stop_under;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

use super::{run_git, start_waiting_git};

/// Rounds per run: one round alone rarely meets the race on an idle machine.
const ROUNDS: usize = 20;

/// Starts `repo` with an alias whose shell writes its own and its `sleep`'s Windows pids.
fn repo_with_pid_alias(repo: &Path, pids: &Path) {
    fs::create_dir(repo).unwrap();
    run_git(repo, &["init", "-q"]);
    let pids = pids.to_string_lossy().replace('\\', "/");
    let alias = format!(
        "!sleep 10 & c=$!; i=0; \
         while [ ! -r /proc/$c/winpid ] && [ $i -lt 40 ]; do sleep 0.05; i=$((i + 1)); done; \
         echo \"$(cat /proc/$$/winpid) $(cat /proc/$c/winpid)\" > \"{pids}.tmp\" && \
         mv \"{pids}.tmp\" \"{pids}\"; wait"
    );
    run_git(repo, &["config", "alias.navo-wait", &alias]);
}

/// Waits until the alias wrote both pids.
fn wait_pids(pids: &Path) -> Vec<u32> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(text) = fs::read_to_string(pids) {
            let parsed: Vec<u32> = text
                .split_whitespace()
                .filter_map(|pid| pid.parse().ok())
                .collect();
            if parsed.len() == 2 {
                return parsed;
            }
        }
        assert!(Instant::now() < deadline, "the Git alias never started");
        thread::sleep(Duration::from_millis(5));
    }
}

/// A handle that can wait for `pid`, opened while the process still runs.
fn open_process(pid: u32) -> OwnedHandle {
    // SAFETY: OpenProcess takes no pointers; the null result is checked before wrapping.
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(
        !raw.is_null(),
        "pid {pid}: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: raw is a fresh process handle that only this guard owns.
    unsafe { OwnedHandle::from_raw_handle(raw) }
}

/// Whether the process has fully exited (its object is signalled).
fn exited(process: &OwnedHandle) -> bool {
    // SAFETY: the handle is live; a zero timeout only polls.
    unsafe { WaitForSingleObject(process.as_raw_handle(), 0) == WAIT_OBJECT_0 }
}

/// Red when the pause wakes on the job's process count while a stopped process is still
/// exiting (`alive > 0`), or when something outside the tree holds the folder (`alive == 0`
/// and the rename fails).
#[test]
fn a_pause_returns_only_after_every_stopped_git_process_has_exited() {
    for round in 0..ROUNDS {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let moved = dir.path().join("repo-renamed");
        let pids = dir.path().join("pids.txt");
        repo_with_pid_alias(&repo, &pids);
        let child = start_waiting_git(&repo);
        let processes: Vec<OwnedHandle> = wait_pids(&pids).into_iter().map(open_process).collect();

        let pause = stop_under(&repo, Duration::from_secs(3)).unwrap();
        let alive = processes.iter().filter(|process| !exited(process)).count();
        let renamed = fs::rename(&repo, &moved);
        drop(pause);
        drop(child);
        assert!(
            alive == 0 && renamed.is_ok(),
            "round {round}: {alive} stopped Git processes still exiting when the pause \
             returned; rename: {renamed:?}"
        );
    }
}
