//! Unit checks for the command backend: batch children, idle expiry and stopped trees.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::Cancel;
use crate::cli::ProcessOptions;
use crate::cli::process_cap::{hook_after_readers, run_cancellable};
use crate::cli::stop_under;
use crate::{CliGit, GitError, GitSource, ObjectId};

use super::super::process_tree::GitTree;
use super::{CommandSpec, base_command, hook_after_reap, run_process};

/// Creates a repository in `repo` and asserts that `git init` succeeded.
fn init_repo(repo: &Path) {
    fs::create_dir(repo).unwrap();
    let output = Command::new(git_program())
        .arg("-C")
        .arg(repo)
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success(), "git init: {output:?}");
}

/// The Git program the tests start; Windows names it with `.exe`.
fn git_program() -> &'static str {
    if cfg!(windows) { "git.exe" } else { "git" }
}

/// A Git spec against the real Git, optionally in `repo`, with a bounded wait.
fn spec<'a>(repo: Option<&'a Path>, timeout: Duration) -> CommandSpec<'a> {
    CommandSpec {
        git: Path::new(git_program()),
        repo,
        git_dir: None,
        env: &[],
        timeout,
    }
}

/// The alias body: Unix writes `$$`/`$!`; Windows writes both Windows pids from `/proc`.
fn wait_alias() -> String {
    let body = if cfg!(windows) {
        "cat /proc/$$/winpid > parent.pid\nsleep 20 &\nchild=$!\ni=0\nwhile [ ! -r /proc/$child/winpid ] && [ $i -lt 20 ]; do\nsleep 0.05\ni=$((i + 1))\ndone\ncat /proc/$child/winpid > child.pid\nwait"
    } else {
        "echo $$ > parent.pid\nsleep 20 &\necho $! > child.pid\nwait"
    };
    format!("alias.navo-wait=!{body}")
}

/// Waits until `repo/name` holds a non-empty value and returns it trimmed.
fn wait_for_file(repo: &Path, name: &str) -> String {
    let started = Instant::now();
    loop {
        if let Ok(value) = fs::read_to_string(repo.join(name))
            && !value.trim().is_empty()
        {
            return value.trim().to_owned();
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the helper did not start"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// Whether a Unix pid is still running; a zombie counts as gone on Linux.
#[cfg(unix)]
fn running(pid: &str) -> bool {
    #[cfg(target_os = "linux")]
    if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
        return !stat.rsplit_once(") ").unwrap().1.starts_with('Z');
    }
    Command::new("kill")
        .args(["-0", pid])
        .output()
        .unwrap()
        .status
        .success()
}

/// Whether a Windows native pid is still listed by tasklist.
#[cfg(windows)]
fn running(pid: &str) -> bool {
    let output = Command::new("tasklist.exe")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(pid))
}

/// Waits up to two seconds for a pid to leave the process table.
fn assert_gone(pid: &str) {
    let started = Instant::now();
    while running(pid) {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "pid {pid} is still running"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// The batch child must name the absolute git dir and never change into the repository.
#[test]
fn the_batch_command_avoids_the_repository_folder() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);

    let cli = CliGit::new(&repo).unwrap();
    let command = cli.batch_command().unwrap();
    let args: Vec<_> = command.get_args().collect();
    assert!(args.contains(&std::ffi::OsStr::new("--git-dir")));
    assert!(!args.contains(&std::ffi::OsStr::new("-C")));

    let git_dir = cli.git_dir().unwrap();
    assert!(git_dir.is_absolute());
    assert!(args.contains(&git_dir.as_os_str()));
    let temp_dir = std::env::temp_dir();
    let current = command
        .get_current_dir()
        .expect("the batch child must set a current directory");
    assert_eq!(current, temp_dir.as_path());
    assert!(!current.starts_with(&repo));
}

/// An unused batch child is replaced on the next read once its idle limit has passed.
#[test]
fn an_idle_batch_child_restarts_on_the_next_read() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);

    let mut cli = CliGit::new(&repo).unwrap();
    cli.batch_idle = Duration::from_secs(60);
    let missing = ObjectId::Sha1([0xff; 20]);

    assert!(matches!(
        cli.read_blob(&missing),
        Err(GitError::MissingObject { .. })
    ));
    let first = generation(&cli);
    assert!(matches!(
        cli.read_blob(&missing),
        Err(GitError::MissingObject { .. })
    ));
    assert_eq!(generation(&cli), first, "a fresh child must be reused");

    cli.batch_idle = Duration::from_millis(10);
    thread::sleep(Duration::from_millis(30));
    assert!(matches!(
        cli.read_blob(&missing),
        Err(GitError::MissingObject { .. })
    ));
    assert_ne!(generation(&cli), first, "an idle child must be replaced");
}

