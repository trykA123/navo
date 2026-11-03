use navo_git::{Cancel, CliGit, GitError};

use super::cli_remote_support::{fetch, repo, with_origin};

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
use navo_git::trust::Guard;

#[cfg(unix)]
use super::cli_remote_support::{sibling, wrapped};

#[cfg(unix)]
fn calls(log: &Path) -> String {
    fs::read_to_string(log).unwrap()
}

#[cfg(unix)]
fn assert_refused_before_any_process(cli: &CliGit, guard: &Guard, log: &Path) -> GitError {
    let seen = calls(log);
    let error = fetch(cli, guard).unwrap_err();
    assert_eq!(calls(log), seen, "git ran for a refused fetch");
    error
}

#[cfg(unix)]
#[test]
fn a_guarded_repository_is_refused_before_any_process_starts() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    fixture.git_ok(&["config", "core.sshCommand", "true"]);
    let (cli, log) = wrapped(&fixture, "");
    let guard = cli.guard().unwrap();
    assert!(guard.is_guarded() && !guard.is_blocked());
    let error = assert_refused_before_any_process(&cli, &guard, &log);
    assert!(matches!(error, GitError::Guarded { .. }), "{error:?}");
}

#[cfg(unix)]
#[test]
fn a_blocked_guard_is_refused_before_any_process_starts() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    fixture.git_ok(&["config", "filter.a=b.clean", "cat"]);
    let (cli, log) = wrapped(&fixture, "");
    let guard = cli.guard().unwrap();
    assert!(guard.is_blocked());
    let error = assert_refused_before_any_process(&cli, &guard, &log);
    assert!(matches!(error, GitError::NotTrusted { .. }), "{error:?}");
}

#[test]
fn a_branch_without_an_upstream_is_refused() {
    let Some(fixture) = repo() else { return };
    let cli = CliGit::new(fixture.repo()).unwrap();
    let error = fetch(&cli, &cli.guard().unwrap()).unwrap_err();
    assert!(matches!(error, GitError::NoUpstream), "{error:?}");
}

#[test]
fn a_detached_head_is_refused() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    fixture.git_ok(&["checkout", "--detach"]);
    let cli = CliGit::new(fixture.repo()).unwrap();
    let error = fetch(&cli, &cli.guard().unwrap()).unwrap_err();
    assert!(matches!(error, GitError::DetachedHead), "{error:?}");
}

#[test]
fn a_cancelled_token_stops_before_the_fetch() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let cli = CliGit::new(fixture.repo()).unwrap();
    let cancel = Cancel::default();
    cancel.cancel();
    let error = cli
        .fetch_with(
            &cli.guard().unwrap(),
            &cancel,
            std::time::Duration::from_secs(20),
        )
        .unwrap_err();
    assert!(matches!(error, GitError::Cancelled), "{error:?}");
}

#[cfg(unix)]
#[test]
fn an_upstream_name_starting_with_a_dash_is_refused() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let marker = sibling(&fixture, "marker");
    let url = origin.to_str().unwrap();
    fixture.git_ok(&["config", "remote.-evil.url", url]);
    fixture.git_ok(&[
        "config",
        "remote.-evil.fetch",
        "+refs/heads/*:refs/remotes/-evil/*",
    ]);
    fixture.git_ok(&["config", "branch.main.remote", "-evil"]);
    let (cli, log) = wrapped(&fixture, "");
    let guard = cli.guard().unwrap();
    let error = fetch(&cli, &guard).unwrap_err();
    assert!(matches!(error, GitError::UnsafeRemote { .. }), "{error:?}");
    assert!(!calls(&log).contains(" fetch "), "a fetch started");

    let option = format!("--upload-pack=touch {}", marker.display());
    fixture.git_ok(&["config", "branch.main.remote", &option]);
    let error = fetch(&cli, &guard).unwrap_err();
    assert!(error.to_string().contains("upstream"), "{error}");
    assert!(!marker.exists());
    assert!(!calls(&log).contains(" fetch "), "a fetch started");
}
