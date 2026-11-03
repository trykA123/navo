use std::fs;
use std::time::Duration;

use navo_git::{Cancel, GitError, SyncKind};

use super::cli_remote_support::{push_from_clone, repo, sibling, with_origin};
use super::cli_sync_support::{
    DEADLINE, cancel_when, cli, confirm, nothing_left, script_file, slashed, stall_body,
    stalling_hook,
};
use super::fixture::Fixture;

#[test]
fn cancel_in_a_running_post_merge_hook_reports_that_head_moved() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let (hooks, marker) = stalling_hook(&fixture, "post-merge");
    fixture.git_ok(&["config", "core.hooksPath", &slashed(&hooks)]);
    let cli = cli(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let (result, took) = cancel_when(&marker, |cancel| cli.pull_with(&target, cancel, DEADLINE));
    assert!(
        matches!(
            result,
            Err(GitError::Interrupted {
                lock: false,
                moved: true
            })
        ),
        "{result:?}"
    );
    assert!(
        marker.exists() && took < Duration::from_secs(10),
        "{took:?}"
    );
    nothing_left(&fixture);
}

/// `origin` gains a file whose smudge filter touches a marker and stalls while Git holds
/// `index.lock`.
fn stalling_checkout(fixture: &Fixture, origin: &std::path::Path) -> std::path::PathBuf {
    let clone = sibling(fixture, "clone2");
    fixture.git_ok(&["clone", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    fs::write(clone.join(".gitattributes"), b"*.slow filter=slow\n").unwrap();
    fs::write(clone.join("x.slow"), b"slow\n").unwrap();
    let clone = clone.to_str().unwrap();
    fixture.git_ok(&["-C", clone, "add", "."]);
    fixture.git_ok(&["-C", clone, "commit", "-m", "slow"]);
    fixture.git_ok(&["-C", clone, "push", "origin", "main"]);
    let (body, marker) = stall_body(fixture);
    let smudge = script_file(fixture, "smudge", &body);
    let command = format!("sh '{}'", slashed(&smudge));
    fixture.git_ok(&["config", "filter.slow.smudge", &command]);
    marker
}

#[test]
fn a_deadline_in_a_running_checkout_reports_the_left_index_lock() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let marker = stalling_checkout(&fixture, &origin);
    let cli = cli(&fixture);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let result = cli.pull_with(&target, &Cancel::default(), Duration::from_secs(4));
    assert!(
        matches!(
            result,
            Err(GitError::Interrupted {
                lock: true,
                moved: false
            })
        ),
        "{result:?}"
    );
    assert!(
        marker.exists(),
        "git was not in the checkout when the deadline hit"
    );
    nothing_left(&fixture);
}

#[cfg(unix)]
#[test]
fn a_cancel_after_git_finished_reports_the_finished_pull() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    let marker = sibling(&fixture, "pulled");
    let extra = format!(
        "case \" $* \" in *\" merge \"*) git \"$@\"; code=$?; (touch '{}'; sleep 2) >&2 & exit $code ;; esac",
        marker.display()
    );
    let (cli, _) = super::cli_remote_support::wrapped(&fixture, &extra);
    let target = confirm(&cli, SyncKind::Pull).unwrap();
    let (result, _) = cancel_when(&marker, |cancel| cli.pull_with(&target, cancel, DEADLINE));
    assert_eq!(result.unwrap().commits, 1);
    assert!(marker.exists());
    nothing_left(&fixture);
}
