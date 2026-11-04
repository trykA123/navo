use std::path::MAIN_SEPARATOR_STR;

use tempfile::TempDir;

use super::{names, search, write};

fn found(tree: &TempDir, input: &str) -> Vec<String> {
    let mut found = names(&search(tree, input));
    found.sort();
    found
}

fn joined(parts: &[&str]) -> String {
    parts.join(MAIN_SEPARATOR_STR)
}

fn ignore_tree() -> TempDir {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "*.log\nbuild/\n");
    write(&tree, "keep.txt", "needle");
    write(&tree, "app.log", "needle");
    write(&tree, "build/out.txt", "needle");
    write(&tree, "other/build", "needle");
    tree
}

#[test]
fn gitignore_outside_a_repository_hides_files_and_skips_folders() {
    let tree = ignore_tree();
    let batches = search(&tree, "needle");
    let mut hits = names(&batches);
    hits.sort();
    assert_eq!(
        hits,
        vec![joined(&["keep.txt"]), joined(&["other", "build"])]
    );
    let progress = &batches.last().unwrap().progress;
    assert_eq!((progress.scanned, progress.matched), (3, 2));
}

#[test]
fn git_off_searches_ignored_files_and_folders() {
    let tree = ignore_tree();
    assert_eq!(
        found(&tree, "needle git:off"),
        vec![
            joined(&["app.log"]),
            joined(&["build", "out.txt"]),
            joined(&["keep.txt"]),
            joined(&["other", "build"]),
        ]
    );
}

#[test]
fn nearer_gitignore_and_negation_take_precedence() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "*.txt\n!top.txt\n");
    write(&tree, "sub/.gitignore", "!keep.txt\n");
    write(&tree, "a.txt", "needle");
    write(&tree, "top.txt", "needle");
    write(&tree, "sub/keep.txt", "needle");
    write(&tree, "sub/drop.txt", "needle");
    write(&tree, "note.md", "needle");
    assert_eq!(
        found(&tree, "needle"),
        vec![
            joined(&["note.md"]),
            joined(&["sub", "keep.txt"]),
            joined(&["top.txt"]),
        ]
    );
}

#[test]
fn anchored_patterns_match_from_their_own_folder() {
    let tree = TempDir::new().unwrap();
    write(&tree, "sub/.gitignore", "/local.txt\n");
    write(&tree, "local.txt", "needle");
    write(&tree, "sub/local.txt", "needle");
    write(&tree, "sub/deeper/local.txt", "needle");
    assert_eq!(
        found(&tree, "needle"),
        vec![
            joined(&["local.txt"]),
            joined(&["sub", "deeper", "local.txt"]),
        ]
    );
}

#[test]
fn gitignore_above_the_search_root_applies_inside_a_repository() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".git/HEAD", "ref");
    write(&tree, ".gitignore", "*.log\nsrc/gen/\n");
    write(&tree, "src/a.txt", "needle");
    write(&tree, "src/b.log", "needle");
    write(&tree, "src/gen/c.txt", "needle");
    assert_eq!(found(&tree, "needle in:src"), vec![joined(&["a.txt"])]);
}

#[test]
fn gitignore_above_the_search_root_outside_a_repository_is_skipped() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "*.log\n");
    write(&tree, "src/b.log", "needle");
    assert_eq!(found(&tree, "needle in:src"), vec![joined(&["b.log"])]);
}

#[test]
fn gitignore_above_a_repository_top_folder_is_skipped() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "*.log\n");
    write(&tree, "repo/.git/HEAD", "ref");
    write(&tree, "repo/b.log", "needle");
    assert_eq!(found(&tree, "needle in:repo"), vec![joined(&["b.log"])]);
}

#[test]
fn search_root_inside_an_ignored_folder_is_searched() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "build/\n");
    write(&tree, "build/out.txt", "needle");
    assert_eq!(found(&tree, "needle in:build"), vec![joined(&["out.txt"])]);
}

#[test]
fn nested_repository_keeps_the_outer_rules() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "*.log\n");
    write(&tree, "inner/.git/HEAD", "ref");
    write(&tree, "inner/app.log", "needle");
    write(&tree, "inner/keep.txt", "needle");
    assert_eq!(found(&tree, "needle"), vec![joined(&["inner", "keep.txt"])]);
}

#[test]
fn folder_listed_in_several_batches_reads_its_gitignore_first() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "*.log\n");
    for index in 0..1100 {
        write(&tree, &format!("{index:04}.txt"), "x");
    }
    write(&tree, "app.log", "needle");
    write(&tree, "keep.txt", "needle");
    assert_eq!(found(&tree, "needle"), vec![joined(&["keep.txt"])]);
}

#[cfg(unix)]
#[test]
fn symlinked_gitignore_in_large_folder_is_not_applied() {
    use std::os::unix::fs::symlink;

    let tree = TempDir::new().unwrap();
    write(&tree, "rules", "*.log\n");
    symlink(tree.path().join("rules"), tree.path().join(".gitignore")).unwrap();
    for index in 0..1100 {
        write(&tree, &format!("{index:04}.txt"), "x");
    }
    write(&tree, "app.log", "needle");
    assert_eq!(found(&tree, "needle"), vec![joined(&["app.log"])]);
}

#[cfg(unix)]
#[test]
fn symlinked_gitignore_above_search_root_is_not_applied() {
    use std::os::unix::fs::symlink;

    let tree = TempDir::new().unwrap();
    write(&tree, "rules", "*.log\n");
    symlink(tree.path().join("rules"), tree.path().join(".gitignore")).unwrap();
    write(&tree, ".git/HEAD", "ref");
    write(&tree, "src/app.log", "needle");
    assert_eq!(found(&tree, "needle in:src"), vec![joined(&["app.log"])]);
}

#[cfg(unix)]
#[test]
fn fifo_gitignore_above_search_root_does_not_block() {
    use std::sync::mpsc;
    use std::time::Duration;

    let tree = TempDir::new().unwrap();
    let fifo = tree.path().join(".gitignore");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    write(&tree, ".git/HEAD", "ref");
    write(&tree, "src/app.txt", "needle");
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        sender.send(found(&tree, "needle in:src")).unwrap();
    });
    let result = receiver.recv_timeout(Duration::from_secs(5));
    if result.is_err() {
        drop(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&fifo)
                .unwrap(),
        );
        worker.join().unwrap();
        panic!("search blocked on a FIFO gitignore");
    }
    worker.join().unwrap();
    assert_eq!(result.unwrap(), vec![joined(&["app.txt"])]);
}

#[test]
fn only_the_first_mebibyte_of_a_gitignore_is_read() {
    let tree = TempDir::new().unwrap();
    let mut rules = b"a.txt\n".to_vec();
    rules.resize(1024 * 1024, b'#');
    rules.extend_from_slice(b"\nb.txt\n");
    write(&tree, ".gitignore", rules);
    write(&tree, "a.txt", "needle");
    write(&tree, "b.txt", "needle");
    assert_eq!(found(&tree, "needle"), vec![joined(&["b.txt"])]);
}

#[cfg(windows)]
#[test]
fn gitignore_matches_case_insensitively_on_windows() {
    let tree = TempDir::new().unwrap();
    write(&tree, ".gitignore", "*.LOG\n");
    write(&tree, "app.log", "needle");
    write(&tree, "keep.txt", "needle");
    assert_eq!(found(&tree, "needle"), vec![joined(&["keep.txt"])]);
}
