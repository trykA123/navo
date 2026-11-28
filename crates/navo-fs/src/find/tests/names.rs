use std::ops::ControlFlow;
use std::path::MAIN_SEPARATOR_STR;

use navo_core::find::FindQuery;
use tempfile::TempDir;

use super::archives::zip_bytes;
use super::{location, names, search, write};
use crate::CancelToken;
use crate::find::walk_files;

fn found(tree: &TempDir, input: &str) -> Vec<String> {
    let mut found = names(&search(tree, input));
    found.sort();
    found
}

fn joined(parts: &[&str]) -> String {
    parts.join(MAIN_SEPARATOR_STR)
}

#[test]
fn name_mask_filters_files_and_archives_without_pruning_folders() {
    let tree = TempDir::new().unwrap();
    write(&tree, "lib.rs", b"fn lib() {}");
    write(&tree, "lib_test.rs", b"fn test() {}");
    write(&tree, "notes.txt", b"hello");
    write(&tree, "nested/app.rs", b"fn app() {}");
    write(&tree, "nested/app_test.rs", b"fn app_test() {}");
    write(&tree, "dir_test.rs/keep.rs", b"fn keep() {}");
    write(
        &tree,
        "pack.zip",
        zip_bytes(&[
            ("inner/in_zip.rs", b"fn zip() {}"),
            ("inner/in_zip_test.rs", b"fn zip_test() {}"),
            ("inner/readme.txt", b"readme"),
        ]),
    );

    assert_eq!(
        found(&tree, "name:*.rs;!*_test.rs"),
        vec![
            joined(&["dir_test.rs", "keep.rs"]),
            joined(&["lib.rs"]),
            joined(&["nested", "app.rs"]),
        ]
    );
    assert_eq!(
        found(&tree, "name:*.rs;!*_test.rs arc:on"),
        vec![
            joined(&["dir_test.rs", "keep.rs"]),
            joined(&["lib.rs"]),
            joined(&["nested", "app.rs"]),
            "pack.zip\\inner\\in_zip.rs".to_owned(),
        ]
    );
}

#[test]
fn name_filter_combines_with_text_search_and_regex_masks() {
    let tree = TempDir::new().unwrap();
    write(&tree, "match.rs", b"needle");
    write(&tree, "miss.rs", b"other");
    write(&tree, "skip_test.rs", b"needle");
    write(
        &tree,
        "pack.zip",
        zip_bytes(&[
            ("inside.rs", b"needle"),
            ("inside_test.rs", b"needle"),
            ("other.rs", b"other"),
        ]),
    );

    assert_eq!(
        found(&tree, "needle name:*.rs;!*_test.rs arc:on"),
        vec![joined(&["match.rs"]), "pack.zip\\inside.rs".to_owned()]
    );
    assert_eq!(
        found(&tree, "name:/^m.*\\.rs$/"),
        vec![joined(&["match.rs"]), joined(&["miss.rs"])]
    );
}

#[test]
fn walk_files_applies_name_filter() {
    let tree = TempDir::new().unwrap();
    write(&tree, "src/keep.rs", b"needle");
    write(&tree, "src/keep_test.rs", b"needle");
    write(&tree, "src/notes.txt", b"needle");

    let mut batches = Vec::new();
    walk_files(
        &location(&tree),
        &FindQuery::parse("name:*.rs;!*_test.rs"),
        &CancelToken::new(),
        |batch| {
            batches.push(batch);
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    let mut hits = names(&batches);
    hits.sort();
    assert_eq!(hits, vec![joined(&["src", "keep.rs"])]);
}
