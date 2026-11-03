use std::fs;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use navo_git::{Cancel, CliGit, CliOptions, GitError};

use super::cli_remote_origin::{closed_port, with_url_origin};
use super::cli_remote_support::{fetch, push_from_clone, repo, sibling, status, with_origin};

#[cfg(target_os = "linux")]
use super::cli_remote_challenge::Challenge;
#[cfg(target_os = "linux")]
use super::cli_remote_support::running_in;

#[cfg(unix)]
fn hook_dir(fixture: &super::fixture::Fixture) -> std::path::PathBuf {
    let dir = sibling(fixture, "hooks");
    fs::create_dir_all(&dir).unwrap();
    let marker = sibling(fixture, "hook-ran");
    let body = format!("#!/bin/sh\ntouch '{}'\n", marker.display());
    for name in [
        "reference-transaction",
        "post-fetch",
        "pre-auto-gc",
        "post-checkout",
    ] {
        let path = dir.join(name);
        fs::write(&path, &body).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

#[cfg(unix)]
#[test]
fn a_hook_path_from_the_global_config_runs_no_hook_in_a_fetch() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let hooks = hook_dir(&fixture);
    let marker = sibling(&fixture, "hook-ran");
    let global = sibling(&fixture, "global-config");
    fs::write(
        &global,
        format!(
            "[core]\n\thooksPath = {}\n[gc]\n\tauto = 1\n",
            hooks.display()
        ),
    )
    .unwrap();
    push_from_clone(&fixture, &origin, "clone2");
    fixture.git_ok(&[
        "-c",
        &format!("core.hooksPath={}", hooks.display()),
        "fetch",
        "-q",
        "origin",
    ]);
    assert!(
        marker.exists(),
        "the fixture never runs a hook, so the test proves nothing"
    );
    fs::remove_file(&marker).unwrap();
    push_from_clone(&fixture, &origin, "clone3");
    let options = CliOptions {
        env: vec![("GIT_CONFIG_GLOBAL".into(), global.into_os_string())],
        ..CliOptions::default()
    };
    let cli = CliGit::with_options(fixture.repo(), &options).unwrap();
    fetch(&cli, &cli.guard().unwrap()).unwrap();
    assert!(!marker.exists(), "a hook ran in Navo's fetch");
    assert_eq!(status(&cli).upstream.map(|u| u.behind), Some(2));
}

#[cfg(unix)]
#[test]
fn local_hook_config_is_refused_and_runs_no_hook() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let hooks = hook_dir(&fixture);
    let marker = sibling(&fixture, "hook-ran");
    let command = format!("touch {}", marker.display());
    fixture.git_ok(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
    fixture.git_ok(&["config", "hook.x.event", "reference-transaction"]);
    fixture.git_ok(&["config", "hook.x.command", &command]);
    fixture.git_ok(&["config", "gc.recentObjectsHook", &command]);
    push_from_clone(&fixture, &origin, "clone2");
    let cli = CliGit::new(fixture.repo()).unwrap();
    let guard = cli.guard().unwrap();
    assert!(guard.is_guarded());
    let error = fetch(&cli, &guard).unwrap_err();
    assert!(matches!(error, GitError::Guarded { .. }), "{error:?}");
    assert!(!marker.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn an_origin_that_is_not_listening_fails_inside_the_deadline_and_leaves_nothing() {
    let Some(fixture) = repo() else { return };
    with_url_origin(
        &fixture,
        &format!("http://127.0.0.1:{}/repo.git", closed_port()),
    );
    let cli = CliGit::new(fixture.repo()).unwrap();
    let started = Instant::now();
    let error = fetch(&cli, &cli.guard().unwrap()).unwrap_err();
    assert!(matches!(error, GitError::Command { .. }), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(
        running_in(fixture.repo().parent().unwrap()),
        Vec::<String>::new()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn an_origin_that_asks_for_credentials_fails_without_a_prompt() {
    let Some(fixture) = repo() else { return };
    let server = Challenge::start();
    with_url_origin(&fixture, &server.url());
    let cli = CliGit::new(fixture.repo()).unwrap();
    let started = Instant::now();
    let error = fetch(&cli, &cli.guard().unwrap()).unwrap_err();
    assert!(matches!(error, GitError::Command { .. }), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(
        server.hits.load(Ordering::Acquire) >= 1,
        "the server was never asked"
    );
    assert_eq!(
        running_in(fixture.repo().parent().unwrap()),
        Vec::<String>::new()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn cancel_kills_the_running_fetch() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let (cli, _) = super::cli_remote_support::wrapped(
        &fixture,
        "case \" $* \" in *\" fetch \"*) sleep 31.5 ;; esac",
    );
    let guard = cli.guard().unwrap();
    let cancel = Cancel::default();
    let trigger = cancel.clone();
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(400));
        trigger.cancel();
    });
    let started = Instant::now();
    let error = cli
        .fetch_with(&guard, &cancel, Duration::from_secs(20))
        .unwrap_err();
    canceller.join().unwrap();
    assert!(matches!(error, GitError::Cancelled), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(
        running_in(fixture.repo().parent().unwrap()),
        Vec::<String>::new()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_fetch_past_the_deadline_is_killed() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    let (cli, _) = super::cli_remote_support::wrapped(
        &fixture,
        "case \" $* \" in *\" fetch \"*) sleep 31.6 ;; esac",
    );
    let guard = cli.guard().unwrap();
    let started = Instant::now();
    let error = cli
        .fetch_with(&guard, &Cancel::default(), Duration::from_millis(500))
        .unwrap_err();
    assert!(matches!(error, GitError::Timeout), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(
        running_in(fixture.repo().parent().unwrap()),
        Vec::<String>::new()
    );
}
