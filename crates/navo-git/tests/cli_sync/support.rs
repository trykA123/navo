use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use navo_git::{
    Cancel, CliGit, CliOptions, GitError, ProcessOptions, ProcessOutput, SyncKind, SyncSummary,
    SyncTarget,
};

use super::cli_remote_support::sibling;
use super::fixture::Fixture;

pub const DEADLINE: Duration = Duration::from_secs(20);

/// Options that hide the machine's system and user Git config, so settings such as Git for
/// Windows' `core.autocrlf=true` cannot change what the product does in a test.
pub fn isolated(fixture: &Fixture) -> CliOptions {
    let global = sibling(fixture, "global.gitconfig");
    fs::write(&global, b"").unwrap();
    CliOptions {
        env: vec![
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), global.into()),
        ],
        ..CliOptions::default()
    }
}

/// The product's CLI for `fixture`, with an isolated Git config.
pub fn cli(fixture: &Fixture) -> CliGit {
    CliGit::with_options(fixture.repo(), &isolated(fixture)).unwrap()
}

/// What the confirmation of `kind` binds the run to.
pub fn confirm(cli: &CliGit, kind: SyncKind) -> Result<SyncTarget, GitError> {
    Ok(cli.sync_preview(kind, &cli.guard()?)?.target)
}

pub fn pull(cli: &CliGit) -> Result<SyncSummary, GitError> {
    cli.pull_with(&confirm(cli, SyncKind::Pull)?, &Cancel::default(), DEADLINE)
}

pub fn push(cli: &CliGit) -> Result<SyncSummary, GitError> {
    cli.push_with(&confirm(cli, SyncKind::Push)?, &Cancel::default(), DEADLINE)
}

/// The pull command line of the first version, as a control for what the fix prevents.
pub fn old_pull(cli: &CliGit) -> ProcessOutput {
    let args = [
        "pull",
        "--ff-only",
        "--no-rebase",
        "--no-recurse-submodules",
        "--no-stat",
    ];
    let options = ProcessOptions {
        stdout_cap: 1 << 20,
        timeout: DEADLINE,
    };
    cli.run_cancellable(&args, &Cancel::default(), &options)
        .unwrap()
}

/// A fresh clone of `origin` named `name` commits `file` with `bytes` and sends it to `main`.
pub fn change_on_origin(fixture: &Fixture, origin: &Path, name: &str, file: (&str, &[u8])) {
    let clone = sibling(fixture, name);
    fixture.git_ok(&["clone", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    fs::write(clone.join(file.0), file.1).unwrap();
    let clone = clone.to_str().unwrap();
    fixture.git_ok(&["-C", clone, "add", "."]);
    fixture.git_ok(&["-C", clone, "commit", "-m", name]);
    fixture.git_ok(&["-C", clone, "push", "origin", "main"]);
}

pub fn commit_file(fixture: &Fixture, file: &str, bytes: &[u8]) {
    fixture.write(file, bytes);
    fixture.git_ok(&["add", file]);
    fixture.commit(file);
}

/// HEAD, the index bytes and every worktree file outside `.git`.
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    head: Vec<u8>,
    index: Vec<u8>,
    files: BTreeMap<PathBuf, Vec<u8>>,
}

pub fn snapshot(fixture: &Fixture) -> Snapshot {
    let mut files = BTreeMap::new();
    collect(fixture.repo(), fixture.repo(), &mut files);
    Snapshot {
        head: fixture.git_ok(&["rev-parse", "HEAD"]).stdout,
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        files,
    }
}

fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path == root.join(".git") {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            files.insert(relative, fs::read(&path).unwrap());
        }
    }
}

/// Every ref of the bare `origin` with its object.
pub fn refs_of(fixture: &Fixture, origin: &Path) -> String {
    let output = fixture.git_ok(&["-C", origin.to_str().unwrap(), "for-each-ref"]);
    String::from_utf8(output.stdout).unwrap()
}

pub fn calls(log: &Path) -> String {
    fs::read_to_string(log).unwrap_or_default()
}

/// The git calls of a run, where a wrapper can log them (unix); empty elsewhere.
pub struct Log(Option<PathBuf>);

impl Log {
    /// Fails when `words` was never seen; passes where no log is kept.
    pub fn assert_ran(&self, words: &str) {
        if let Some(log) = &self.0 {
            assert!(calls(log).contains(words), "git never ran {words:?}");
        }
    }

    /// Fails when `words` was seen; passes where no log is kept.
    pub fn assert_not_ran(&self, words: &str) {
        if let Some(log) = &self.0 {
            assert!(!calls(log).contains(words), "git ran {words:?}");
        }
    }
}

/// A CLI for the repository and the log of its git calls.
#[cfg(unix)]
pub fn watched(fixture: &Fixture) -> (CliGit, Log) {
    let (cli, log) = super::cli_remote_support::wrapped(fixture, "");
    (cli, Log(Some(log)))
}

/// A CLI for the repository and an empty log: a shell wrapper cannot front `git.exe`.
#[cfg(not(unix))]
pub fn watched(fixture: &Fixture) -> (CliGit, Log) {
    (cli(fixture), Log(None))
}

