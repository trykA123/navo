//! Checks that a file-operation pause reaches a filter stream that is held open.

use std::fs;
use std::io::{self, Read};
use std::process::Command;
use std::time::{Duration, Instant};

use navo_git::{CliGit, CliOptions, GitError, GitPath, GitSource};

use super::fixture::Fixture;

/// Asserts that a pid recorded by the filter wrapper is gone.
fn assert_reaped(pid: &str) {
    assert!(
        !Command::new("kill")
            .args(["-0", pid])
            .output()
            .unwrap()
            .status
            .success(),
        "the filter process must be reaped"
    );
}

/// A repository with one file behind a smudge filter.
fn fixture() -> Fixture {
    let fixture = Fixture::new().expect("Git is required for paused filter tests");
    fixture.init(&["-b", "main"]);
    fixture.write(".gitattributes", b"data.bin filter=slow -text\n");
    fixture.write("data.bin", b"payload\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("paused");
    fixture
}

/// A Git wrapper whose `--filters` branch runs `body` instead of the real `cat-file`.
fn cli(fixture: &Fixture, body: &str, timeout: Duration) -> CliGit {
    let script = format!(
        "#!/bin/sh\nif [ \"$4\" = --filters ]; then\ncd \"$2\" || exit 1\necho $$ > .git/filter.pid\n{body}\nfi\nexec git \"$@\"\n"
    );
    CliGit::with_options(
        fixture.repo(),
        &CliOptions {
            git: Some(fixture.script("git-paused", &script)),
            timeout,
            ..CliOptions::default()
        },
    )
    .unwrap()
}

/// A held filter stream ends with `Paused` when a file operation pauses the repository.
#[test]
fn a_file_operation_pauses_a_held_filter_stream() {
    let fixture = fixture();
    let cli = cli(&fixture, "printf hello\nsleep 20", Duration::from_secs(2));
    let mut blob = cli
        .read_blob_worktree_form("HEAD", &GitPath::from("data.bin"))
        .unwrap();

    let started = Instant::now();
    let pause = navo_git::cli::stop_under(fixture.repo(), Duration::from_secs(2)).unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "the stop took {:?}",
        started.elapsed()
    );

    let filter = fs::read_to_string(fixture.repo().join(".git/filter.pid")).unwrap();
    assert_reaped(filter.trim());

    let mut bytes = Vec::new();
    let error = blob.reader.read_to_end(&mut bytes).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(matches!(
        error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<GitError>()),
        Some(GitError::Paused)
    ));
    assert_eq!(bytes, b"hello");
    drop(pause);
}
