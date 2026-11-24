//! Tests for the editor CLI tool runner: refusals (relative, repo-local, shell), the timeout,
//! stdin to stdout, and the output cap.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{ToolError, ToolRequest, run_tool, run_tool_capped};

/// A request for `exe` that edits a file in a fresh temp folder with no `.git` above it.
fn request(exe: &Path, edited_dir: &Path) -> ToolRequest {
    ToolRequest {
        exe: exe.to_path_buf(),
        argv: Vec::new(),
        stdin: Vec::new(),
        cwd: edited_dir.to_path_buf(),
        timeout: Duration::from_secs(5),
        edited_file: edited_dir.join("note.txt"),
    }
}

#[cfg(unix)]
fn echo_exe() -> PathBuf {
    PathBuf::from("/bin/cat")
}

#[cfg(windows)]
fn echo_exe() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\findstr.exe")
}

#[test]
fn editor_tools_relative_exe_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let result = run_tool(&request(Path::new("cat"), dir.path()));
    assert!(matches!(result, Err(ToolError::NotAbsolute)), "{result:?}");
}

#[test]
fn editor_tools_missing_exe_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let result = run_tool(&request(&dir.path().join("no-such-tool"), dir.path()));
    assert!(matches!(result, Err(ToolError::NotFound)), "{result:?}");
}

#[test]
fn editor_tools_binary_planted_in_repo_is_refused() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    std::fs::create_dir_all(repo.path().join("src/deep")).unwrap();
    std::fs::create_dir(repo.path().join("tools")).unwrap();
    let planted = repo
        .path()
        .join("tools")
        .join(echo_exe().file_name().unwrap());
    std::fs::copy(echo_exe(), &planted).unwrap();
    let result = run_tool(&request(&planted, &repo.path().join("src/deep")));
    assert!(matches!(result, Err(ToolError::RepoLocal)), "{result:?}");
}

#[test]
fn editor_tools_binary_next_to_file_without_repo_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let planted = dir.path().join(echo_exe().file_name().unwrap());
    std::fs::copy(echo_exe(), &planted).unwrap();
    let result = run_tool(&request(&planted, dir.path()));
    assert!(matches!(result, Err(ToolError::RepoLocal)), "{result:?}");
}

#[test]
fn editor_tools_shells_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let edited = dir.path().join("edited");
    std::fs::create_dir(&edited).unwrap();
    let shells = dir.path().join("shells");
    std::fs::create_dir(&shells).unwrap();
    let names = [
        "cmd.exe",
        "PowerShell.EXE",
        "pwsh",
        "bash",
        "wsl.exe",
        "env",
        "dash",
        "ksh",
        "csh",
        "busybox",
        "cscript.exe",
        "WScript.exe",
        "mshta.exe",
        "conhost.exe",
    ];
    for name in names {
        let fake = shells.join(name);
        std::fs::copy(echo_exe(), &fake).unwrap();
        let result = run_tool(&request(&fake, &edited));
        assert!(
            matches!(result, Err(ToolError::Shell)),
            "{name}: {result:?}"
        );
    }
    #[cfg(unix)]
    {
        let result = run_tool(&request(Path::new("/bin/sh"), &edited));
        assert!(
            matches!(result, Err(ToolError::Shell)),
            "/bin/sh: {result:?}"
        );
    }
}

#[cfg(unix)]
fn runaway() -> (PathBuf, Vec<String>) {
    (PathBuf::from("/bin/sleep"), vec!["30".to_owned()])
}

#[cfg(windows)]
fn runaway() -> (PathBuf, Vec<String>) {
    let argv = ["-n", "30", "127.0.0.1"].map(str::to_owned).to_vec();
    (PathBuf::from(r"C:\Windows\System32\PING.EXE"), argv)
}

#[test]
fn editor_tools_runaway_process_dies_at_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, argv) = runaway();
    let mut req = request(&exe, dir.path());
    req.argv = argv;
    req.timeout = Duration::from_millis(300);
    let started = Instant::now();
    let result = run_tool(&req);
    assert!(matches!(result, Err(ToolError::Timeout)), "{result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn editor_tools_stdin_reaches_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(&echo_exe(), dir.path());
    if cfg!(windows) {
        req.argv = vec!["^".to_owned()];
    }
    req.stdin = b"hi".to_vec();
    let output = run_tool(&req).unwrap();
    assert_eq!(output.status, Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), "hi");
    assert!(!output.truncated);
}

