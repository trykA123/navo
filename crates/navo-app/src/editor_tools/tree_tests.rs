//! Tests that the editor tool runner kills the whole process tree: the test binary re-runs
//! itself as a tool that starts a grandchild, then the test checks the grandchild is gone.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{ToolError, ToolRequest, run_tool};

/// Marker file in the helper's cwd; its text ("hold" or "detach") switches on parent mode.
const MODE_FILE: &str = ".navo-e8a-tree-mode";

/// File in the helper's cwd that receives the grandchild's pid.
const PID_FILE: &str = "grandchild.pid";

/// Not a real test: run as a tool by the tests below, it starts a 30 s grandchild and either
/// sleeps holding stdout ("hold") or exits at once with the grandchild detached ("detach").
/// Without the marker in its cwd (a normal test run) it returns at once.
#[test]
fn editor_tools_tree_helper() {
    let Ok(mode) = std::fs::read_to_string(Path::new(MODE_FILE)) else {
        return;
    };
    let mut command = Command::new("/bin/sleep");
    command.arg("30").stdin(Stdio::null()).stderr(Stdio::null());
    if mode == "detach" {
        command.stdout(Stdio::null());
    }
    #[allow(
        clippy::zombie_processes,
        reason = "the grandchild is left running on purpose: the runner under test must kill it"
    )]
    let child = command.spawn().unwrap();
    std::fs::write("pid.tmp", child.id().to_string()).unwrap();
    std::fs::rename("pid.tmp", PID_FILE).unwrap();
    if mode == "hold" {
        std::thread::sleep(Duration::from_secs(30));
    }
}

/// A request that re-runs this test binary as the tree helper in a fresh cwd set to `mode`.
fn helper_request(mode: &str, timeout: Duration) -> (tempfile::TempDir, ToolRequest) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MODE_FILE), mode).unwrap();
    let request = ToolRequest {
        exe: std::env::current_exe().unwrap(),
        argv: ["editor_tools_tree_helper", "--test-threads=1"]
            .map(str::to_owned)
            .to_vec(),
        stdin: Vec::new(),
        cwd: dir.path().to_path_buf(),
        timeout,
        edited_file: dir.path().join("note.txt"),
    };
    (dir, request)
}

/// True once `pid` names no process; polls for 3 s, since init reaps an orphan a little later.
fn is_gone(pid: libc::pid_t) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        // SAFETY: kill with signal 0 takes no pointers and only checks that the pid exists.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if !alive && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Reads the grandchild pid the helper wrote into `dir`.
fn grandchild_pid(dir: &Path) -> libc::pid_t {
    let path: PathBuf = dir.join(PID_FILE);
    std::fs::read_to_string(path)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn editor_tools_timeout_kills_the_grandchild_holding_stdout() {
    let timeout = Duration::from_secs(2);
    let (dir, req) = helper_request("hold", timeout);
    let started = Instant::now();
    let result = run_tool(&req);
    assert!(matches!(result, Err(ToolError::Timeout)), "{result:?}");
    let elapsed = started.elapsed();
    assert!(elapsed < timeout + Duration::from_secs(3), "{elapsed:?}");
    let pid = grandchild_pid(dir.path());
    assert!(is_gone(pid), "grandchild {pid} survived the timeout");
}

#[test]
fn editor_tools_clean_exit_kills_the_detached_grandchild() {
    let (dir, req) = helper_request("detach", Duration::from_secs(10));
    let output = run_tool(&req).unwrap();
    assert_eq!(output.status, Some(0), "{output:?}");
    let pid = grandchild_pid(dir.path());
    assert!(is_gone(pid), "grandchild {pid} survived the tool's exit");
}
