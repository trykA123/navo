//! Tests for the side cells of the folder tree: cells, item counts, newer side, gutter tags.

use sidewise_ui::browser::{CompareKind, CompareRow, CompareSide, RowStatus};

use super::folders_cells::{GutTag, Newer, child_counts, gut_of, newer_of};
use super::folders_model::FolderTree;

/// A side of `kind` with a size and an optional time.
fn side(kind: CompareKind, size: u64, modified_ms: Option<i64>) -> Option<CompareSide> {
    Some(CompareSide {
        kind,
        size,
        modified_ms,
    })
}

/// The fixture: `a` (x.txt, b), `a/b` (y.txt on the left only), `c`.
fn rows() -> Vec<CompareRow> {
    let folder = || side(CompareKind::Folder, 0, None);
    let file = || side(CompareKind::File, 7, Some(100));
    vec![
        CompareRow::new(1, "a", RowStatus::Different, (folder(), folder())),
        CompareRow::new(2, "a/x.txt", RowStatus::Different, (file(), file())),
        CompareRow::new(3, "a/b", RowStatus::Different, (folder(), folder())),
        CompareRow::new(4, "a/b/y.txt", RowStatus::OnlyLeft, (file(), None)),
        CompareRow::new(5, "c", RowStatus::Same, (folder(), folder())),
    ]
}

/// A changed file with the two modified times.
fn changed(left: Option<i64>, right: Option<i64>, status: RowStatus) -> CompareRow {
    let file = |t| side(CompareKind::File, 1, t);
    CompareRow::new(9, "f.txt", status, (file(left), file(right)))
}

/// A file present on one side only has no cell on the other.
#[test]
fn a_one_sided_file_has_no_right_cell() {
    let mut tree = FolderTree::new(rows());
    tree.expand_all_under("a");
    let lines = tree.lines();
    let line = lines.iter().find(|l| l.path == "a/b/y.txt").unwrap();
    assert!(line.left.is_some());
    assert!(line.right.is_none());
    assert_eq!(line.left.unwrap().size, 7);
}

/// A folder counts its direct children on each side.
#[test]
fn folder_items_count_direct_children_per_side() {
    let rows = rows();
    let counts = child_counts(&rows);
    assert_eq!(counts["a"], [2, 2]);
    assert_eq!(counts["a/b"], [1, 0]);
    let mut tree = FolderTree::new(rows);
    tree.expand_all_under("a");
    let lines = tree.lines();
    let a = lines.iter().find(|l| l.path == "a").unwrap();
    assert_eq!(a.left.unwrap().items, Some(2));
}

/// The newer side needs a changed file with two different known times.
#[test]
fn newer_needs_a_changed_file_and_two_times() {
    let diff = RowStatus::Different;
    assert_eq!(
        newer_of(&changed(Some(100), Some(200), diff)),
        Some(Newer::Right)
    );
    assert_eq!(
        newer_of(&changed(Some(300), Some(200), diff)),
        Some(Newer::Left)
    );
    assert_eq!(newer_of(&changed(Some(200), Some(200), diff)), None);
    assert_eq!(
        newer_of(&changed(Some(100), Some(200), RowStatus::Same)),
        None
    );
    assert_eq!(newer_of(&changed(None, Some(200), diff)), None);
}

/// Each status maps to its gutter tag; Same shows none.
#[test]
fn gutter_tag_follows_the_status() {
    assert_eq!(
        gut_of(RowStatus::SameAfterRules),
        Some(GutTag::SameAfterRules)
    );
    assert_eq!(gut_of(RowStatus::Same), None);
    assert_eq!(gut_of(RowStatus::Checking), Some(GutTag::Checking));
    assert_eq!(gut_of(RowStatus::NotChecked("x")), Some(GutTag::Checking));
    assert_eq!(gut_of(RowStatus::Different), Some(GutTag::Changed));
    assert_eq!(gut_of(RowStatus::TypeConflict), Some(GutTag::Conflict));
    assert_eq!(gut_of(RowStatus::OnlyLeft), Some(GutTag::OnlyLeft));
    assert_eq!(gut_of(RowStatus::OnlyRight), Some(GutTag::OnlyRight));
}

/// Expand all opens the folder and every folder below it, not its siblings.
#[test]
fn expand_all_opens_every_folder_below() {
    let mut tree = FolderTree::new(rows());
    tree.expand_all_under("a");
    let lines = tree.lines();
    let open = |p: &str| lines.iter().find(|l| l.path == p).unwrap().expanded;
    assert!(open("a"));
    assert!(open("a/b"));
    assert!(!open("c"));
}
