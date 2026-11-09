//! Portable checks for the batch command and its idle expiry.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::{CliGit, GitError, GitSource, ObjectId};

/// Creates a repository in `repo` and asserts that `git init` succeeded.
fn init_repo(repo: &Path) {
    fs::create_dir(repo).unwrap();
    let git = if cfg!(windows) { "git.exe" } else { "git" };
    let output = Command::new(git)
        .arg("-C")
        .arg(repo)
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success(), "git init: {output:?}");
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

/// The generation of the live batch child; panics when no child is open.
fn generation(cli: &CliGit) -> u64 {
    cli.session
        .lock()
        .expect("the batch session lock is not poisoned")
        .as_ref()
        .expect("the batch child is alive")
        .generation()
}
