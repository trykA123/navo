mod archives;

use std::fs::{self, File, FileTimes};
use std::ops::ControlFlow;
use std::time::{Duration, UNIX_EPOCH};

use rusty_core::find::FindQuery;
use rusty_core::{Location, ProviderPath};
use tempfile::TempDir;

use super::{FindError, FoundBatch, find_files};
use crate::CancelToken;

fn location(tree: &TempDir) -> Location {
    Location::new(tree.path().to_str().unwrap()).unwrap()
}

fn write(tree: &TempDir, name: &str, bytes: impl AsRef<[u8]>) {
    let path = tree.path().join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn search(tree: &TempDir, input: &str) -> Vec<FoundBatch> {
    let mut batches = Vec::new();
    find_files(
        &location(tree),
        &FindQuery::parse(input),
        &CancelToken::new(),
        |batch| {
            batches.push(batch);
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    batches
}

fn names(batches: &[FoundBatch]) -> Vec<String> {
    batches
        .iter()
        .flat_map(|batch| &batch.files)
        .map(|file| file.relative.to_text())
        .collect()
}

#[test]
fn nested_hits_keep_relative_paths_and_progress() {
    let tree = TempDir::new().unwrap();
    write(&tree, "src/a.txt", b"needle\nneedle\n");
    write(&tree, "plain.txt", b"nothing");
    write(&tree, "binary", b"needle\0");
    let batches = search(&tree, "needle");
    assert_eq!(
        names(&batches),
        [format!("src{}a.txt", std::path::MAIN_SEPARATOR)]
    );
    let found = &batches
        .iter()
        .flat_map(|batch| &batch.files)
        .next()
        .unwrap();
    assert_eq!(found.entry.name, ProviderPath::from_text("a.txt"));
    assert_eq!(found.hits.total, 2);
    assert_eq!(found.hits.hits[1].line, 2);
    let progress = &batches.last().unwrap().progress;
    assert_eq!(
        (
            progress.scanned,
            progress.hits,
            progress.matched,
            progress.skipped
        ),
        (3, 2, 1, 1)
    );
    assert!(progress.current.is_some());
}

#[test]
fn filters_apply_before_content_reads() {
    let tree = TempDir::new().unwrap();
    write(&tree, "keep.RS", b"needle");
    write(&tree, "wrong.txt", b"needle");
    write(&tree, "large.rs", b"needle and more");
    write(&tree, "old.rs", b"needle");
    File::options()
        .write(true)
        .open(tree.path().join("old.rs"))
        .unwrap()
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000)))
        .unwrap();
    let batches = search(
        &tree,
        "needle ext:rs size:<10 mod:>2026-01-01 attr:-readonly",
    );
    assert_eq!(names(&batches), ["keep.RS"]);
    assert_eq!(batches.last().unwrap().progress.scanned, 4);
}

#[test]
fn filter_only_queries_list_files_without_scanning_binary_content() {
    let tree = TempDir::new().unwrap();
    write(&tree, "raw.dat", b"\0");
    let batches = search(&tree, "ext:dat");
    assert_eq!(names(&batches), ["raw.dat"]);
    assert_eq!(batches.last().unwrap().progress.hits, 0);
}

#[test]
fn hidden_folders_require_an_explicit_filter_and_git_is_always_skipped() {
    let tree = TempDir::new().unwrap();
    write(&tree, "visible.txt", b"needle");
    write(&tree, ".hidden/.hit.txt", b"needle");
    write(&tree, ".git/.hit.txt", b"needle");
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use std::process::Command;
        for name in [".hidden", ".hidden/.hit.txt", ".git", ".git/.hit.txt"] {
            assert!(
                Command::new("attrib")
                    .arg("+H")
                    .arg(tree.path().join(name))
                    .status()
                    .unwrap()
                    .success()
            );
            assert_ne!(
                fs::metadata(tree.path().join(name))
                    .unwrap()
                    .file_attributes()
                    & 2,
                0
            );
        }
    }
    assert_eq!(names(&search(&tree, "needle")), ["visible.txt"]);
    assert_eq!(
        names(&search(&tree, "needle attr:hidden")),
        [format!(".hidden{}.hit.txt", std::path::MAIN_SEPARATOR)]
    );
}

#[test]
fn within_overrides_scope_and_rebases_names() {
    let tree = TempDir::new().unwrap();
    write(&tree, "src/a.txt", b"needle");
    write(&tree, "other.txt", b"needle");
    assert_eq!(names(&search(&tree, "needle in:src")), ["a.txt"]);
    let input = format!("needle in:\"{}\"", tree.path().join("src").display());
    assert_eq!(names(&search(&tree, &input)), ["a.txt"]);
}

#[test]
fn size_filter_can_allow_files_above_the_default_limit() {
    let tree = TempDir::new().unwrap();
    write(&tree, "large.bin", b"needle\n");
    File::options()
        .write(true)
        .open(tree.path().join("large.bin"))
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(names(&search(&tree, "ext:bin")).is_empty());
    assert_eq!(names(&search(&tree, "ext:bin size:>64M")), ["large.bin"]);
}