/// A stop kills the git alias, its shell and the shell's background child.
#[test]
fn stop_under_kills_a_plain_git_tree_and_returns_paused() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let alias = wait_alias();
    let args = ["-c", alias.as_str(), "navo-wait"];
    let spec = spec(Some(&repo), Duration::from_secs(5));

    let result = thread::scope(|scope| {
        let handle = scope.spawn(|| run_process(&spec, &args));
        let parent = wait_for_file(&repo, "parent.pid");
        let child = wait_for_file(&repo, "child.pid");
        assert!(running(&parent), "the alias shell must be alive");
        assert!(running(&child), "the background child must be alive");

        let started = Instant::now();
        let pause = stop_under(&repo, Duration::from_secs(2)).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "the stop took {:?}",
            started.elapsed()
        );

        let result = handle.join().unwrap();
        assert_gone(&parent);
        assert_gone(&child);
        drop(pause);
        result
    });

    assert!(
        matches!(result, Err(GitError::Paused)),
        "a stopped command must return Paused, got {result:?}"
    );
}

/// Dropping a live tree stops its process group.
#[test]
fn dropping_a_git_tree_kills_a_live_alias_group() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let alias = wait_alias();
    let spec = spec(Some(&repo), Duration::from_secs(5));
    let mut command = base_command(&spec);
    command.args(["-c", alias.as_str(), "navo-wait"]);
    let tree = GitTree::spawn(&mut command).unwrap();

    let parent = wait_for_file(&repo, "parent.pid");
    let child = wait_for_file(&repo, "child.pid");
    assert!(running(&parent), "the alias shell must be alive");
    assert!(running(&child), "the background child must be alive");
    drop(tree);
    assert_gone(&parent);
    assert_gone(&child);
}

/// A pause refuses the child before it spawns, so no pid file appears.
#[test]
fn a_live_pause_refuses_the_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let pause = stop_under(&repo, Duration::from_millis(100)).unwrap();
    let args = ["-c", "alias.navo-spawn=!echo $$ > parent.pid", "navo-spawn"];
    let spec = spec(Some(&repo), Duration::from_secs(1));

    let result = run_process(&spec, &args);

    assert!(matches!(result, Err(GitError::Paused)));
    assert!(
        !repo.join("parent.pid").exists(),
        "nothing may spawn under a pause"
    );
    drop(pause);
}

/// A child with no repository is not registered, so a pause leaves it running.
#[test]
fn a_run_without_a_repository_registers_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let marker = repo.join("no-repo-started.txt");
    let alias = format!(
        "alias.navo-none=!printf started > '{}'; sleep 20",
        marker.to_string_lossy().replace('\\', "/")
    );
    let args = ["-c", alias.as_str(), "navo-none"];
    let spec = spec(None, Duration::from_secs(2));

    let result = thread::scope(|scope| {
        let handle = scope.spawn(|| run_process(&spec, &args));
        wait_for_file(&repo, "no-repo-started.txt");
        let started = Instant::now();
        let pause = stop_under(&repo, Duration::from_millis(50)).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "an unregistered child must not hold the pause"
        );
        drop(pause);
        handle.join().unwrap()
    });

    assert!(
        matches!(result, Err(GitError::Timeout)),
        "a child with no repository must keep running until its timeout"
    );
}

/// A stop set after the readers join still returns Paused.
#[test]
fn a_stop_after_the_readers_join_returns_paused() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);
    hook_after_readers(|stop| {
        stop.as_ref()
            .expect("the run has a registry ticket")
            .cancel();
    });
    let spec = spec(Some(&repo), Duration::from_secs(2));
    let options = ProcessOptions {
        stdout_cap: 65536,
        timeout: Duration::from_secs(2),
    };
    let result = run_cancellable(&spec, &["--version"], &Cancel::default(), &options);
    assert!(matches!(result, Err(GitError::Paused)));
}

/// A Windows exit ends the job before the ticket drops.
#[cfg(windows)]
#[test]
fn a_windows_exit_terminates_the_job_before_dropping_the_ticket() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let alias = "alias.navo-child=!sleep 60 >/dev/null 2>&1 &\nchild=$!\ni=0\nwhile [ ! -r /proc/$child/winpid ] && [ $i -lt 20 ]; do\nsleep 0.05\ni=$((i + 1))\ndone\ncat /proc/$child/winpid > child.pid\nexit 0";
    let spec = spec(Some(&repo), Duration::from_secs(5));
    let mut command = base_command(&spec);
    command.args(["-c", alias, "navo-child"]);
    let mut tree = GitTree::spawn(&mut command).unwrap();
    let child = wait_for_file(&repo, "child.pid");
    let started = Instant::now();
    loop {
        if tree.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the alias did not exit"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_gone(&child);
}

/// A stop that lands after the reap still returns Paused.
#[test]
fn a_stop_after_the_reap_returns_paused() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo);
    hook_after_reap(|stop| stop.expect("the run has a registry ticket").cancel());
    let spec = spec(Some(&repo), Duration::from_secs(2));
    let result = run_process(&spec, &["--version"]);
    assert!(matches!(result, Err(GitError::Paused)));
}

/// The generation of the live batch child; panics when no child is open.
fn generation(cli: &CliGit) -> u64 {
    cli.session
        .lock()
        .expect("the batch session lock is not poisoned")
        .as_ref()
        .expect("the batch child is alive")
        .generation()
}
