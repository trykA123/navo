use navo_git::status::{Badge, Head, Operation, RepoStatus, badge};

use super::fixture::Fixture;
use super::gix_status_support::{
    cli_status, commit_file, diverge_on_a, gix_status, path, push_from_clone, repo, sibling,
    with_origin,
};

fn agree(fixture: &Fixture) -> RepoStatus {
    let expected = cli_status(fixture.repo());
    let actual = gix_status(fixture.repo());
    assert_eq!(actual, expected, "gix differs from git status");
    actual
}

#[test]
fn a_clean_repository_agrees() {
    let Some(fixture) = repo() else { return };
    let status = agree(&fixture);
    assert_eq!(badge(&status), Badge::Clean);
}

#[test]
fn a_modified_file_agrees() {
    let Some(fixture) = repo() else { return };
    fixture.write("a.txt", b"edited\n");
    assert_eq!(agree(&fixture).changed, 1);
}

#[test]
fn a_deleted_file_agrees() {
    let Some(fixture) = repo() else { return };
    std::fs::remove_file(fixture.repo().join("a.txt")).unwrap();
    assert_eq!(agree(&fixture).changed, 1);
}

#[test]
fn untracked_files_and_folders_agree() {
    let Some(fixture) = repo() else { return };
    fixture.write("new/one.txt", b"1\n");
    fixture.write("new/deep/two.txt", b"2\n");
    fixture.write("three.txt", b"3\n");
    let status = agree(&fixture);
    assert_eq!(status.untracked, 2);
}

#[test]
fn ignored_files_are_not_counted() {
    let Some(fixture) = repo() else { return };
    commit_file(&fixture, ".gitignore", "*.log\nbuild/\n", "ignore");
    fixture.write("x.log", b"x\n");
    fixture.write("build/out.bin", b"o\n");
    assert_eq!(agree(&fixture).untracked, 0);
}

#[test]
fn a_staged_file_agrees() {
    let Some(fixture) = repo() else { return };
    fixture.write("staged.txt", b"s\n");
    fixture.git_ok(&["add", "staged.txt"]);
    assert_eq!(agree(&fixture).changed, 1);
}

#[test]
fn a_file_changed_in_the_index_and_the_worktree_counts_once() {
    let Some(fixture) = repo() else { return };
    fixture.write("a.txt", b"staged\n");
    fixture.git_ok(&["add", "a.txt"]);
    fixture.write("a.txt", b"worktree\n");
    assert_eq!(agree(&fixture).changed, 1);
}

#[test]
fn a_rename_counts_as_two_paths_without_rename_tracking() {
    let Some(fixture) = repo() else { return };
    fixture.git_ok(&["mv", "a.txt", "b.txt"]);
    assert_eq!(agree(&fixture).changed, 2);
}

#[test]
fn an_intent_to_add_file_agrees() {
    let Some(fixture) = repo() else { return };
    fixture.write("later.txt", b"l\n");
    fixture.git_ok(&["add", "-N", "later.txt"]);
    agree(&fixture);
}

#[test]
fn nested_changes_give_the_same_folder_badges() {
    let Some(fixture) = repo() else { return };
    commit_file(&fixture, "src/deep/mod.txt", "m\n", "deep");
    commit_file(&fixture, "docs/readme.txt", "r\n", "docs");
    fixture.write("src/deep/mod.txt", b"edited\n");
    fixture.write("docs/new/note.txt", b"n\n");
    let status = agree(&fixture);
    assert_eq!(status.dir_badge("src/deep"), Badge::Modified);
    assert_eq!(status.dir_badge("src"), Badge::Modified);
    assert_eq!(status.dir_badge("docs/new"), Badge::Modified);
    assert_eq!(status.dir_badge("docs"), Badge::Modified);
}

#[test]
fn two_unpushed_commits_are_ahead_two() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    commit_file(&fixture, "b.txt", "b\n", "b");
    commit_file(&fixture, "c.txt", "c\n", "c");
    let upstream = agree(&fixture).upstream.unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (2, 0));
}

