use std::path::MAIN_SEPARATOR_STR;

use tempfile::TempDir;

use super::archives::zip_bytes;
use super::{names, search, write};

fn found(tree: &TempDir, input: &str) -> Vec<String> {
    let mut found = names(&search(tree, input));
    found.sort();
    found
}

fn joined(parts: &[&str]) -> String {
    parts.join(MAIN_SEPARATOR_STR)
}

#[test]
fn depth_limits_how_many_folder_levels_are_entered() {
    let tree = TempDir::new().unwrap();
    write(&tree, "root.txt", b"needle");
    write(&tree, "a/one.txt", b"needle");
    write(&tree, "a/b/two.txt", b"needle");
    write(&tree, "a/b/c/three.txt", b"needle");

    let zero = search(&tree, "needle depth:0");
    assert_eq!(names(&zero), vec![joined(&["root.txt"])]);
    assert_eq!(zero.last().unwrap().progress.scanned, 1);

    let one = search(&tree, "needle depth:1");
    let mut one_names = names(&one);
    one_names.sort();
    assert_eq!(
        one_names,
        vec![joined(&["a", "one.txt"]), joined(&["root.txt"])]
    );
    assert_eq!(one.last().unwrap().progress.scanned, 2);

    assert_eq!(
        found(&tree, "needle depth:2"),
        vec![
            joined(&["a", "b", "two.txt"]),
            joined(&["a", "one.txt"]),
            joined(&["root.txt"]),
        ]
    );
    assert_eq!(
        found(&tree, "needle"),
        vec![
            joined(&["a", "b", "c", "three.txt"]),
            joined(&["a", "b", "two.txt"]),
            joined(&["a", "one.txt"]),
            joined(&["root.txt"]),
        ]
    );
}

#[test]
fn depth_does_not_limit_nested_entries_inside_archives() {
    let tree = TempDir::new().unwrap();
    write(&tree, "root.txt", b"needle");
    write(
        &tree,
        "pack.zip",
        zip_bytes(&[("deep/nested/inside.txt", b"needle")]),
    );
    write(
        &tree,
        "a/sub.zip",
        zip_bytes(&[("inner/sub.txt", b"needle")]),
    );

    assert_eq!(
        found(&tree, "needle depth:0 arc:on"),
        vec![
            "pack.zip\\deep\\nested\\inside.txt".to_owned(),
            joined(&["root.txt"]),
        ]
    );
    assert_eq!(
        found(&tree, "needle depth:1 arc:on"),
        vec![
            "a\\sub.zip\\inner\\sub.txt".to_owned(),
            "pack.zip\\deep\\nested\\inside.txt".to_owned(),
            joined(&["root.txt"]),
        ]
    );
}

#[test]
fn depth_applies_relative_to_in_scope_and_filter_only_queries() {
    let tree = TempDir::new().unwrap();
    write(&tree, "a/one.txt", b"one");
    write(&tree, "a/b/two.txt", b"two");
    write(&tree, "a/b/c/three.txt", b"three");

    assert_eq!(found(&tree, "depth:0 in:a"), vec![joined(&["one.txt"])]);
    assert_eq!(
        found(&tree, "depth:1 in:a"),
        vec![joined(&["b", "two.txt"]), joined(&["one.txt"])]
    );
}
