use std::time::Duration;

use navo_git::{Cancel, GitError, SyncKind};

use super::cli_remote_support::{repo, with_origin};
use super::cli_sync_support::{
    DEADLINE, cancel_when, cli, commit_file, confirm, hook, nothing_left, push, refs_of, slashed,
    stalling_hook, watched,
};

#[test]
fn no_upstream_and_a_detached_head_are_refused_before_any_push() {
    let Some(fixture) = repo() else { return };
    let (cli, log) = watched(&fixture);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::NoUpstream), "{error:?}");
    let origin = with_origin(&fixture);
    let before = refs_of(&fixture, &origin);
    commit_file(&fixture, "one.txt", b"1\n");
    fixture.git_ok(&["checkout", "--detach"]);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::DetachedHead), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[test]
fn a_repository_hook_runs_in_a_confirmed_push_and_the_confirmation_names_it() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let (hooks, marker) = hook(&fixture, "pre-push", 0);
    fixture.git_ok(&["config", "core.hooksPath", &slashed(&hooks)]);
    commit_file(&fixture, "one.txt", b"1\n");
    let cli = cli(&fixture);
    let guard = cli.guard().unwrap();
    let preview = cli.sync_preview(SyncKind::Push, &guard).unwrap();
    assert_eq!(preview.guarded, vec!["core.hookspath".to_owned()]);
    assert_eq!(
        (preview.target.tracking.remote.as_str(), preview.commits),
        ("origin", 1)
    );
    assert!(!marker.exists());
    cli.push_with(&preview.target, &Cancel::default(), DEADLINE)
        .unwrap();
    assert!(marker.exists(), "the user's pre-push hook was skipped");
}

#[test]
fn a_refusing_pre_push_hook_leaves_the_origin_as_it_was() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let (hooks, marker) = hook(&fixture, "pre-push", 1);
    fixture.git_ok(&["config", "core.hooksPath", &slashed(&hooks)]);
    commit_file(&fixture, "one.txt", b"1\n");
    let before = refs_of(&fixture, &origin);
    let cli = cli(&fixture);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::Command { .. }), "{error:?}");
    assert!(marker.exists());
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[cfg(target_os = "linux")]
#[test]
fn an_origin_that_asks_for_credentials_fails_typed_without_a_prompt() {
    let Some(fixture) = repo() else { return };
    let server = super::cli_remote_challenge::Challenge::start();
    super::cli_remote_origin::with_url_origin(&fixture, &server.url());
    let cli = cli(&fixture);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::AuthFailed), "{error:?}");
}

#[test]
fn cancel_and_the_deadline_kill_a_push_while_git_runs_and_leave_no_git() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let (hooks, marker) = stalling_hook(&fixture, "pre-push");
    fixture.git_ok(&["config", "core.hooksPath", &slashed(&hooks)]);
    commit_file(&fixture, "one.txt", b"1\n");
    let before = refs_of(&fixture, &origin);
    let cli = cli(&fixture);
    let target = confirm(&cli, SyncKind::Push).unwrap();
    let (result, took) = cancel_when(&marker, |cancel| cli.push_with(&target, cancel, DEADLINE));
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
    assert!(
        marker.exists() && took < Duration::from_secs(10),
        "{took:?}"
    );
    std::fs::remove_file(&marker).unwrap();
    let error = cli
        .push_with(&target, &Cancel::default(), Duration::from_secs(3))
        .unwrap_err();
    assert!(matches!(error, GitError::Timeout), "{error:?}");
    assert!(marker.exists(), "git was not running when the deadline hit");
    nothing_left(&fixture);
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[test]
fn a_mirror_remote_config_does_not_make_the_push_a_mirror() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    fixture.git_ok(&["push", "-q", "origin", "main:refs/heads/extra"]);
    fixture.git_ok(&["config", "remote.origin.mirror", "true"]);
    commit_file(&fixture, "one.txt", b"1\n");
    let extra = refs_of(&fixture, &origin)
        .lines()
        .find(|line| line.ends_with("refs/heads/extra"))
        .unwrap()
        .to_owned();
    let summary = push(&cli(&fixture)).unwrap();
    assert_eq!(summary.commits, 1);
    let after = refs_of(&fixture, &origin);
    assert!(after.lines().any(|line| line == extra), "{after}");
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    let head = String::from_utf8(head).unwrap();
    let main = after.lines().find(|line| line.ends_with("refs/heads/main"));
    assert!(main.unwrap().starts_with(head.trim()), "{after}");
}

#[test]
fn a_remote_name_with_an_equals_sign_is_refused_before_any_push() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    fixture.git_ok(&["remote", "rename", "origin", "a=b"]);
    commit_file(&fixture, "one.txt", b"1\n");
    let before = refs_of(&fixture, &origin);
    let (cli, log) = watched(&fixture);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::UnsafeRemote { .. }), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(refs_of(&fixture, &origin), before);
}

fn topic_tracking_origin(fixture: &super::fixture::Fixture) {
    fixture.git_ok(&["checkout", "-q", "-b", "topic"]);
    commit_file(fixture, "topic.txt", b"topic\n");
    fixture.git_ok(&["push", "-q", "-u", "origin", "topic"]);
}

#[test]
fn a_remote_branch_deleted_after_a_merged_pr_is_not_recreated() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    topic_tracking_origin(&fixture);
    let origin_dir = origin.to_str().unwrap();
    fixture.git_ok(&["-C", origin_dir, "branch", "-D", "topic"]);
    commit_file(&fixture, "more.txt", b"more\n");
    let before = refs_of(&fixture, &origin);
    let (cli, log) = watched(&fixture);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::RemoteBranchGone), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[test]
fn a_remote_branch_that_still_exists_is_pushed_after_the_check() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    topic_tracking_origin(&fixture);
    commit_file(&fixture, "more.txt", b"more\n");
    let (cli, log) = watched(&fixture);
    let summary = push(&cli).unwrap();
    assert_eq!(summary.commits, 1);
    log.assert_ran(" ls-remote ");
}

#[test]
fn every_push_address_must_still_hold_the_branch() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    topic_tracking_origin(&fixture);
    let second = super::cli_remote_support::sibling(&fixture, "second.git");
    let second_dir = second.to_str().unwrap();
    fixture.git_ok(&["init", "--bare", "-b", "main", second_dir]);
    fixture.git_ok(&["push", "-q", second_dir, "main"]);
    let origin_dir = origin.to_str().unwrap();
    fixture.git_ok(&["config", "--add", "remote.origin.pushurl", origin_dir]);
    fixture.git_ok(&["config", "--add", "remote.origin.pushurl", second_dir]);
    commit_file(&fixture, "more.txt", b"more\n");
    let before = (refs_of(&fixture, &origin), refs_of(&fixture, &second));
    let (cli, log) = watched(&fixture);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::RemoteBranchGone), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(
        before,
        (refs_of(&fixture, &origin), refs_of(&fixture, &second))
    );
    fixture.git_ok(&["push", "-q", second_dir, "topic@{u}:refs/heads/topic"]);
    assert_eq!(push(&cli).unwrap().commits, 1);
}