#[test]
fn editor_tools_output_over_cap_is_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(&echo_exe(), dir.path());
    if cfg!(windows) {
        req.argv = vec!["^".to_owned()];
    }
    req.stdin = vec![b'x'; 4096];
    let output = run_tool_capped(&req, 100).unwrap();
    assert_eq!(output.stdout.len(), 100);
    assert!(output.truncated);
}

#[test]
fn editor_tools_extension_rule_allows_only_exe_and_com_on_windows() {
    use super::guard::is_allowed_extension;
    for name in [
        "fmt.bat", "fmt.cmd", "fmt.BAT", "fmt.Cmd", "fmt.ps1", "fmt.vbs", "fmt",
    ] {
        assert!(
            !is_allowed_extension(Path::new(name), true),
            "windows {name}"
        );
    }
    for name in ["fmt.exe", "fmt.EXE", "fmt.com", "fmt.Com"] {
        assert!(
            is_allowed_extension(Path::new(name), true),
            "windows {name}"
        );
    }
    for name in ["fmt.bat", "fmt.cmd", "fmt.BAT", "fmt.ps1", "fmt.vbs"] {
        assert!(!is_allowed_extension(Path::new(name), false), "unix {name}");
    }
    for name in ["fmt", "fmt.exe", "fmt.com", "python3.12"] {
        assert!(is_allowed_extension(Path::new(name), false), "unix {name}");
    }
}

#[cfg(windows)]
#[test]
fn editor_tools_batch_copies_of_a_real_exe_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let edited = dir.path().join("edited");
    std::fs::create_dir(&edited).unwrap();
    for name in ["fmt.BAT", "fmt.cmd"] {
        let fake = dir.path().join(name);
        std::fs::copy(echo_exe(), &fake).unwrap();
        let result = run_tool(&request(&fake, &edited));
        assert!(
            matches!(result, Err(ToolError::Shell)),
            "{name}: {result:?}"
        );
    }
}

/// A temp repo (`.git` at its root) holding a copy of the echo exe under `tools/`.
fn repo_with_planted_exe() -> (tempfile::TempDir, PathBuf) {
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    std::fs::create_dir(repo.path().join("tools")).unwrap();
    let planted = repo
        .path()
        .join("tools")
        .join(echo_exe().file_name().unwrap());
    std::fs::copy(echo_exe(), &planted).unwrap();
    (repo, planted)
}

#[test]
fn editor_tools_unresolvable_folder_is_refused() {
    let (repo, planted) = repo_with_planted_exe();
    let mut req = request(&planted, repo.path());
    req.edited_file = repo.path().join("missing").join("x.txt");
    let result = run_tool(&req);
    assert!(
        matches!(result, Err(ToolError::FolderUnresolved)),
        "{result:?}"
    );
}

#[cfg(unix)]
#[test]
fn editor_tools_unresolvable_folder_behind_a_symlink_is_refused() {
    let (repo, planted) = repo_with_planted_exe();
    let outside = tempfile::tempdir().unwrap();
    let link = outside.path().join("link");
    std::os::unix::fs::symlink(repo.path(), &link).unwrap();
    let mut req = request(&planted, outside.path());
    req.edited_file = link.join("missing").join("x.txt");
    let result = run_tool(&req);
    assert!(
        matches!(result, Err(ToolError::FolderUnresolved)),
        "{result:?}"
    );
}

#[test]
fn editor_tools_missing_job_kills_the_child_and_refuses() {
    let (exe, argv) = runaway();
    let mut child = std::process::Command::new(exe).args(argv).spawn().unwrap();
    let result = super::process::require_job::<()>(&mut child, None);
    assert!(matches!(result, Err(ToolError::Spawn(_))), "{result:?}");
    let status = child.try_wait().unwrap();
    assert!(status.is_some(), "the unconfined child is still running");
}
