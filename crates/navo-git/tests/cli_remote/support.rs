use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use navo_git::status::{GitStatus, RepoStatus};
use navo_git::trust::Guard;
use navo_git::{Cancel, CliGit, GitError};

use super::fixture::Fixture;

const DEADLINE: Duration = Duration::from_secs(20);

/// A repository on `main` with one commit, or `None` only when Git is not installed.
#[allow(clippy::print_stderr, reason = "a skipped test says why")]
pub fn repo() -> Option<Fixture> {
    let fixture = match Fixture::new() {
        Ok(fixture) => fixture,
        Err(error) if error.starts_with("git was not found") => {
            let name = if cfg!(windows) { "git.exe" } else { "git" };
            eprintln!("skipped: {name} not found");
            return None;
        }
        Err(error) => panic!("git fixture setup failed: {error}"),
    };
    fixture.init(&["-b", "main"]);
    fixture.write("a.txt", b"base\n");
    fixture.git_ok(&["add", "a.txt"]);
    fixture.commit("base");
    Some(fixture)
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

/// A second clone pushes one commit to `origin`; `fixture` does not fetch it.
pub fn push_from_clone(fixture: &Fixture, origin: &Path, name: &str) {
    let clone = sibling(fixture, name);
    fixture.git_ok(&["clone", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    fs::write(clone.join(format!("{name}.txt")), b"remote\n").unwrap();
    let clone = clone.to_str().unwrap();
    fixture.git_ok(&["-C", clone, "add", "."]);
    fixture.git_ok(&["-C", clone, "commit", "-m", "remote"]);
    fixture.git_ok(&["-C", clone, "push", "origin", "main"]);
}

pub fn status(cli: &CliGit) -> RepoStatus {
    let guard = cli.guard().unwrap();
    cli.status(&guard, &Cancel::default()).unwrap()
}

pub fn fetch(cli: &CliGit, guard: &Guard) -> Result<(), GitError> {
    cli.fetch_with(guard, &Cancel::default(), DEADLINE)
}

/// Processes whose working directory is inside `root`.
#[cfg(target_os = "linux")]
pub fn running_in(root: &Path) -> Vec<String> {
    let root = root.canonicalize().unwrap();
    let mut found = Vec::new();
    for entry in fs::read_dir("/proc").unwrap().flatten() {
        let Ok(cwd) = fs::read_link(entry.path().join("cwd")) else {
            continue;
        };
        if cwd.starts_with(&root) {
            let args = fs::read(entry.path().join("cmdline")).unwrap_or_default();
            found.push(String::from_utf8_lossy(&args).replace('\0', " "));
        }
    }
    found
}

/// A `git` wrapper that logs every call and what the fetch child sees, then runs the real git.
#[cfg(unix)]
pub fn wrapped(fixture: &Fixture, extra: &str) -> (CliGit, PathBuf) {
    let log = sibling(fixture, "git.log");
    let env = sibling(fixture, "git.env");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{}'\ncase \" $* \" in *\" fetch \"*) echo \"$HOME|${{USERPROFILE-unset}}|$GCM_INTERACTIVE|$SSH_ASKPASS_REQUIRE|$GIT_TERMINAL_PROMPT|${{GIT_ASKPASS-unset}}\" > '{}' ;; esac\n{extra}\nexec git \"$@\"\n",
        log.display(),
        env.display()
    );
    let options = navo_git::CliOptions {
        git: Some(fixture.script("git-wrap", &script)),
        ..navo_git::CliOptions::default()
    };
    (CliGit::with_options(fixture.repo(), &options).unwrap(), log)
}
