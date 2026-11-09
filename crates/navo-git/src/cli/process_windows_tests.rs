//! Windows regression for repository moves while the CLI backend keeps Git running.

use std::fs;
use std::io::Read;
use std::process::Command;
use std::time::{Duration, Instant};

use super::super::process_tree::GitTree;
use super::super::running::{self, Work};
use crate::{CliGit, GitError, GitSource, ObjectId};

#[test]
fn a_repository_folder_can_be_renamed_while_git_is_running() {
    let listing = tempfile::tempdir().unwrap();
    let root = listing.path().join("repo");
    let moved = listing.path().join("repo-renamed");
    fs::create_dir(&root).unwrap();
    let initialized = Command::new("git.exe")
        .arg("-C")
        .arg(&root)
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(initialized.status.success(), "git init: {initialized:?}");
    fs::rename(&root, &moved).unwrap();
    fs::rename(&moved, &root).unwrap();

    let git = CliGit::new(&root).unwrap();
    // A missing-object reply proves the batch child is ready without leaving a blob stream open.
    assert!(matches!(
        git.read_blob(&ObjectId::Sha1([0xff; 20])),
        Err(GitError::MissingObject { .. })
    ));
    let renamed = fs::rename(&root, &moved);
    drop(git);

    if renamed.is_err() {
        fs::rename(&root, &moved).expect("the same rename must succeed after Git exits");
    }
    assert!(
        renamed.is_ok(),
        "background Git must leave the listed child repository renameable: {renamed:?}"
    );
}

#[test]
fn a_repository_with_packed_objects_can_be_renamed_while_git_is_running() {
    let listing = tempfile::tempdir().unwrap();
    let root = listing.path().join("repo");
    let moved = listing.path().join("repo-renamed");
    fs::create_dir(&root).unwrap();
    run_git(&root, &["init", "-q"]);
    fs::write(root.join("packed.txt"), b"packed\n").unwrap();

    run_git(&root, &["add", "packed.txt"]);
    commit_all(&root);
    run_git(&root, &["gc", "-q"]);
    let packed = fs::read_dir(root.join(".git").join("objects").join("pack"))
        .unwrap()
        .flatten()
        .any(|entry| entry.path().extension().and_then(|name| name.to_str()) == Some("pack"));
    assert!(packed, "git gc must leave the objects in a pack");

    let git = CliGit::new(&root).unwrap();
    let entry = git
        .list_tree("HEAD")
        .unwrap()
        .into_iter()
        .find(|entry| entry.path.as_bytes() == b"packed.txt")
        .expect("the committed file is in the tree");
    let mut reader = git.read_blob(&entry.id).unwrap();
    let mut content = Vec::new();
    reader.read_to_end(&mut content).unwrap();
    drop(reader);
    assert_eq!(content, b"packed\n");

    let renamed = fs::rename(&root, &moved);
    if renamed.is_ok() {
        fs::rename(&moved, &root).expect("the packed repository must rename back");
    }
    drop(git);
    assert!(
        renamed.is_ok(),
        "background Git must leave the packed repository renameable: {renamed:?}"
    );
}

/// Runs one Git command in `repo` and fails the test when it exits non-zero.
fn run_git(repo: &std::path::Path, args: &[&str]) {
    let output = Command::new("git.exe")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// Commits the staged files with a fixed test identity.
fn commit_all(repo: &std::path::Path) {
    let identity = [
        "-c",
        "user.name=Navo",
        "-c",
        "user.email=navo@example.invalid",
    ];
    let args: Vec<&str> = identity
        .into_iter()
        .chain(["commit", "-q", "-m", "one"])
        .collect();
    run_git(repo, &args);
}

#[test]
fn a_finished_tree_has_no_active_process_before_stop_under_succeeds() {
    let listing = tempfile::tempdir().unwrap();
    let repo = listing.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let mut command = Command::new("cmd.exe");
    command.args([
        "/C",
        "start",
        "/B",
        "ping",
        "-n",
        "10",
        "127.0.0.1",
        ">",
        "nul",
    ]);
    let mut tree = GitTree::spawn(&mut command).unwrap();
    tree.attach(Some(running::register(&repo, Work::Read).unwrap()));

    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = tree.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "the wrapper did not exit");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success());
    assert_eq!(
        tree.active_processes().unwrap(),
        0,
        "the sleeping descendant must be gone before the ticket drops"
    );
    assert!(running::stop_under(&repo, Duration::ZERO).is_ok());
    tree.stop().unwrap();
}
