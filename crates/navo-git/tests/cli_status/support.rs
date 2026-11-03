use std::fs;
use std::path::PathBuf;

use navo_git::status::{GitStatus, RepoStatus};
use navo_git::{Cancel, CliGit};

#[cfg(unix)]
use navo_git::CliOptions;

use super::fixture::Fixture;

/// A repository on `main` with one commit, or `None` only when Git is not installed.
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

pub fn status(fixture: &Fixture) -> RepoStatus {
    let cli = CliGit::new(fixture.repo()).unwrap();
    let guard = cli.guard().unwrap();
    cli.status(&guard, &Cancel::default()).unwrap()
}

pub fn sibling(fixture: &Fixture, name: &str) -> PathBuf {
    fixture.repo().with_file_name(name)
}

/// Adds a local bare `origin`, pushes `main` and tracks it.
pub fn with_origin(fixture: &Fixture) -> PathBuf {
    let origin = sibling(fixture, "origin.git");
    fixture.git_ok(&["init", "--bare", "-b", "main", origin.to_str().unwrap()]);
    fixture.git_ok(&["remote", "add", "origin", origin.to_str().unwrap()]);
    fixture.git_ok(&["push", "-u", "origin", "main"]);
    origin
}

/// A second clone of `origin` pushes one commit, then `fixture` fetches it.
pub fn push_from_clone(fixture: &Fixture, origin: &std::path::Path) {
    let clone = sibling(fixture, "clone2");
    fixture.git_ok(&["clone", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    fs::write(clone.join("remote.txt"), b"remote\n").unwrap();
    let clone = clone.to_str().unwrap();
    fixture.git_ok(&["-C", clone, "add", "remote.txt"]);
    fixture.git_ok(&["-C", clone, "commit", "-m", "remote"]);
    fixture.git_ok(&["-C", clone, "push", "origin", "main"]);
    fixture.git_ok(&["fetch", "origin"]);
}

/// Merge conflict on `a.txt`: `other` and `main` both rewrite it.
pub fn diverge_on_a(fixture: &Fixture) {
    fixture.git_ok(&["checkout", "-b", "other"]);
    commit_file(fixture, "a.txt", "other\n", "other");
    fixture.git_ok(&["checkout", "main"]);
    commit_file(fixture, "a.txt", "main\n", "main");
}

#[cfg(unix)]
pub fn wrapped(fixture: &Fixture, extra: &str) -> (CliGit, PathBuf) {
    let log = sibling(fixture, "git.log");
    let env = sibling(fixture, "git.env");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{}'\ncase \" $* \" in *\" status \"*) echo \"$HOME|${{USERPROFILE-unset}}\" > '{}' ;; esac\n{extra}\nexec git \"$@\"\n",
        log.display(),
        env.display()
    );
    let cli = CliGit::with_options(
        fixture.repo(),
        &CliOptions {
            git: Some(fixture.script("git-wrap", &script)),
            ..CliOptions::default()
        },
    )
    .unwrap();
    (cli, log)
}
