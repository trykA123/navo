use std::time::Duration;

use navo_git::{Cancel, GitError, SyncKind};

use super::cli_remote_origin::{closed_port, with_url_origin};
use super::cli_remote_support::{repo, with_origin};
use super::cli_sync_support::{cli, confirm, pull, snapshot};

#[cfg(unix)]
use super::cli_remote_support::wrapped;
#[cfg(unix)]
use super::cli_sync_support::{DEADLINE, calls};

#[cfg(target_os = "linux")]
use super::cli_remote_challenge::Challenge;
#[cfg(target_os = "linux")]
use std::sync::atomic::Ordering;

#[cfg(unix)]
#[test]
fn the_pull_runs_the_command_of_d100g_and_nothing_more() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let (cli, log) = wrapped(&fixture, "");
    pull(&cli).unwrap();
    let all = calls(&log);
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    let head = String::from_utf8(head).unwrap();
    let fetch = all.lines().find(|line| line.contains(" fetch ")).unwrap();
    let merge = all.lines().find(|line| line.contains(" merge ")).unwrap();
    let prefix = "-c credential.interactive=never -c gc.auto=0 -c maintenance.auto=false";
    assert!(
        fetch.ends_with(&format!(
            "{prefix} -c core.hooksPath= -c fetch.pruneTags=false -c remote.origin.pruneTags=false fetch --no-all --no-recurse-submodules --no-prune --no-tags --write-fetch-head --refmap= --no-auto-maintenance --end-of-options origin refs/heads/main"
        )),
        "{fetch}"
    );
    assert!(
        merge.ends_with(&format!(
            "{prefix} -c submodule.recurse=false merge --ff-only --no-autostash --no-squash --no-stat --end-of-options {}",
            head.trim()
        )),
        "{merge}"
    );
    assert_eq!(all.matches(" fetch ").count(), 1);
    assert_eq!(all.matches(" merge ").count(), 1);
    assert!(!all.contains(" pull "), "{all}");
    assert!(!all.contains(" update-ref "), "{all}");
    for word in [
        "--rebase",
        "--force",
        "--prune ",
        "--tags",
        "--all ",
        "--autostash",
    ] {
        assert!(!fetch.contains(word) && !merge.contains(word), "{word}");
    }
}

#[cfg(unix)]
#[test]
fn no_upstream_and_a_detached_head_are_refused_before_any_pull() {
    let Some(fixture) = repo() else { return };
    let (cli, log) = wrapped(&fixture, "");
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::NoUpstream), "{error:?}");
    with_origin(&fixture);
    fixture.git_ok(&["checkout", "--detach"]);
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::DetachedHead), "{error:?}");
    assert!(!calls(&log).contains(" fetch "), "a pull started");
}

#[cfg(unix)]
#[test]
fn a_local_upstream_is_refused_before_any_pull() {
    let Some(fixture) = repo() else { return };
    fixture.git_ok(&["branch", "other"]);
    fixture.git_ok(&["config", "branch.main.remote", "."]);
    fixture.git_ok(&["config", "branch.main.merge", "refs/heads/other"]);
    let (cli, log) = wrapped(&fixture, "");
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::NoUpstream), "{error:?}");
    assert!(!calls(&log).contains(" fetch "), "a pull started");
}

#[cfg(unix)]
#[test]
fn a_remote_name_with_an_equals_sign_is_refused_before_any_pull() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    fixture.git_ok(&["remote", "rename", "origin", "a=b"]);
    let (cli, log) = wrapped(&fixture, "");
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::UnsafeRemote { .. }), "{error:?}");
    assert!(!calls(&log).contains(" fetch "), "a pull started");
    assert!(origin.exists());
}

#[cfg(unix)]
#[test]
fn a_config_changed_after_the_confirmation_is_refused_before_any_pull() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let (cli, log) = wrapped(&fixture, "");
    let confirmed = confirm(&cli, SyncKind::Pull).unwrap();
    fixture.git_ok(&["config", "core.sshCommand", "true"]);
    let error = cli
        .pull_with(&confirmed, &Cancel::default(), DEADLINE)
        .unwrap_err();
    assert!(matches!(error, GitError::GuardChanged), "{error:?}");
    assert!(!calls(&log).contains(" fetch "), "a pull started");
}

#[test]
fn a_cancelled_token_stops_before_the_pull() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let cli = cli(&fixture);
    let cancel = Cancel::default();
    cancel.cancel();
    let error = cli
        .pull_with(
            &confirm(&cli, SyncKind::Pull).unwrap(),
            &cancel,
            Duration::from_secs(20),
        )
        .unwrap_err();
    assert!(matches!(error, GitError::Cancelled), "{error:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn an_origin_that_asks_for_credentials_fails_typed_without_a_prompt() {
    let Some(fixture) = repo() else { return };
    let server = Challenge::start();
    with_url_origin(&fixture, &server.url());
    let before = snapshot(&fixture);
    let cli = cli(&fixture);
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::AuthFailed), "{error:?}");
    assert!(
        server.hits.load(Ordering::Acquire) >= 1,
        "the server was never asked"
    );
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn an_origin_that_is_not_listening_fails_and_changes_nothing() {
    let Some(fixture) = repo() else { return };
    let url = format!("http://127.0.0.1:{}/repo.git", closed_port());
    with_url_origin(&fixture, &url);
    let before = snapshot(&fixture);
    let cli = cli(&fixture);
    let error = pull(&cli).unwrap_err();
    assert!(matches!(error, GitError::Command { .. }), "{error:?}");
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn an_unreachable_origin_with_a_local_commit_is_a_failure_not_a_merge_need() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let url = format!("http://127.0.0.1:{}/repo.git", closed_port());
    fixture.git_ok(&["remote", "set-url", "origin", &url]);
    super::cli_sync_support::commit_file(&fixture, "local.txt", b"local\n");
    let before = snapshot(&fixture);
    let error = pull(&cli(&fixture)).unwrap_err();
    assert!(matches!(error, GitError::Command { .. }), "{error:?}");
    assert_eq!(snapshot(&fixture), before);
}