/// `path` with forward slashes, as the shell of a hook and a Git config value want it.
pub fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Runs `operation` while a thread cancels it as soon as `marker` exists.
pub fn cancel_when<T>(marker: &Path, operation: impl FnOnce(&Cancel) -> T) -> (T, Duration) {
    use std::thread;
    use std::time::Instant;

    let cancel = Cancel::default();
    let trigger = cancel.clone();
    let marker = marker.to_path_buf();
    let canceller = thread::spawn(move || {
        let started = Instant::now();
        while !marker.exists() && !trigger.is_cancelled() && started.elapsed() < DEADLINE {
            thread::sleep(Duration::from_millis(5));
        }
        trigger.cancel();
    });
    let started = Instant::now();
    let out = operation(&cancel);
    let took = started.elapsed();
    cancel.cancel();
    canceller.join().unwrap();
    (out, took)
}

/// Shell lines that touch a marker, then stall past every deadline, and the marker path.
/// On Windows the stall is a child whose process id is first written to `stalled_pid`.
pub fn stall_body(fixture: &Fixture) -> (String, PathBuf) {
    let marker = sibling(fixture, "stalled");
    let touch = slashed(&marker);
    #[cfg(windows)]
    let body = format!(
        "sleep 41 &\ncat /proc/$!/winpid >> '{}'\ntouch '{touch}'\nwait",
        slashed(&stalled_pid(fixture))
    );
    #[cfg(not(windows))]
    let body = format!("touch '{touch}'\nsleep 41");
    (body, marker)
}

#[cfg(windows)]
fn stalled_pid(fixture: &Fixture) -> PathBuf {
    sibling(fixture, "stalled.pid")
}

/// A hook directory whose `name` hook touches a marker, then stalls past every deadline.
pub fn stalling_hook(fixture: &Fixture, name: &str) -> (PathBuf, PathBuf) {
    let (body, marker) = stall_body(fixture);
    (script_hook(fixture, name, &body), marker)
}

/// Fails when a process of the stalled script outlives the kill.
#[cfg(target_os = "linux")]
pub fn nothing_left(fixture: &Fixture) {
    let found = super::cli_remote_support::running_in(fixture.repo().parent().unwrap());
    assert_eq!(found, Vec::<String>::new());
}

/// Fails when a stalled child, whose id the script wrote, outlives the kill.
#[cfg(windows)]
pub fn nothing_left(fixture: &Fixture) {
    use std::process::Command;
    use std::time::Instant;

    let ids = fs::read_to_string(stalled_pid(fixture)).expect("the stall wrote no process id");
    assert!(
        !ids.trim().is_empty(),
        "the stall wrote an empty process id"
    );
    for pid in ids.split_whitespace() {
        let alive = || {
            let listing = Command::new("tasklist.exe")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&listing.stdout)
                .lines()
                .any(|line| line.split_whitespace().nth(1) == Some(pid))
        };
        let started = Instant::now();
        while alive() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!alive(), "the stalled child {pid} outlived the kill");
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn nothing_left(_fixture: &Fixture) {}

/// Makes `origin`'s upload-pack touch a marker and wait for a go file, so a fetch stalls
/// before it writes any ref. Returns the marker and the go file.
pub fn stalling_fetch(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let (stalled, go) = (sibling(fixture, "fetching"), sibling(fixture, "go"));
    let body = format!(
        "touch '{}'\nn=0\nwhile [ ! -e '{}' ] && [ $n -lt 400 ]; do sleep 0.05; n=$((n+1)); done\nexec git upload-pack \"$@\"",
        slashed(&stalled),
        slashed(&go)
    );
    let script = script_file(fixture, "upload-pack", &body);
    let command = format!("sh '{}'", slashed(&script));
    fixture.git_ok(&["config", "remote.origin.uploadpack", &command]);
    (stalled, go)
}

/// Runs `operation` while another thread waits for `stalled`, runs `change`, then creates `go`.
pub fn change_while_stalled<T>(
    (stalled, go): (&Path, &Path),
    change: impl FnOnce() + Send,
    operation: impl FnOnce() -> T,
) -> T {
    use std::time::Instant;

    std::thread::scope(|scope| {
        let changer = scope.spawn(move || {
            let started = Instant::now();
            while !stalled.exists() && started.elapsed() < DEADLINE {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(stalled.exists(), "the fetch never started");
            change();
            fs::write(go, b"").unwrap();
        });
        let out = operation();
        changer.join().unwrap();
        out
    })
}

/// A hook directory whose `name` hook touches `marker` and exits with `code`.
pub fn hook(fixture: &Fixture, name: &str, code: u8) -> (PathBuf, PathBuf) {
    let marker = sibling(fixture, "hook-ran");
    let body = format!("touch '{}'\nexit {code}", slashed(&marker));
    (script_hook(fixture, name, &body), marker)
}

/// A hook directory whose `name` hook runs `body`.
pub fn script_hook(fixture: &Fixture, name: &str, body: &str) -> PathBuf {
    let dir = sibling(fixture, "hooks");
    fs::create_dir_all(&dir).unwrap();
    write_script(&dir.join(name), body);
    dir
}

/// A hook `name` in the repository's own `.git/hooks` that appends its argument and the refs
/// it is given to `log`.
pub fn logging_repo_hook(fixture: &Fixture, name: &str, log: &Path) {
    let dir = fixture.repo().join(".git/hooks");
    fs::create_dir_all(&dir).unwrap();
    let body = format!("{{ echo \"$1\"; cat; }} >> '{}'", slashed(log));
    write_script(&dir.join(name), &body);
}

/// A shell script `name` beside the repository.
pub fn script_file(fixture: &Fixture, name: &str, body: &str) -> PathBuf {
    let path = sibling(fixture, name);
    write_script(&path, body);
    path
}

/// A `#!/bin/sh` file; Git for Windows runs it through its own shell without an executable bit.
fn write_script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
