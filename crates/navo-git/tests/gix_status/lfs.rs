use std::fs;
use std::time::{Duration, SystemTime};

use navo_git::status::{GitStatus, RepoStatus};
use navo_git::{Cancel, GitError, GixGit};
use sha2::{Digest, Sha256};

use super::fixture::Fixture;
use super::gix_status_support::{commit_file, repo};

const ATTRIBUTES: &[u8] = b"*.bin filter=lfs diff=lfs merge=lfs -text\n";
const CONTENT: &[u8] = b"payload one\n";

/// Defines the standard git-lfs driver in a user config; git-lfs itself is not needed.
fn define_filter(fixture: &Fixture, name: &str, clean: &str) {
    let config = fixture.repo().with_file_name("global.gitconfig");
    let text = format!(
        "[filter \"{name}\"]\n\tclean = {clean}\n\tsmudge = {clean}\n\tprocess = {clean}\n\
         \trequired = true\n"
    );
    fs::write(&config, text).unwrap();
    // SAFETY: the test process runs one test and spawns no thread that reads the environment.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &config);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
}

fn define_lfs(fixture: &Fixture) {
    define_filter(fixture, "lfs", "git-lfs clean -- %f");
}

pub fn pointer(content: &[u8]) -> Vec<u8> {
    if content.is_empty() {
        return Vec::new();
    }
    let hex = Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{hex}\nsize {}\n",
        content.len()
    )
    .into_bytes()
}

/// Commits `a.bin` as an LFS pointer for `CONTENT` with the worktree file in place and its stat
/// recorded, the state `git add` leaves with git-lfs installed.
fn lfs_repo() -> Option<Fixture> {
    let fixture = repo()?;
    commit_file(
        &fixture,
        ".gitattributes",
        std::str::from_utf8(ATTRIBUTES).unwrap(),
        "attributes",
    );
    fixture.write("a.bin", CONTENT);
    fixture.git_ok(&["add", "a.bin"]);
    let output = fixture.git_stdin_ok(&["hash-object", "-w", "--stdin"], &pointer(CONTENT));
    let id =
        gix::hash::ObjectId::from_hex(String::from_utf8_lossy(&output.stdout).trim().as_bytes())
            .unwrap();
    swap_index_id(&fixture, "a.bin", id);
    fixture.commit("lfs");
    swap_index_id(&fixture, "a.bin", id);
    Some(fixture)
}

fn swap_index_id(fixture: &Fixture, path: &str, id: gix::hash::ObjectId) {
    let repo = gix::open(fixture.repo()).unwrap();
    let mut index = repo.open_index().unwrap();
    for (entry, name) in index.entries_mut_with_paths() {
        if name == path.as_bytes() {
            entry.id = id;
            // git smudges the size of an entry written within the index's own second.
            entry.stat.size = u32::try_from(CONTENT.len()).unwrap();
        }
    }
    index.write(gix::index::write::Options::default()).unwrap();
}

fn status(fixture: &Fixture) -> Result<RepoStatus, GitError> {
    let gix = GixGit::new(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    gix.status(&guard, &Cancel::default())
}

fn age(fixture: &Fixture, name: &str, secs: u64) {
    fs::File::options()
        .write(true)
        .open(fixture.repo().join(name))
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn an_lfs_file_whose_stat_matches_is_clean() {
    let Some(fixture) = lfs_repo() else { return };
    define_lfs(&fixture);
    let status = status(&fixture).unwrap();
    assert_eq!((status.changed, status.untracked), (0, 0));
}

#[test]
fn touching_an_lfs_file_without_changing_it_leaves_it_clean() {
    let Some(fixture) = lfs_repo() else { return };
    age(&fixture, "a.bin", 7200);
    define_lfs(&fixture);
    assert_eq!(status(&fixture).unwrap().changed, 0);
}

#[test]
fn rewriting_an_lfs_file_with_the_same_content_leaves_it_clean() {
    let Some(fixture) = lfs_repo() else { return };
    fixture.write("a.bin", CONTENT);
    age(&fixture, "a.bin", 7200);
    define_lfs(&fixture);
    assert_eq!(status(&fixture).unwrap().changed, 0);
}

#[test]
fn changed_lfs_content_of_the_same_size_is_modified() {
    let Some(fixture) = lfs_repo() else { return };
    fixture.write("a.bin", b"payload two\n");
    age(&fixture, "a.bin", 7200);
    define_lfs(&fixture);
    assert_eq!(status(&fixture).unwrap().changed, 1);
}

#[test]
fn changed_lfs_content_of_another_size_is_modified() {
    let Some(fixture) = lfs_repo() else { return };
    fixture.write("a.bin", b"a longer payload\n");
    define_lfs(&fixture);
    assert_eq!(status(&fixture).unwrap().changed, 1);
}

#[test]
fn an_emptied_lfs_file_is_modified() {
    let Some(fixture) = lfs_repo() else { return };
    fixture.write("a.bin", b"");
    define_lfs(&fixture);
    assert_eq!(status(&fixture).unwrap().changed, 1);
}

#[test]
fn an_untracked_lfs_path_and_a_plain_edit_still_count() {
    let Some(fixture) = lfs_repo() else { return };
    fixture.write("new.bin", b"new\n");
    fixture.write("a.txt", b"edited\n");
    define_lfs(&fixture);
    let status = status(&fixture).unwrap();
    assert_eq!((status.changed, status.untracked), (1, 1));
}

#[test]
fn a_custom_filter_is_refused_and_named() {
    let Some(fixture) = lfs_repo() else { return };
    fixture.write(".gitattributes", b"*.txt filter=crypt\n");
    define_filter(&fixture, "crypt", "cat");
    match status(&fixture) {
        Err(GitError::FilterRefused { filter, .. }) => assert_eq!(filter, "crypt"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_lfs_driver_that_is_not_git_lfs_is_refused() {
    let Some(fixture) = lfs_repo() else { return };
    define_filter(&fixture, "lfs", "my-tool clean");
    match status(&fixture) {
        Err(GitError::FilterRefused { filter, .. }) => assert_eq!(filter, "lfs"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_lfs_file_over_the_size_cap_hands_the_repository_over() {
    let Some(fixture) = lfs_repo() else { return };
    let file = fs::File::options()
        .write(true)
        .open(fixture.repo().join("a.bin"))
        .unwrap();
    file.set_len(513 * 1024 * 1024).unwrap();
    drop(file);
    define_lfs(&fixture);
    match status(&fixture) {
        Err(GitError::FilterRefused { filter, .. }) => assert_eq!(filter, "lfs"),
        other => panic!("{other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn no_git_lfs_process_runs_for_a_changed_file() {
    let Some(fixture) = lfs_repo() else { return };
    let marker = fixture.repo().with_file_name("lfs-ran");
    let script = format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.display());
    let tool = fixture.script("git-lfs", &script);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::iter::once(tool.parent().unwrap().to_path_buf())
        .chain(std::env::split_paths(&path))
        .collect::<Vec<_>>();
    // SAFETY: the test process runs one test and spawns no thread that reads the environment.
    unsafe { std::env::set_var("PATH", std::env::join_paths(dirs).unwrap()) };
    fixture.write("a.bin", b"payload two\n");
    age(&fixture, "a.bin", 7200);
    define_lfs(&fixture);
    assert_eq!(status(&fixture).unwrap().changed, 1);
    assert!(!marker.exists(), "git-lfs was started");
}
