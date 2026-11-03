use std::sync::Arc;
use std::sync::mpsc;

use navo_compare::model::HashCache;
use navo_compare::model::Status;

use super::handle::{final_status, finish};
use super::{location, write};
use crate::compare::handle::{CompareHandle, CompareStart, CompareTarget};
use crate::compare::hash_cache::StoreHashCache;
use crate::compare::settings::CompareSettings;
use navo_store::CacheDb;

fn run(settings: CompareSettings) -> Vec<String> {
    let (left, right) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for root in [left.path(), right.path()] {
        write(root, ".gitignore", b"skip.txt\n");
        write(root, "keep.txt", b"k");
        write(root, "skip.txt", b"s");
        write(root, "mine.log", b"m");
    }
    let (sender, receiver) = mpsc::channel();
    let cache: Arc<dyn HashCache> =
        Arc::new(StoreHashCache::new(CacheDb::open_in_memory().unwrap()));
    let request = CompareStart {
        left: CompareTarget::Local(location(left.path())),
        right: CompareTarget::Local(location(right.path())),
        settings,
        cache,
    };
    let _handle = CompareHandle::start(request, sender);
    let (rows, _, _) = finish(&receiver);
    let mut names: Vec<_> = rows
        .iter()
        .filter_map(|row| row.left.as_ref())
        .map(|entry| entry.rel.as_path().to_text())
        .collect();
    names.sort();
    names.dedup();
    for name in &names {
        assert_eq!(final_status(&rows, name), Status::Same);
    }
    names
}

#[test]
fn the_root_gitignore_hides_rows_when_respected() {
    let names = run(CompareSettings::default());
    assert_eq!(names, [".gitignore", "keep.txt", "mine.log"]);
}

#[test]
fn the_root_gitignore_is_ignored_when_not_respected() {
    let settings = CompareSettings {
        respect_gitignore: false,
        ..CompareSettings::default()
    };
    let names = run(settings);
    assert_eq!(names, [".gitignore", "keep.txt", "mine.log", "skip.txt"]);
}

#[test]
fn the_user_ignore_list_hides_rows() {
    let settings = CompareSettings {
        ignore_list: vec!["*.log".to_owned()],
        ..CompareSettings::default()
    };
    let names = run(settings);
    assert_eq!(names, [".gitignore", "keep.txt"]);
}
