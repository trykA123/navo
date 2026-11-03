use std::time::Duration;

use navo_git::status::{GitStatus, Operation};
use navo_git::{Cancel, GitError, GixGit, StatusLimits};

use super::fixture::Fixture;
use super::gix_status_support::{commit_file, repo};

fn tracked_files(fixture: &Fixture) {
    for index in 0..30 {
        let name = format!("m{index:02}.txt");
        fixture.write(&name, b"base\n");
        fixture.git_ok(&["add", &name]);
    }
    fixture.write("zz.txt", b"base\n");
    fixture.git_ok(&["add", "zz.txt"]);
    fixture.commit("many files");
}

#[test]
fn a_one_kib_cap_is_capped_and_still_finds_a_conflict() {
    let Some(fixture) = repo() else { return };
    tracked_files(&fixture);
    fixture.git_ok(&["checkout", "-b", "other"]);
    commit_file(&fixture, "zz.txt", "other\n", "other");
    fixture.git_ok(&["checkout", "main"]);
    commit_file(&fixture, "zz.txt", "main\n", "main");
    assert!(!fixture.git(&["merge", "other"]).status.success());
    for index in 0..30 {
        fixture.write(&format!("m{index:02}.txt"), b"edited\n");
    }
    for index in 0..500 {
        fixture.write(&format!("u{index:03}.txt"), b"u\n");
    }
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    let limits = StatusLimits {
        stdout_cap: 1024,
        ..StatusLimits::default()
    };
    let status = gix.status_with(&guard, &Cancel::default(), limits).unwrap();
    assert!(status.capped);
    assert!(status.changed < 30 && status.untracked < 500);
    assert_eq!(
        status.conflicted, 1,
        "the index stages show it past the cap"
    );
    assert_eq!(status.operation, Some(Operation::Merge));
}

#[test]
fn a_zero_timeout_is_a_timeout() {
    let Some(fixture) = repo() else { return };
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    let limits = StatusLimits {
        timeout: Duration::ZERO,
        ..StatusLimits::default()
    };
    let error = gix
        .status_with(&guard, &Cancel::default(), limits)
        .unwrap_err();
    assert!(matches!(error, GitError::Timeout), "{error:?}");
}

#[test]
fn a_cancelled_status_says_cancelled() {
    let Some(fixture) = repo() else { return };
    let gix = GixGit::new_isolated(fixture.repo()).unwrap();
    let guard = gix.guard().unwrap();
    let cancel = Cancel::default();
    cancel.cancel();
    let error = gix.status(&guard, &cancel).unwrap_err();
    assert!(matches!(error, GitError::Cancelled), "{error:?}");
}