#[test]
fn cancellation_keeps_delivered_hits_and_stops_at_a_batch_boundary() {
    let tree = TempDir::new().unwrap();
    for index in 0..450 {
        write(&tree, &format!("{index:03}.txt"), b"needle");
    }
    let cancel = CancelToken::new();
    let mut batches = Vec::new();
    let result = find_files(
        &location(&tree),
        &FindQuery::parse("needle"),
        &cancel,
        |batch| {
            batches.push(batch);
            ControlFlow::Break(())
        },
    );
    assert!(matches!(result, Err(FindError::Cancelled)));
    assert!(cancel.is_cancelled());
    assert!(!names(&batches).is_empty());
    assert!(batches[0].progress.scanned <= 200);
}

#[test]
fn cancellation_from_the_final_batch_is_reported() {
    let tree = TempDir::new().unwrap();
    write(&tree, "hit.txt", b"needle");
    let cancel = CancelToken::new();
    let mut found = Vec::new();
    let result = find_files(
        &location(&tree),
        &FindQuery::parse("needle"),
        &cancel,
        |batch| {
            found.extend(batch.files);
            cancel.cancel();
            ControlFlow::Continue(())
        },
    );
    assert!(matches!(result, Err(FindError::Cancelled)));
    assert_eq!(found.len(), 1);
}

#[test]
fn pre_cancelled_search_does_not_list_and_other_providers_refuse() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = find_files(
        &Location::new("/missing").unwrap(),
        &FindQuery::default(),
        &cancel,
        |_| panic!("cancelled"),
    );
    assert!(matches!(result, Err(FindError::Cancelled)));
    let result = find_files(
        &Location::new("fake:///folder").unwrap(),
        &FindQuery::default(),
        &CancelToken::new(),
        |_| panic!("unsupported"),
    );
    assert!(matches!(result, Err(FindError::Unsupported)));
}

#[test]
fn batches_are_bounded_sorted_and_report_final_counts() {
    let tree = TempDir::new().unwrap();
    for index in (0..450).rev() {
        write(&tree, &format!("{index:03}.txt"), b"needle");
    }
    let batches = search(&tree, "needle");
    assert!(batches.len() >= 3);
    for batch in &batches {
        assert!(batch.files.len() <= 200);
        let names = names(std::slice::from_ref(batch));
        assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
    }
    assert_eq!(names(&batches).len(), 450);
    let progress = &batches.last().unwrap().progress;
    assert_eq!(
        (progress.scanned, progress.matched, progress.hits),
        (450, 450, 450)
    );
}

#[test]
fn modified_filters_and_size_ranges_select_files() {
    let tree = TempDir::new().unwrap();
    write(&tree, "old.txt", b"needle");
    write(&tree, "new.txt", b"needle");
    File::options()
        .write(true)
        .open(tree.path().join("old.txt"))
        .unwrap()
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(86400)))
        .unwrap();
    for query in ["mod:<1970-01-03", "mod:>7d"] {
        assert_eq!(names(&search(&tree, query)), ["old.txt"]);
    }
    for query in ["mod:today", "mod:<7d", "mod:>2026-01-01 size:6..6"] {
        assert_eq!(names(&search(&tree, query)), ["new.txt"]);
    }
    assert!(names(&search(&tree, "size:7..10")).is_empty());
}

#[test]
fn readonly_attribute_filters_include_and_exclude_files() {
    let tree = TempDir::new().unwrap();
    write(&tree, "readonly.txt", b"needle");
    write(&tree, "writable.txt", b"needle");
    let path = tree.path().join("readonly.txt");
    let original = fs::metadata(&path).unwrap().permissions();
    let mut permissions = original.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let included = search(&tree, "needle attr:readonly");
    let excluded = search(&tree, "needle attr:-readonly");
    fs::set_permissions(path, original).unwrap();
    assert_eq!(names(&included), ["readonly.txt"]);
    assert_eq!(names(&excluded), ["writable.txt"]);
}

#[cfg(unix)]
#[test]
fn directory_links_do_not_recurse() {
    let tree = TempDir::new().unwrap();
    write(&tree, "src/a.txt", b"needle");
    std::os::unix::fs::symlink(tree.path(), tree.path().join("src/loop")).unwrap();
    assert_eq!(names(&search(&tree, "needle")), ["src/a.txt"]);
}

#[test]
fn missing_root_and_invalid_regex_are_reported() {
    let tree = TempDir::new().unwrap();
    let root = location(&tree)
        .join(&ProviderPath::from_text("missing"))
        .unwrap();
    let result = find_files(&root, &FindQuery::default(), &CancelToken::new(), |_| {
        ControlFlow::Continue(())
    });
    assert!(matches!(result, Err(FindError::Filesystem(_))));
    let query = FindQuery {
        text: "[".to_owned(),
        regex: true,
        ..FindQuery::default()
    };
    let result = find_files(&location(&tree), &query, &CancelToken::new(), |_| {
        ControlFlow::Continue(())
    });
    assert!(matches!(result, Err(FindError::Invalid(_))));
}
