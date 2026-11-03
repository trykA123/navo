use std::fs;
use std::time::{Duration, SystemTime};

use navo_git::status::{GitStatus, Reason};
use navo_git::trust::{Guard, Refusal, refusal, untrusted_reason};
use navo_git::{Cancel, GitError, GixGit};

use super::fixture::Fixture;
use super::gix_status_support::{gix_status, repo};

#[cfg(unix)]
use super::gix_status_support::sibling;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
fn marker(fixture: &Fixture) -> PathBuf {
    sibling(fixture, "marker")
}

#[cfg(unix)]
fn program(fixture: &Fixture) -> PathBuf {
    let script = format!("#!/bin/sh\ntouch '{}'\ncat\n", marker(fixture).display());
    fixture.script("evil.sh", &script)
}

/// Same size as the committed file and an old mtime, so only reading the content shows the change.
fn edit(fixture: &Fixture, bytes: &[u8], age_secs: u64) {
    fixture.write("a.txt", bytes);
    let old = SystemTime::now() - Duration::from_secs(age_secs);
    fs::File::options()
        .write(true)
        .open(fixture.repo().join("a.txt"))
        .unwrap()
        .set_modified(old)
        .unwrap();
}

#[cfg(unix)]
fn attribute_filter(fixture: &Fixture) {
    fixture.write(".gitattributes", b"*.txt filter=evil\n");
    fixture.git_ok(&["add", ".gitattributes"]);
    fixture.commit("attributes");
}

#[cfg(unix)]
fn assert_program_is_live(fixture: &Fixture) {
    edit(fixture, b"aaaa\n", 3600);
    fixture.git_ok(&["status", "--porcelain"]);
    let marker = marker(fixture);
    assert!(
        marker.exists(),
        "plain git must run it, or the setup proves nothing"
    );
    fs::remove_file(&marker).unwrap();
}

fn assert_not_trusted(error: &GitError) {
    assert!(matches!(error, GitError::NotTrusted { .. }), "{error:?}");
    assert_eq!(untrusted_reason(error), Some(Reason::NotTrusted));
    assert_eq!(refusal(error), Some(Refusal::Guard));
}

fn assert_filter_refused(error: &GitError, name: &str) {
    assert!(
        matches!(error, GitError::FilterRefused { filter, .. } if filter == name),
        "{error:?}"
    );
    assert_eq!(untrusted_reason(error), Some(Reason::NotTrusted));
    assert_eq!(refusal(error), Some(Refusal::Filter));
}

#[cfg(unix)]
fn assert_program_refused(error: &GitError) {
    assert!(
        matches!(error, GitError::ProgramConfigured { .. }),
        "{error:?}"
    );
    assert_eq!(refusal(error), Some(Refusal::Program));
}

#[cfg(unix)]
#[test]
fn a_clean_filter_in_the_repository_config_is_not_trusted_and_never_runs() {
    let Some(fixture) = repo() else { return };
    attribute_filter(&fixture);
    let script = program(&fixture);
    fixture.git_ok(&["config", "filter.evil.clean", script.to_str().unwrap()]);
    assert_program_is_live(&fixture);
    edit(&fixture, b"bbbb\n", 7200);
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    let error = gix.status(&guard, &Cancel::default()).unwrap_err();
    assert_filter_refused(&error, "evil");
    assert!(!marker(&fixture).exists(), "a filter driver ran");
}

#[cfg(unix)]
#[test]
fn a_filter_added_after_the_guard_was_read_still_never_runs() {
    let Some(fixture) = repo() else { return };
    attribute_filter(&fixture);
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    assert!(!guard.is_guarded());
    let script = program(&fixture);
    fixture.git_ok(&["config", "filter.evil.clean", script.to_str().unwrap()]);
    assert_program_is_live(&fixture);
    edit(&fixture, b"bbbb\n", 7200);
    let outcome = gix.status(&guard, &Cancel::default());
    assert!(
        !marker(&fixture).exists(),
        "a filter driver ran: {outcome:?}"
    );
}

#[test]
fn a_filter_driver_in_the_repository_config_is_not_trusted() {
    let Some(fixture) = repo() else { return };
    fixture.write(".gitattributes", b"*.txt filter=evil\n");
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    assert!(!guard.is_guarded());
    fixture.git_ok(&["config", "filter.evil.clean", "cat"]);
    let error = gix.status(&guard, &Cancel::default()).unwrap_err();
    assert_filter_refused(&error, "evil");
}

#[cfg(unix)]
#[test]
fn an_fsmonitor_program_is_not_trusted_and_never_runs() {
    let Some(fixture) = repo() else { return };
    let script = program(&fixture);
    fixture.git_ok(&["config", "core.fsmonitor", script.to_str().unwrap()]);
    assert_program_is_live(&fixture);
    edit(&fixture, b"bbbb\n", 7200);
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    let error = gix.status(&guard, &Cancel::default()).unwrap_err();
    assert_program_refused(&error);
    assert!(!marker(&fixture).exists(), "fsmonitor ran");
}

#[test]
fn a_blocked_guard_is_not_trusted() {
    let Some(fixture) = repo() else { return };
    fixture.git_ok(&["config", "filter.a=b.clean", "cat"]);
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let blocked = Guard::read(&fixture.repo().join(".git")).unwrap();
    assert!(blocked.is_blocked());
    let error = gix.status(&blocked, &Cancel::default()).unwrap_err();
    assert_not_trusted(&error);
}

#[test]
fn a_hooks_path_alone_does_not_stop_a_read_only_status() {
    let Some(fixture) = repo() else { return };
    fixture.git_ok(&["config", "core.hooksPath", "nowhere"]);
    assert_eq!(gix_status(fixture.repo()).changed, 0);
}

#[test]
fn status_leaves_the_index_bytes_and_mtime_alone() {
    let Some(fixture) = repo() else { return };
    edit(&fixture, b"same\n", 3600);
    let index = fixture.repo().join(".git/index");
    let read = || {
        (
            fs::read(&index).unwrap(),
            fs::metadata(&index).unwrap().modified().unwrap(),
        )
    };
    let before = read();
    gix_status(fixture.repo());
    assert_eq!(read(), before);
}
