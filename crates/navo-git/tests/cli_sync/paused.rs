//! Write sessions are refused while a file operation pauses their repository, a running
//! session makes the pause wait, and a session that ends in time lets the pause pass.

use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::thread;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

#[cfg(unix)]
use navo_git::CliGit;
use navo_git::cli::stop_under;
use navo_git::{Cancel, GitError, SyncKind};

#[cfg(unix)]
use super::cli_remote_support::sibling;
use super::cli_remote_support::{repo, with_origin};
#[cfg(unix)]
use super::cli_sync_support::isolated;
use super::cli_sync_support::{DEADLINE, cli, confirm, refs_of, snapshot};
use super::fixture::Fixture;

/// Blocks fetch, Pull and Push on `path` and checks none of them changed anything.
fn refused_under(fixture: &Fixture, path: &Path) {
    let origin = with_origin(fixture);
    let cli = cli(fixture);
    let guard = cli.guard().unwrap();
    let pull = confirm(&cli, SyncKind::Pull).unwrap();
    let push = confirm(&cli, SyncKind::Push).unwrap();
    let before = snapshot(fixture);
    let refs = refs_of(fixture, &origin);
    let fetch_head = fs::read(fixture.repo().join(".git/FETCH_HEAD")).ok();

    let pause = stop_under(path, Duration::from_millis(100)).unwrap();
    let fetched = cli.fetch_with(&guard, &Cancel::default(), DEADLINE);
    let pulled = cli.pull_with(&pull, &Cancel::default(), DEADLINE);
    let pushed = cli.push_with(&push, &Cancel::default(), DEADLINE);
    drop(pause);

    assert!(matches!(fetched, Err(GitError::Paused)), "{fetched:?}");
    assert!(matches!(pulled, Err(GitError::Paused)), "{pulled:?}");
    assert!(matches!(pushed, Err(GitError::Paused)), "{pushed:?}");
    assert_eq!(snapshot(fixture), before);
    assert_eq!(refs_of(fixture, &origin), refs);
    assert_eq!(
        fs::read(fixture.repo().join(".git/FETCH_HEAD")).ok(),
        fetch_head
    );
}

#[test]
fn a_pause_on_the_repository_refuses_fetch_pull_and_push() {
    let Some(fixture) = repo() else { return };
    let path = fixture.repo().to_path_buf();
    refused_under(&fixture, &path);
}

#[test]
fn a_pause_on_a_subfolder_refuses_fetch_pull_and_push() {
    let Some(fixture) = repo() else { return };
    let sub = fixture.repo().join("sub");
    fs::create_dir_all(&sub).unwrap();
    refused_under(&fixture, &sub);
}

/// A wrapper Git that records a session command and sleeps before running the real Git.
#[cfg(unix)]
fn slow_cli(fixture: &Fixture, marker: &Path) -> CliGit {
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do\n  case \"$a\" in\n    fetch|pull)\n      printf started > '{}'\n      sleep 0.6\n      break;;\n  esac\ndone\nexec git \"$@\"\n",
        marker.display()
    );
    let mut options = isolated(fixture);
    options.git = Some(fixture.script("git-slow", &script));
    CliGit::with_options(fixture.repo(), &options).unwrap()
}

/// Waits until the slow wrapper has started the session.
#[cfg(unix)]
fn wait_for_marker(marker: &Path) {
    let started = Instant::now();
    while !marker.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the slow session did not start"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// A running fetch makes `stop_under` wait and fail, and the fetch then finishes.
#[cfg(unix)]
#[test]
fn a_running_fetch_session_holds_stop_under_and_finishes() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let marker = sibling(&fixture, "fetch.started");
    let cli = slow_cli(&fixture, &marker);
    let guard = cli.guard().unwrap();
    let result = thread::scope(|scope| {
        let handle = scope.spawn(|| cli.fetch_with(&guard, &Cancel::default(), DEADLINE));
        wait_for_marker(&marker);
        let busy = stop_under(fixture.repo(), Duration::from_millis(100)).unwrap_err();
        assert_eq!(busy.repo.as_path(), fixture.repo());
        handle.join().unwrap()
    });
    assert!(
        result.is_ok(),
        "the fetch must survive the wait: {result:?}"
    );
    assert!(fixture.repo().join(".git/FETCH_HEAD").exists());
}

/// A Pull that ends within the wait lets `stop_under` succeed.
#[cfg(unix)]
#[test]
fn a_pull_session_that_ends_within_the_wait_lets_stop_under_pass() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let marker = sibling(&fixture, "pull.started");
    let cli = slow_cli(&fixture, &marker);
    let pull = confirm(&cli, SyncKind::Pull).unwrap();
    let result = thread::scope(|scope| {
        let handle = scope.spawn(|| cli.pull_with(&pull, &Cancel::default(), DEADLINE));
        wait_for_marker(&marker);
        let pause = stop_under(fixture.repo(), Duration::from_secs(2)).unwrap();
        drop(pause);
        handle.join().unwrap()
    });
    assert!(result.is_ok(), "the pull must finish: {result:?}");
}

#[test]
fn a_pause_after_a_session_finds_no_inner_read() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let sub = fixture.repo().join("sub");
    fs::create_dir_all(&sub).unwrap();
    let cli = cli(&fixture);
    let guard = cli.guard().unwrap();

    cli.fetch_with(&guard, &Cancel::default(), DEADLINE)
        .unwrap();
    assert!(
        stop_under(&sub, Duration::ZERO).is_ok(),
        "fetch left an inner read registered"
    );

    let pull = confirm(&cli, SyncKind::Pull).unwrap();
    cli.pull_with(&pull, &Cancel::default(), DEADLINE).unwrap();
    assert!(
        stop_under(&sub, Duration::ZERO).is_ok(),
        "pull left an inner read registered"
    );

    let push = confirm(&cli, SyncKind::Push).unwrap();
    cli.push_with(&push, &Cancel::default(), DEADLINE).unwrap();
    assert!(
        stop_under(&sub, Duration::ZERO).is_ok(),
        "push left an inner read registered"
    );
}