#[test]
fn a_fetched_remote_commit_is_behind_one() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin);
    let status = agree(&fixture);
    assert_eq!(status.upstream.unwrap().behind, 1);
    assert!(status.fetched_ms.is_some());
}

#[test]
fn a_local_and_a_remote_commit_diverge() {
    let Some(fixture) = repo() else { return };
    let origin = with_origin(&fixture);
    push_from_clone(&fixture, &origin);
    commit_file(&fixture, "local.txt", "l\n", "local");
    let status = agree(&fixture);
    assert_eq!(badge(&status), Badge::Diverged);
}

#[test]
fn an_upstream_that_is_gone_agrees() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    fixture.git_ok(&["update-ref", "-d", "refs/remotes/origin/main"]);
    agree(&fixture);
}

#[test]
fn a_conflicted_merge_agrees() {
    let Some(fixture) = repo() else { return };
    diverge_on_a(&fixture);
    assert!(!fixture.git(&["merge", "other"]).status.success());
    let status = agree(&fixture);
    assert_eq!((status.conflicted, status.changed), (1, 0));
    assert_eq!(status.operation, Some(Operation::Merge));
}

#[test]
fn a_conflict_below_a_folder_marks_the_folder() {
    let Some(fixture) = repo() else { return };
    commit_file(&fixture, "sub/f.txt", "base\n", "sub");
    fixture.git_ok(&["checkout", "-b", "other"]);
    commit_file(&fixture, "sub/f.txt", "other\n", "other");
    fixture.git_ok(&["checkout", "main"]);
    commit_file(&fixture, "sub/f.txt", "main\n", "main");
    assert!(!fixture.git(&["merge", "other"]).status.success());
    let status = agree(&fixture);
    assert_eq!(status.dir_badge("sub"), Badge::Conflict);
}

#[test]
fn a_rebase_stopped_on_a_conflict_agrees() {
    let Some(fixture) = repo() else { return };
    diverge_on_a(&fixture);
    assert!(!fixture.git(&["rebase", "other"]).status.success());
    let status = agree(&fixture);
    assert_eq!(status.operation, Some(Operation::Rebase));
}

#[test]
fn a_cherry_pick_stopped_on_a_conflict_agrees() {
    let Some(fixture) = repo() else { return };
    diverge_on_a(&fixture);
    assert!(!fixture.git(&["cherry-pick", "other"]).status.success());
    assert_eq!(agree(&fixture).operation, Some(Operation::CherryPick));
}

#[test]
fn a_detached_head_agrees() {
    let Some(fixture) = repo() else { return };
    fixture.git_ok(&["checkout", "--detach"]);
    assert!(matches!(agree(&fixture).head, Head::Detached(_)));
}

#[test]
fn a_repository_without_commits_agrees() {
    let Some(fixture) = repo() else { return };
    let empty = sibling(&fixture, "empty");
    fixture.git_ok(&["init", "-b", "main", path(&empty)]);
    std::fs::write(empty.join("u.txt"), b"u\n").unwrap();
    let expected = cli_status(&empty);
    let actual = gix_status(&empty);
    assert_eq!(actual, expected);
    assert_eq!(actual.head, Head::Unborn("main".to_owned()));
}

#[test]
fn a_linked_worktree_agrees() {
    let Some(fixture) = repo() else { return };
    let linked = sibling(&fixture, "linked");
    fixture.git_ok(&["worktree", "add", "-b", "feature", path(&linked)]);
    std::fs::write(linked.join("a.txt"), b"linked edit\n").unwrap();
    assert_eq!(gix_status(&linked), cli_status(&linked));
}

#[test]
fn a_symlink_and_an_executable_bit_change_agree() {
    let Some(fixture) = repo() else { return };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = fixture.repo().join("a.txt");
        let mut mode = std::fs::metadata(&file).unwrap().permissions();
        mode.set_mode(0o755);
        std::fs::set_permissions(&file, mode).unwrap();
        std::os::unix::fs::symlink("a.txt", fixture.repo().join("link")).unwrap();
    }
    agree(&fixture);
}
