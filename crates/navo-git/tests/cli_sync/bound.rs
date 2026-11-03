use navo_git::{Cancel, GitError, SyncKind};

use super::cli_remote_support::{fetch, push_from_clone, repo, sibling, with_origin};
use super::cli_sync_support::{DEADLINE, cli, commit_file, confirm, refs_of, snapshot, watched};

#[test]
fn a_commit_made_after_the_push_confirmation_refuses_and_sends_nothing() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    commit_file(&fixture, "one.txt", b"1\n");
    let (cli, log) = watched(&fixture);
    let target = confirm(&cli, SyncKind::Push).unwrap();
    commit_file(&fixture, "two.txt", b"2\n");
    let before = refs_of(&fixture, &origin);
    let error = cli
        .push_with(&target, &Cancel::default(), DEADLINE)
        .unwrap_err();
    assert!(matches!(error, GitError::TargetChanged), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[test]
fn a_branch_switch_after_the_pull_confirmation_refuses_and_pulls_nothing() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let (cli, log) = watched(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    fixture.git_ok(&["checkout", "-q", "-b", "other", "--track", "origin/main"]);
    let before = snapshot(&fixture);
    let error = cli
        .pull_with(&target, &Cancel::default(), DEADLINE)
        .unwrap_err();
    assert!(matches!(error, GitError::TargetChanged), "{error:?}");
    log.assert_not_ran(" fetch ");
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn an_upstream_changed_after_the_push_confirmation_refuses() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    commit_file(&fixture, "one.txt", b"1\n");
    let (cli, log) = watched(&fixture);
    let target = confirm(&cli, SyncKind::Push).unwrap();
    fixture.git_ok(&["config", "branch.main.merge", "refs/heads/elsewhere"]);
    let before = refs_of(&fixture, &origin);
    let error = cli
        .push_with(&target, &Cancel::default(), DEADLINE)
        .unwrap_err();
    assert!(matches!(error, GitError::TargetChanged), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[test]
fn the_push_preview_says_when_it_creates_the_remote_branch() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let cli = cli(&fixture);
    let guard = cli.guard().unwrap();
    assert!(!cli.sync_preview(SyncKind::Push, &guard).unwrap().creates);
    fixture.git_ok(&["checkout", "-q", "-b", "topic"]);
    fixture.git_ok(&["config", "branch.topic.remote", "origin"]);
    fixture.git_ok(&["config", "branch.topic.merge", "refs/heads/topic"]);
    commit_file(&fixture, "topic.txt", b"topic\n");
    assert!(cli.sync_preview(SyncKind::Push, &guard).unwrap().creates);
    assert!(!cli.sync_preview(SyncKind::Pull, &guard).unwrap().creates);
}

#[test]
fn the_pull_preview_names_when_its_count_was_fetched() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let cli = cli(&fixture);
    let guard = cli.guard().unwrap();
    let preview = cli.sync_preview(SyncKind::Pull, &guard).unwrap();
    assert_eq!((preview.commits, preview.fetched_ms), (0, None));
    fetch(&cli, &guard).unwrap();
    let preview = cli.sync_preview(SyncKind::Pull, &guard).unwrap();
    assert_eq!(preview.commits, 1);
    assert!(preview.fetched_ms.is_some_and(|ms| ms > 0));
}

#[test]
fn a_remote_url_changed_after_the_push_confirmation_refuses_and_sends_nothing() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    commit_file(&fixture, "one.txt", b"1\n");
    let (cli, log) = watched(&fixture);
    let target = confirm(&cli, SyncKind::Push).unwrap();
    let elsewhere = sibling(&fixture, "elsewhere.git");
    fixture.git_ok(&["init", "--bare", "-b", "main", elsewhere.to_str().unwrap()]);
    fixture.git_ok(&["config", "remote.origin.url", elsewhere.to_str().unwrap()]);
    let before = (refs_of(&fixture, &origin), refs_of(&fixture, &elsewhere));
    let error = cli
        .push_with(&target, &Cancel::default(), DEADLINE)
        .unwrap_err();
    assert!(matches!(error, GitError::GuardChanged), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(
        before,
        (refs_of(&fixture, &origin), refs_of(&fixture, &elsewhere))
    );
}

#[test]
fn a_push_url_or_fetch_refspec_changed_after_the_pull_confirmation_refuses() {
    for change in [
        ["config", "--add", "remote.origin.pushurl", "/nowhere"],
        [
            "config",
            "--add",
            "remote.origin.fetch",
            "+refs/tags/*:refs/tags/*",
        ],
    ] {
        let Some(fixture) = repo() else { return };
        let origin = with_origin(&fixture);
        push_from_clone(&fixture, &origin, "clone2");
        let (cli, log) = watched(&fixture);
        let target = confirm(&cli, SyncKind::Pull).unwrap();
        fixture.git_ok(&change);
        let before = snapshot(&fixture);
        let error = cli
            .pull_with(&target, &Cancel::default(), DEADLINE)
            .unwrap_err();
        assert!(
            matches!(error, GitError::GuardChanged),
            "{change:?}: {error:?}"
        );
        log.assert_not_ran(" fetch ");
        assert_eq!(snapshot(&fixture), before);
    }
}

#[test]
fn a_tracking_ref_moved_after_the_push_confirmation_refuses_a_changed_count() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    commit_file(&fixture, "one.txt", b"1\n");
    let (cli, log) = watched(&fixture);
    let target = confirm(&cli, SyncKind::Push).unwrap();
    fixture.git_ok(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let before = refs_of(&fixture, &origin);
    let error = cli
        .push_with(&target, &Cancel::default(), DEADLINE)
        .unwrap_err();
    assert!(matches!(error, GitError::TargetChanged), "{error:?}");
    log.assert_not_ran(" push ");
    assert_eq!(refs_of(&fixture, &origin), before);
}
