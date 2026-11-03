use std::fs;

use navo_git::{Cancel, GitError, SyncKind, SyncSummary};

use super::cli_remote_support::{fetch, push_from_clone, repo, status, with_origin};
use super::cli_sync_support::{
    DEADLINE, change_on_origin, change_while_stalled, cli, commit_file, confirm, pull, snapshot,
    stalling_fetch,
};

#[test]
fn a_fast_forward_pull_moves_head_and_counts_the_commits() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    push_from_clone(&fixture, &origin, "clone3");
    let cli = cli(&fixture);
    let summary = pull(&cli).unwrap();
    assert_eq!(
        summary,
        SyncSummary {
            kind: SyncKind::Pull,
            branch: "main".into(),
            remote: "origin".into(),
            commits: 2,
            tracking_stale: false,
        }
    );
    let origin_main = fixture.git_ok(&["-C", origin.to_str().unwrap(), "rev-parse", "main"]);
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    assert_eq!(head, origin_main.stdout);
    assert_eq!(fixture.git_ok(&["rev-parse", "FETCH_HEAD"]).stdout, head);
    assert_eq!(fixture.git_ok(&["rev-parse", "origin/main"]).stdout, head);
    assert_eq!(
        fs::read(fixture.repo().join("clone3.txt")).unwrap(),
        b"remote\n"
    );
    let upstream = status(&cli).upstream.unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (0, 0));
}

#[test]
fn a_tracking_ref_moved_during_the_pull_stays_where_it_was_moved() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let base = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    push_from_clone(&fixture, &origin, "clone2");
    let cli = cli(&fixture);
    fetch(&cli, &cli.guard().unwrap()).unwrap();
    let fetched = fixture.git_ok(&["rev-parse", "origin/main"]).stdout;
    fixture.git_ok(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    push_from_clone(&fixture, &origin, "clone3");
    let files = stalling_fetch(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let elsewhere = String::from_utf8(fetched).unwrap();
    let move_it = || {
        fixture.git_ok(&["update-ref", "refs/remotes/origin/main", elsewhere.trim()]);
    };
    let summary = change_while_stalled((&files.0, &files.1), move_it, || {
        cli.pull_with(&target, &Cancel::default(), DEADLINE)
    })
    .unwrap();
    assert_eq!(summary.commits, 2);
    assert!(summary.tracking_stale);
    let tracking = fixture.git_ok(&["rev-parse", "origin/main"]).stdout;
    assert_eq!(String::from_utf8(tracking).unwrap(), elsewhere);
    assert_ne!(fixture.git_ok(&["rev-parse", "HEAD"]).stdout, base);
}

#[test]
fn an_up_to_date_pull_reports_no_commits_and_changes_nothing() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let before = snapshot(&fixture);
    let cli = cli(&fixture);
    assert_eq!(pull(&cli).unwrap().commits, 0);
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn a_diverged_pull_returns_not_fast_forward_and_changes_nothing() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    commit_file(&fixture, "local.txt", b"local\n");
    fixture.git_ok(&["config", "pull.rebase", "true"]);
    fixture.git_ok(&["config", "pull.ff", "false"]);
    let before = snapshot(&fixture);
    let cli = cli(&fixture);
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::NotFastForward), "{error:?}");
    assert_eq!(snapshot(&fixture), before);
    assert!(!fixture.repo().join("clone2.txt").exists());
}

#[test]
fn a_pull_that_was_diverged_before_the_fetch_also_changes_nothing() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let cli = cli(&fixture);
    fetch(&cli, &cli.guard().unwrap()).unwrap();
    commit_file(&fixture, "local.txt", b"local\n");
    let before = snapshot(&fixture);
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::NotFastForward), "{error:?}");
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn a_local_change_the_pull_would_overwrite_stays_as_it_was() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    change_on_origin(&fixture, &origin, "clone2", ("a.txt", b"theirs\n"));
    fixture.write("a.txt", b"mine, not committed\n");
    let before = snapshot(&fixture);
    let cli = cli(&fixture);
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::LocalChanges), "{error:?}");
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn an_untracked_file_the_pull_would_overwrite_stays_as_it_was() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    fixture.write("clone2.txt", b"my own untracked file\n");
    let before = snapshot(&fixture);
    let cli = cli(&fixture);
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::LocalChanges), "{error:?}");
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn a_local_change_the_pull_does_not_touch_is_kept() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    fixture.write("a.txt", b"mine, not committed\n");
    let cli = cli(&fixture);
    assert_eq!(pull(&cli).unwrap().commits, 1);
    assert_eq!(
        fs::read(fixture.repo().join("a.txt")).unwrap(),
        b"mine, not committed\n"
    );
}
