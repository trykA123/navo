use std::ops::ControlFlow;

use navo_compare::model::{
    CancelToken, EntryKind, ReadHint, Reason, RelPath, SideEntry, SideSource,
};
use navo_core::ProviderPath;
use std::io::Read;

use super::{location, write};
use crate::compare::local::LocalSide;

fn rel(text: &str) -> RelPath {
    RelPath::new(ProviderPath::from_text(text))
}

fn list(side: &LocalSide, folder: &str) -> Vec<SideEntry> {
    let mut found = Vec::new();
    side.list(
        &rel(folder),
        &mut |entry| {
            found.push(entry);
            ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .unwrap();
    found.sort_by_key(|entry| entry.rel.as_path().to_text());
    found
}

#[test]
fn list_gives_files_and_folders_with_row_paths() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    write(dir.path(), "sub/b.txt", b"hello");
    let side = LocalSide::new(location(dir.path())).unwrap();
    let top = list(&side, "");
    let names: Vec<_> = top.iter().map(|e| e.rel.as_path().to_text()).collect();
    assert_eq!(names, ["a.txt", "sub"]);
    assert_eq!((top[0].kind, top[0].size), (EntryKind::File, 3));
    assert_eq!(top[1].kind, EntryKind::Folder);
    let inner = list(&side, "sub");
    assert_eq!(inner[0].rel.as_path().to_text(), "sub/b.txt");
}

#[test]
fn open_reads_the_file_bytes() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "sub/b.txt", b"hello");
    let side = LocalSide::new(location(dir.path())).unwrap();
    let mut text = String::new();
    side.open(&rel("sub/b.txt"), ReadHint::Map)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "hello");
}

#[test]
fn stamp_matches_the_listed_entry() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    let side = LocalSide::new(location(dir.path())).unwrap();
    let entry = list(&side, "").remove(0);
    let stamp = side.stamp(&entry.rel).unwrap();
    assert_eq!((stamp.size, stamp.modified), (entry.size, entry.modified));
}

#[cfg(unix)]
#[test]
fn file_id_is_set_where_the_platform_gives_one() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    let side = LocalSide::new(location(dir.path())).unwrap();
    assert!(side.file_id(&rel("a.txt")).is_some());
    assert!(!side.read_only());
}

#[test]
fn a_missing_file_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let side = LocalSide::new(location(dir.path())).unwrap();
    assert!(matches!(
        side.open(&rel("gone.txt"), ReadHint::Buffered),
        Err(Reason::NotFound)
    ));
    assert_eq!(side.stamp(&rel("gone.txt")).unwrap_err(), Reason::NotFound);
}

#[test]
fn a_file_is_not_a_side_root() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    let file = location(&dir.path().join("a.txt"));
    assert!(matches!(LocalSide::new(file), Err(Reason::Unsupported)));
}

#[test]
fn a_cancelled_list_stops_without_entries() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    let side = LocalSide::new(location(dir.path())).unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    let mut seen = 0;
    side.list(
        &rel(""),
        &mut |_| {
            seen += 1;
            ControlFlow::Continue(())
        },
        &cancel,
    )
    .unwrap();
    assert_eq!(seen, 0);
}
