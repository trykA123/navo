use std::fs;
use std::path::{Path, PathBuf};

use navo_git::status::{GitStatus, RepoStatus};
use navo_git::{Cancel, CliGit, GixGit};

use super::fixture::Fixture;

pub fn repo() -> Option<Fixture> {
    let fixture = match Fixture::new() {
        Ok(fixture) => fixture,
        Err(error) if error.starts_with("git was not found") => return None,
        Err(error) => panic!("git fixture setup failed: {error}"),
    };
    fixture.init(&["-b", "main"]);
    commit_file(&fixture, "a.txt", "base\n", "base");
    Some(fixture)
}

pub fn commit_file(fixture: &Fixture, name: &str, text: &str, message: &str) {
    fixture.write(name, text.as_bytes());
    fixture.git_ok(&["add", name]);
    fixture.commit(message);
}

pub fn sibling(fixture: &Fixture, name: &str) -> PathBuf {
    fixture.repo().with_file_name(name)
}

pub fn with_origin(fixture: &Fixture) -> PathBuf {
    let origin = sibling(fixture, "origin.git");
    fixture.git_ok(&["init", "--bare", "-b", "main", path(&origin)]);
    fixture.git_ok(&["remote", "add", "origin", path(&origin)]);
    fixture.git_ok(&["push", "-u", "origin", "main"]);
    origin
}

pub fn push_from_clone(fixture: &Fixture, origin: &Path) {
    let clone = sibling(fixture, "clone2");
    fixture.git_ok(&["clone", path(origin), path(&clone)]);
    fs::write(clone.join("remote.txt"), b"remote\n").unwrap();
    fixture.git_ok(&["-C", path(&clone), "add", "remote.txt"]);
    fixture.git_ok(&["-C", path(&clone), "commit", "-m", "remote"]);
    fixture.git_ok(&["-C", path(&clone), "push", "origin", "main"]);
    fixture.git_ok(&["fetch", "origin"]);
}

pub fn diverge_on_a(fixture: &Fixture) {
    fixture.git_ok(&["checkout", "-b", "other"]);
    commit_file(fixture, "a.txt", "other\n", "other");
    fixture.git_ok(&["checkout", "main"]);
    commit_file(fixture, "a.txt", "main\n", "main");
}

pub fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

pub fn cli_status(repo: &Path) -> RepoStatus {
    let cli = CliGit::new(repo).unwrap();
    let guard = cli.guard().unwrap();
    cli.status(&guard, &Cancel::default()).unwrap()
}

pub fn gix_status(repo: &Path) -> RepoStatus {
    let gix = GixGit::new_isolated(repo).unwrap();
    let guard = gix.guard().unwrap();
    gix.status(&guard, &Cancel::default()).unwrap()
}
