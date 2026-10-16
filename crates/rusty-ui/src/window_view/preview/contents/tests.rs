use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use super::{CHILD_LIMIT, Contents, RowKind};

fn entry(name: &str, kind: EntryKind) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind,
        size: 1,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn root() -> Location {
    Location::new("/work").unwrap()
}

fn read_root(contents: &mut Contents) {
    let (node, _) = contents.root().unwrap();
    contents.attach(
        node,
        Ok(vec![
            entry("src", EntryKind::Folder),
            entry("notes.txt", EntryKind::File),
        ]),
    );
}

fn names(contents: &Contents) -> Vec<String> {
    (0..contents.len())
        .filter_map(|index| match contents.row(index)?.0 {
            RowKind::Entry(entry) => Some(entry.name.to_text()),
            RowKind::More(hidden) => Some(format!("+{hidden}")),
        })
        .collect()
}

#[test]
fn nothing_shows_until_the_folder_is_read() {
    let contents = Contents::new(root());
    assert!(!contents.is_read());
    assert_eq!(contents.len(), 0);
}

#[test]
fn a_subfolder_can_open_but_its_children_wait_until_it_does() {
    let mut contents = Contents::new(root());
    read_root(&mut contents);
    assert_eq!(names(&contents), ["src", "notes.txt"]);
    let (_, shape) = contents.row(0).unwrap();
    assert!(shape.can_open && !shape.is_open);
    let (node, location) = contents.toggle(0).expect("src needs a read");
    assert_eq!(location.as_str(), "/work/src");
    assert_eq!(names(&contents), ["src", "notes.txt"]);
    contents.attach(node, Ok(vec![entry("main.rs", EntryKind::File)]));
    assert_eq!(names(&contents), ["src", "main.rs", "notes.txt"]);
    assert_eq!(contents.row(1).unwrap().1.depth, 1);
}

#[test]
fn closing_and_reopening_a_read_folder_needs_no_second_read() {
    let mut contents = Contents::new(root());
    read_root(&mut contents);
    let (node, _) = contents.toggle(0).unwrap();
    contents.attach(node, Ok(vec![entry("main.rs", EntryKind::File)]));
    assert!(contents.toggle(0).is_none());
    assert_eq!(names(&contents), ["src", "notes.txt"]);
    assert!(contents.toggle(0).is_none());
    assert_eq!(names(&contents), ["src", "main.rs", "notes.txt"]);
}

#[test]
fn a_huge_folder_shows_the_first_part_and_a_count_of_the_rest() {
    let mut contents = Contents::new(root());
    let (node, _) = contents.root().unwrap();
    let many = (0..CHILD_LIMIT + 5)
        .map(|index| entry(&format!("f{index}"), EntryKind::File))
        .collect();
    contents.attach(node, Ok(many));
    assert_eq!(contents.len(), CHILD_LIMIT + 1);
    assert_eq!(names(&contents).last().map(String::as_str), Some("+5"));
}

#[test]
fn an_unreadable_folder_says_why() {
    let mut contents = Contents::new(root());
    let (node, _) = contents.root().unwrap();
    contents.attach(node, Err("Access denied.".to_owned()));
    assert_eq!(contents.failure(), Some("Access denied."));
    assert!(contents.is_read());
}
