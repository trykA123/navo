use std::fs;

use navo_git::{Cancel, GitError, SyncKind};

use super::cli_remote_support::{push_from_clone, repo, sibling, with_origin};
use super::cli_sync_support::{
    DEADLINE, change_while_stalled, cli, confirm, hook, logging_repo_hook, pull, slashed, snapshot,
    stalling_fetch,
};
use super::fixture::Fixture;

fn local_heads(fixture: &Fixture) -> String {
    let listed = fixture.git_ok(&[
        "for-each-ref",
        "--format=%(refname) %(objectname)",
        "refs/heads",
    ]);
    String::from_utf8(listed.stdout).unwrap()
}

fn head_of(fixture: &Fixture, branch: &str) -> Vec<u8> {
    fixture.git_ok(&["rev-parse", branch]).stdout
}

#[test]
fn a_branch_switched_at_the_same_commit_during_the_fetch_refuses_and_moves_no_branch() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let files = stalling_fetch(&fixture);
    let cli = cli(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let before = head_of(&fixture, "HEAD");
    let switch = || {
        fixture.git_ok(&["checkout", "-q", "-b", "other", "--track", "origin/main"]);
    };
    let result = change_while_stalled((&files.0, &files.1), switch, || {
        cli.pull_with(&target, &Cancel::default(), DEADLINE)
    });
    assert!(matches!(result, Err(GitError::TargetChanged)), "{result:?}");
    assert_eq!(head_of(&fixture, "refs/heads/main"), before);
    assert_eq!(head_of(&fixture, "refs/heads/other"), before);
    assert!(!fixture.repo().join("clone2.txt").exists());
}

#[test]
fn a_hooks_path_added_during_the_fetch_refuses_before_any_hook_runs() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let files = stalling_fetch(&fixture);
    let (hooks, marker) = hook(&fixture, "post-merge", 0);
    let cli = cli(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let before = snapshot(&fixture);
    let heads = local_heads(&fixture);
    let add_hooks = || {
        fixture.git_ok(&["config", "core.hooksPath", &slashed(&hooks)]);
    };
    let result = change_while_stalled((&files.0, &files.1), add_hooks, || {
        cli.pull_with(&target, &Cancel::default(), DEADLINE)
    });
    assert!(matches!(result, Err(GitError::GuardChanged)), "{result:?}");
    assert!(!marker.exists(), "a hook ran after the config changed");
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(local_heads(&fixture), heads);
}

#[test]
fn a_replaced_hooks_path_value_is_a_changed_config() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let (confirmed_hooks, marker) = hook(&fixture, "post-merge", 0);
    fixture.git_ok(&["config", "core.hooksPath", &slashed(&confirmed_hooks)]);
    let cli = cli(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let other = sibling(&fixture, "other-hooks");
    fs::create_dir_all(&other).unwrap();
    fs::copy(confirmed_hooks.join("post-merge"), other.join("post-merge")).unwrap();
    fixture.git_ok(&["config", "core.hooksPath", &slashed(&other)]);
    assert_ne!(cli.guard().unwrap(), target.guard);
    let before = snapshot(&fixture);
    let error = cli
        .pull_with(&target, &Cancel::default(), DEADLINE)
        .unwrap_err();
    assert!(matches!(error, GitError::GuardChanged), "{error:?}");
    assert!(!marker.exists(), "a hook ran under the replaced config");
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn a_reference_transaction_hook_and_hooks_path_added_during_the_fetch_never_run() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let files = stalling_fetch(&fixture);
    let (hooks, marker) = hook(&fixture, "reference-transaction", 0);
    let repo_log = sibling(&fixture, "repo-hook-ran");
    let cli = cli(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let before = snapshot(&fixture);
    let heads = local_heads(&fixture);
    let tracking = head_of(&fixture, "refs/remotes/origin/main");
    let add_hooks = || {
        logging_repo_hook(&fixture, "reference-transaction", &repo_log);
        fixture.git_ok(&["config", "core.hooksPath", &slashed(&hooks)]);
    };
    let result = change_while_stalled((&files.0, &files.1), add_hooks, || {
        cli.pull_with(&target, &Cancel::default(), DEADLINE)
    });
    assert!(matches!(result, Err(GitError::GuardChanged)), "{result:?}");
    assert!(!marker.exists(), "the hooks path hook ran");
    assert!(!repo_log.exists(), "the repository's own hook ran");
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(local_heads(&fixture), heads);
    assert_eq!(head_of(&fixture, "refs/remotes/origin/main"), tracking);
}

#[test]
fn only_the_merge_runs_the_users_reference_transaction_hook() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let log = sibling(&fixture, "transactions");
    logging_repo_hook(&fixture, "reference-transaction", &log);
    let cli = cli(&fixture);
    assert_eq!(pull(&cli).unwrap().commits, 1);
    let seen = fs::read_to_string(&log).unwrap();
    assert!(seen.contains("refs/heads/main"), "{seen}");
    assert!(!seen.contains("refs/remotes/"), "{seen}");
    assert_eq!(
        head_of(&fixture, "refs/remotes/origin/main"),
        head_of(&fixture, "HEAD")
    );
}
