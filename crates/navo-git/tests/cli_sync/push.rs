use std::fs;
use std::path::Path;

use navo_git::{GitError, SyncKind, SyncSummary};

use super::cli_remote_support::{push_from_clone, repo, with_origin};
use super::cli_sync_support::{cli, commit_file, push, refs_of};

fn origin_ref(fixture: &super::fixture::Fixture, origin: &std::path::Path, name: &str) -> Vec<u8> {
    let git_dir = origin.to_str().unwrap();
    fixture
        .git_ok(&["-C", git_dir, "rev-parse", "--verify", name])
        .stdout
}

#[test]
fn push_moves_the_origin_branch_by_exactly_the_local_commits_and_nothing_else() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let old = origin_ref(&fixture, &origin, "refs/heads/main");
    fixture.git_ok(&["checkout", "-b", "other"]);
    commit_file(&fixture, "other.txt", b"other\n");
    fixture.git_ok(&["checkout", "main"]);
    commit_file(&fixture, "one.txt", b"1\n");
    commit_file(&fixture, "two.txt", b"2\n");
    fixture.git_ok(&["tag", "-a", "v1", "-m", "v1"]);
    fixture.git_ok(&["config", "push.followTags", "true"]);
    fixture.git_ok(&["config", "push.default", "matching"]);
    fixture.git_ok(&["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"]);
    let refs_before = refs_of(&fixture, &origin);
    let cli = cli(&fixture);
    let summary = push(&cli).unwrap();
    assert_eq!(
        summary,
        SyncSummary {
            kind: SyncKind::Push,
            branch: "main".into(),
            remote: "origin".into(),
            commits: 2,
            tracking_stale: false,
        }
    );
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    assert_eq!(origin_ref(&fixture, &origin, "refs/heads/main"), head);
    let old = String::from_utf8(old).unwrap();
    let new = String::from_utf8(head).unwrap();
    let range = format!("{}..{}", old.trim(), new.trim());
    let git_dir = origin.to_str().unwrap();
    let counted = fixture.git_ok(&["-C", git_dir, "rev-list", "--count", &range]);
    assert_eq!(counted.stdout, b"2\n");
    let refs_after = refs_of(&fixture, &origin);
    assert_eq!(refs_after.lines().count(), refs_before.lines().count());
    assert!(!refs_after.contains("refs/heads/other"), "{refs_after}");
    assert!(!refs_after.contains("refs/tags/"), "{refs_after}");
}

#[test]
fn an_up_to_date_push_reports_no_commits() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let before = refs_of(&fixture, &origin);
    let cli = cli(&fixture);
    assert_eq!(push(&cli).unwrap().commits, 0);
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[test]
fn a_push_rejected_because_origin_moved_is_never_forced() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin, "clone2");
    commit_file(&fixture, "local.txt", b"local\n");
    fixture.git_ok(&["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"]);
    fixture.git_ok(&["config", "push.default", "matching"]);
    let before = refs_of(&fixture, &origin);
    let cli = cli(&fixture);
    let error = push(&cli).unwrap_err();
    assert!(matches!(error, GitError::Rejected), "{error:?}");
    assert_eq!(refs_of(&fixture, &origin), before);
}

#[test]
fn push_reaches_an_upstream_branch_with_odd_characters() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    let remote = "fix/ä'b#1,c&d$e(f)";
    fixture.git_ok(&["checkout", "-b", "odd-ü@x"]);
    fixture.git_ok(&["push", "origin", &format!("HEAD:refs/heads/{remote}")]);
    fixture.git_ok(&["fetch", "-q", "origin"]);
    fixture.git_ok(&["config", "branch.odd-ü@x.remote", "origin"]);
    let merge = format!("refs/heads/{remote}");
    fixture.git_ok(&["config", "branch.odd-ü@x.merge", &merge]);
    commit_file(&fixture, "odd.txt", b"odd\n");
    let main_before = origin_ref(&fixture, &origin, "refs/heads/main");
    let cli = cli(&fixture);
    let summary = push(&cli).unwrap();
    assert_eq!((summary.branch.as_str(), summary.commits), ("odd-ü@x", 1));
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    assert_eq!(origin_ref(&fixture, &origin, &merge), head);
    assert_eq!(
        origin_ref(&fixture, &origin, "refs/heads/main"),
        main_before
    );
}

#[test]
fn a_new_upstream_branch_counts_only_commits_the_remote_lacks() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    fixture.git_ok(&["checkout", "-b", "topic"]);
    fixture.git_ok(&["config", "branch.topic.remote", "origin"]);
    fixture.git_ok(&["config", "branch.topic.merge", "refs/heads/topic"]);
    commit_file(&fixture, "topic.txt", b"topic\n");
    let cli = cli(&fixture);
    assert_eq!(push(&cli).unwrap().commits, 1);
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    assert_eq!(origin_ref(&fixture, &origin, "refs/heads/topic"), head);
}

const NEVER: [&str; 12] = [
    "\"--force",
    "\"-f\"",
    "\"--delete\"",
    "\"-d\"",
    "\"--tags\"",
    "\"--follow-tags\"",
    "\"--set-upstream\"",
    "\"-u\"",
    "\"--mirror\"",
    "\"--all\"",
    "\"--prune\"",
    "\"+",
];

#[test]
fn the_pull_and_push_sources_hold_no_forcing_token() {
    let cli = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli");
    for file in ["sync.rs", "sync_read.rs"] {
        let text = fs::read_to_string(cli.join(file)).unwrap();
        for token in NEVER {
            assert!(!text.contains(token), "{file} holds {token}");
        }
        assert!(
            !text.contains("format!(\"+"),
            "{file} builds a forced refspec"
        );
    }
}
