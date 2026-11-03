use std::fs::{self, File};
use std::time::{Duration, SystemTime};

use navo_git::status::Operation;
use navo_git::{Cancel, CliGit, GitError, StatusLimits};

use super::cli_status_support::{commit_file, repo};

fn tracked_files(fixture: &super::fixture::Fixture) {
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
    let cli = CliGit::new(fixture.repo()).unwrap();
    let guard = cli.guard().unwrap();
    let limits = StatusLimits {
        stdout_cap: 1024,
        ..StatusLimits::default()
    };
    let status = cli.status_with(&guard, &Cancel::default(), limits).unwrap();
    assert!(status.capped);
    assert!(status.changed < 30 && status.untracked <= 500);
    assert_eq!(status.conflicted, 1, "ls-files -u finds it past the cap");
    assert_eq!(status.operation, Some(Operation::Merge));
}

#[cfg(unix)]
#[test]
fn a_one_millisecond_timeout_is_a_timeout() {
    let Some(fixture) = repo() else { return };
    let (cli, _) = super::cli_status_support::wrapped(
        &fixture,
        "case \" $* \" in *\" status \"*) exec sleep 30 ;; esac",
    );
    let guard = cli.guard().unwrap();
    let limits = StatusLimits {
        timeout: Duration::from_millis(1),
        ..StatusLimits::default()
    };
    let started = std::time::Instant::now();
    let error = cli
        .status_with(&guard, &Cancel::default(), limits)
        .unwrap_err();
    assert!(matches!(error, GitError::Timeout), "{error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the child was not killed"
    );
}

#[test]
fn a_cancelled_status_starts_nothing_and_says_cancelled() {
    let Some(fixture) = repo() else { return };
    let cli = CliGit::new(fixture.repo()).unwrap();
    let guard = cli.guard().unwrap();
    let cancel = Cancel::default();
    cancel.cancel();
    let error = navo_git::status::GitStatus::status(&cli, &guard, &cancel).unwrap_err();
    assert!(matches!(error, GitError::Cancelled), "{error:?}");
}

#[test]
fn status_leaves_the_index_bytes_and_mtime_alone() {
    let Some(fixture) = repo() else { return };
    let old = SystemTime::now() - Duration::from_secs(3600);
    let touch = || {
        File::options()
            .write(true)
            .open(fixture.repo().join("a.txt"))
            .unwrap()
            .set_modified(old)
            .unwrap();
    };
    touch();
    let index = fixture.repo().join(".git/index");
    let before = (
        fs::read(&index).unwrap(),
        fs::metadata(&index).unwrap().modified().unwrap(),
    );
    super::cli_status_support::status(&fixture);
    let after = (
        fs::read(&index).unwrap(),
        fs::metadata(&index).unwrap().modified().unwrap(),
    );
    assert_eq!(before, after);
    fixture.git_ok(&["status", "--porcelain"]);
    assert_ne!(
        fs::read(&index).unwrap(),
        before.0,
        "plain git status refreshes the index, so the stat-dirty setup is real"
    );
}
