use navo_git::status::{Badge, GitStatus, Head, Operation, Upstream, badge};
use navo_git::{Cancel, CliGit};

use super::cli_status_support::{
    commit_file, diverge_on_a, push_from_clone, repo, sibling, status, with_origin,
};

#[test]
fn a_clean_repository_has_no_changes() {
    let Some(fixture) = repo() else { return };
    let status = status(&fixture);
    assert_eq!(status.head, Head::Branch("main".to_owned()));
    assert_eq!(
        (status.changed, status.untracked, status.conflicted),
        (0, 0, 0)
    );
    assert!(status.upstream.is_none() && status.operation.is_none() && !status.capped);
    assert_eq!(status.fetched_ms, None);
    assert_eq!(badge(&status), Badge::Clean);
}

#[test]
fn a_modified_file_counts_once() {
    let Some(fixture) = repo() else { return };
    fixture.write("a.txt", b"edited\n");
    let status = status(&fixture);
    assert_eq!((status.changed, status.untracked), (1, 0));
    assert_eq!(badge(&status), Badge::Modified);
}

#[test]
fn untracked_files_are_counted_apart() {
    let Some(fixture) = repo() else { return };
    fixture.write("new/one.txt", b"1\n");
    fixture.write("two.txt", b"2\n");
    let status = status(&fixture);
    assert_eq!((status.changed, status.untracked), (0, 2));
}

#[test]
fn a_staged_file_is_a_change() {
    let Some(fixture) = repo() else { return };
    fixture.write("staged.txt", b"s\n");
    fixture.git_ok(&["add", "staged.txt"]);
    let status = status(&fixture);
    assert_eq!((status.changed, status.untracked), (1, 0));
}

#[test]
fn two_unpushed_commits_are_ahead_two() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    commit_file(&fixture, "b.txt", "b\n", "b");
    commit_file(&fixture, "c.txt", "c\n", "c");
    let status = status(&fixture);
    let upstream = Upstream {
        name: "origin/main".to_owned(),
        ahead: 2,
        behind: 0,
    };
    assert_eq!(status.upstream, Some(upstream));
    assert_eq!(badge(&status), Badge::Ahead);
}

#[test]
fn a_fetched_remote_commit_is_behind_one() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin);
    let status = status(&fixture);
    let upstream = status.upstream.as_ref().unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (0, 1));
    assert_eq!(badge(&status), Badge::Behind);
    assert!(status.fetched_ms.is_some_and(|ms| ms > 0));
}

#[test]
fn a_local_and_a_remote_commit_diverge() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin);
    commit_file(&fixture, "local.txt", "l\n", "local");
    let status = status(&fixture);
    let upstream = status.upstream.as_ref().unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (1, 1));
    assert_eq!(badge(&status), Badge::Diverged);
}

#[test]
fn a_conflicted_merge_is_a_conflict_in_a_merge() {
    let Some(fixture) = repo() else { return };
    diverge_on_a(&fixture);
    assert!(!fixture.git(&["merge", "other"]).status.success());
    let status = status(&fixture);
    assert_eq!(status.conflicted, 1);
    assert_eq!(status.operation, Some(Operation::Merge));
    assert_eq!(badge(&status), Badge::Conflict);
}

#[test]
fn a_rebase_stopped_on_a_conflict_is_a_rebase() {
    let Some(fixture) = repo() else { return };
    diverge_on_a(&fixture);
    assert!(!fixture.git(&["rebase", "other"]).status.success());
    let status = status(&fixture);
    assert_eq!(status.conflicted, 1);
    assert_eq!(status.operation, Some(Operation::Rebase));
}

#[test]
fn a_cherry_pick_stopped_on_a_conflict_is_a_cherry_pick() {
    let Some(fixture) = repo() else { return };
    diverge_on_a(&fixture);
    assert!(!fixture.git(&["cherry-pick", "other"]).status.success());
    assert_eq!(status(&fixture).operation, Some(Operation::CherryPick));
}

#[test]
fn a_detached_head_is_reported() {
    let Some(fixture) = repo() else { return };
    fixture.git_ok(&["checkout", "--detach"]);
    assert!(matches!(status(&fixture).head, Head::Detached(_)));
}

#[test]
fn a_repository_without_commits_is_unborn() {
    let Some(fixture) = repo() else { return };
    let empty = sibling(&fixture, "empty");
    fixture.git_ok(&["init", "-b", "main", empty.to_str().unwrap()]);
    let cli = CliGit::new(&empty).unwrap();
    let guard = cli.guard().unwrap();
    let status = cli.status(&guard, &Cancel::default()).unwrap();
    assert_eq!(status.head, Head::Unborn("main".to_owned()));
}

#[cfg(unix)]
#[test]
fn a_status_probe_asks_git_for_the_git_dir_once_and_still_reads_the_config() {
    let Some(fixture) = repo() else { return };
    let (cli, log) = super::cli_status_support::wrapped(&fixture, "");
    let guard = cli.guard().unwrap();
    cli.status(&guard, &Cancel::default()).unwrap();
    let calls = std::fs::read_to_string(log).unwrap();
    let count = |word: &str| calls.lines().filter(|line| line.contains(word)).count();
    assert_eq!(count("rev-parse"), 1, "{calls}");
    assert_eq!(count("config --list"), 1, "{calls}");
    assert_eq!(count(" status "), 1, "{calls}");
}

#[cfg(unix)]
fn open_with(git: &std::path::Path, repo: &std::path::Path) {
    let options = navo_git::CliOptions {
        git: Some(git.to_path_buf()),
        ..navo_git::CliOptions::default()
    };
    CliGit::with_options(repo, &options).unwrap();
}

#[cfg(unix)]
#[test]
fn probing_many_repositories_asks_git_for_its_version_once() {
    let Some(fixture) = repo() else { return };
    let log = sibling(&fixture, "once.log");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{}'\nexec git \"$@\"\n",
        log.display()
    );
    let git = fixture.script("git-once", &script);
    for name in ["r1", "r2", "r3"] {
        let other = sibling(&fixture, name);
        fixture.git_ok(&["init", "-b", "main", other.to_str().unwrap()]);
        open_with(&git, &other);
    }
    open_with(&git, fixture.repo());
    let calls = std::fs::read_to_string(log).unwrap();
    let count = |word: &str| calls.lines().filter(|line| line.contains(word)).count();
    assert_eq!(count("--version"), 1, "{calls}");
    assert_eq!(count("--exec-path"), 0, "{calls}");
    assert_eq!(count("rev-parse"), 4, "{calls}");
}

#[cfg(unix)]
#[test]
fn a_changed_git_executable_is_probed_again() {
    let Some(fixture) = repo() else { return };
    let log = sibling(&fixture, "changed.log");
    let script = |note: &str| {
        format!(
            "#!/bin/sh\n# {note}\necho \"$*\" >> '{}'\nexec git \"$@\"\n",
            log.display()
        )
    };
    let git = fixture.script("git-changed", &script("one"));
    open_with(&git, fixture.repo());
    open_with(&git, fixture.repo());
    fixture.script("git-changed", &script("two"));
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&git)
        .unwrap()
        .set_modified(later)
        .unwrap();
    open_with(&git, fixture.repo());
    let calls = std::fs::read_to_string(log).unwrap();
    let versions = calls
        .lines()
        .filter(|line| line.contains("--version"))
        .count();
    assert_eq!(versions, 2, "{calls}");
}
