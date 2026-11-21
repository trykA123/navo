//! Tests for the folder tree model: tones, mixes, filters, expansion and the change map.

use sidewise_ui::browser::{CompareFilter, CompareKind, CompareRow, CompareSide, RowStatus};
use sidewise_ui::compare_diff::map::HunkKind;

use super::folders_model::{FolderTree, SwTone, TreeLine, map_hunks, tone_of};

/// A folder side with no size or time.
fn folder_side() -> Option<CompareSide> {
    Some(CompareSide {
        kind: CompareKind::Folder,
        size: 0,
        modified_ms: None,
    })
}

/// A file side with a placeholder size and no time.
fn file_side() -> Option<CompareSide> {
    Some(CompareSide {
        kind: CompareKind::File,
        size: 1,
        modified_ms: None,
    })
}

/// The fixture tree: folders `a` and `a/b`, files under them, folder `c` and the file `z.txt`.
fn rows() -> Vec<CompareRow> {
    vec![
        CompareRow::new(1, "a", RowStatus::Different, (folder_side(), folder_side())),
        CompareRow::new(
            2,
            "a/x.txt",
            RowStatus::Different,
            (file_side(), file_side()),
        ),
        CompareRow::new(
            3,
            "a/b",
            RowStatus::Different,
            (folder_side(), folder_side()),
        ),
        CompareRow::new(4, "a/b/y.txt", RowStatus::OnlyLeft, (file_side(), None)),
        CompareRow::new(5, "c", RowStatus::Same, (folder_side(), folder_side())),
        CompareRow::new(6, "c/s.txt", RowStatus::Same, (file_side(), file_side())),
        CompareRow::new(7, "z.txt", RowStatus::Same, (file_side(), file_side())),
    ]
}

/// The first line with `path`.
fn line<'a>(lines: &'a [TreeLine], path: &str) -> &'a TreeLine {
    lines
        .iter()
        .find(|line| line.path == path)
        .expect("a line with this path")
}

/// Every status maps to its Sidewise tone, and the unchecked ones to none.
#[test]
fn tone_of_maps_every_status() {
    let cases = [
        (RowStatus::Different, Some(SwTone::Changed)),
        (RowStatus::OnlyLeft, Some(SwTone::OnlyLeft)),
        (RowStatus::OnlyRight, Some(SwTone::OnlyRight)),
        (RowStatus::TypeConflict, Some(SwTone::Conflict)),
        (RowStatus::CaseConflict, Some(SwTone::Conflict)),
        (RowStatus::Same, Some(SwTone::Same)),
        (RowStatus::SameAfterRules, Some(SwTone::Same)),
        (RowStatus::SameSizeTime, Some(SwTone::Same)),
        (RowStatus::Checking, None),
        (RowStatus::NotChecked("reason"), None),
        (RowStatus::Unavailable("reason"), None),
    ];
    for (status, tone) in cases {
        assert_eq!(tone_of(status), tone, "{status:?}");
    }
}

/// A tree with nothing open lists only the root rows, all at depth 0.
#[test]
fn collapsed_tree_lists_top_level_only() {
    let tree = FolderTree::new(rows());
    let lines = tree.lines();
    let paths: Vec<&str> = lines.iter().map(|line| line.path.as_str()).collect();
    assert_eq!(paths, ["a", "c", "z.txt"]);
    let depths: Vec<u16> = lines.iter().map(|line| line.depth).collect();
    assert_eq!(depths, [0, 0, 0]);
}

/// A folder's mix sums every counting file below it, at any depth; file lines show zeros.
#[test]
fn folder_mix_counts_every_file_below() {
    let tree = FolderTree::new(rows());
    let lines = tree.lines();
    assert_eq!(line(&lines, "a").mix, [1, 1, 0, 0, 0]);
    assert_eq!(line(&lines, "c").mix, [0, 0, 0, 0, 1]);
    assert_eq!(line(&lines, "z.txt").mix, [0, 0, 0, 0, 0]);
    assert_eq!(tree.totals(), [1, 1, 0, 0, 2]);
}

/// Opening `a` shows its children in input order; opening `a/b` shows `y.txt` at depth 2.
#[test]
fn expanding_shows_children_in_input_order() {
    let mut tree = FolderTree::new(rows());
    assert!(tree.toggle("a"));
    let lines = tree.lines();
    let paths: Vec<&str> = lines.iter().map(|line| line.path.as_str()).collect();
    assert_eq!(paths, ["a", "a/x.txt", "a/b", "c", "z.txt"]);
    assert!(tree.toggle("a/b"));
    let lines = tree.lines();
    let shown: Vec<&str> = lines.iter().map(|line| line.path.as_str()).collect();
    assert_eq!(shown, ["a", "a/x.txt", "a/b", "a/b/y.txt", "c", "z.txt"]);
    assert_eq!(line(&lines, "a/b/y.txt").depth, 2);
}

/// Toggling a file path changes nothing.
#[test]
fn toggle_on_a_file_changes_nothing() {
    let mut tree = FolderTree::new(rows());
    let before = tree.lines();
    assert!(!tree.toggle("z.txt"));
    assert_eq!(tree.lines(), before);
}

/// The Differences filter drops same files and folders with no differing file below.
#[test]
fn differences_filter_hides_same_rows_and_same_folders() {
    let mut tree = FolderTree::new(rows());
    assert!(tree.toggle("a"));
    assert!(tree.toggle("c"));
    tree.set_filter(CompareFilter::Differences);
    let lines = tree.lines();
    let paths: Vec<&str> = lines.iter().map(|line| line.path.as_str()).collect();
    assert_eq!(paths, ["a", "a/x.txt", "a/b"]);
}

/// Replacing the rows keeps the open folders and the filter.
#[test]
fn set_rows_keeps_open_folders_and_filter() {
    let mut tree = FolderTree::new(rows());
    assert!(tree.toggle("a"));
    assert!(tree.toggle("a/b"));
    tree.set_filter(CompareFilter::Differences);
    let before_lines = tree.lines();
    let before_filter = tree.filter();
    tree.set_rows(rows());
    assert_eq!(tree.lines(), before_lines);
    assert_eq!(tree.filter(), before_filter);
}

/// Changed lines become changed hunks, only-left lines deleted hunks, at their line indexes.
#[test]
fn map_hunks_marks_changed_lines() {
    let mut tree = FolderTree::new(rows());
    assert!(tree.toggle("a"));
    let hunks = map_hunks(&tree.lines());
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].rows, 1..2);
    assert_eq!(hunks[0].kind, HunkKind::Changed);
    assert!(tree.toggle("a/b"));
    let hunks = map_hunks(&tree.lines());
    assert_eq!(hunks.len(), 2);
    assert_eq!(hunks[1].rows, 3..4);
    assert_eq!(hunks[1].kind, HunkKind::Deleted);
}

/// A file row with `status` and both sides present, under `a`.
fn file_row(id: u64, path: &str, status: RowStatus) -> CompareRow {
    CompareRow::new(id, path, status, (file_side(), file_side()))
}

/// Checking, not-checked and unavailable files get no tone and stay out of mixes and totals.
#[test]
fn unsettled_statuses_stay_out_of_mixes_and_totals() {
    let tree = FolderTree::new(unsettled_rows());
    assert_eq!(tree.totals(), [1, 0, 0, 0, 0]);
    let mut open = FolderTree::new(unsettled_rows());
    assert!(open.toggle("a"));
    let lines = open.lines();
    assert_eq!(line(&lines, "a").mix, [1, 0, 0, 0, 0]);
    for path in ["a/c.txt", "a/n.txt", "a/u.txt"] {
        assert_eq!(line(&lines, path).tone, None, "{path}");
    }
}

/// Folder `a` with one settled file and three unsettled ones.
fn unsettled_rows() -> Vec<CompareRow> {
    vec![
        CompareRow::new(1, "a", RowStatus::Different, (folder_side(), folder_side())),
        file_row(2, "a/c.txt", RowStatus::Checking),
        file_row(3, "a/n.txt", RowStatus::NotChecked("big")),
        file_row(4, "a/u.txt", RowStatus::Unavailable("denied")),
        file_row(5, "a/d.txt", RowStatus::Different),
    ]
}

/// A changed row list drops open paths that no longer exist, and a path that returns stays closed.
#[test]
fn set_rows_drops_stale_open_paths_and_keeps_the_filter() {
    let mut tree = FolderTree::new(rows());
    assert!(tree.toggle("a"));
    assert!(tree.toggle("a/b"));
    tree.set_filter(CompareFilter::Differences);
    tree.set_rows(
        rows()
            .into_iter()
            .filter(|r| r.path != "a/b" && r.path != "a/b/y.txt")
            .collect(),
    );
    assert_eq!(tree.filter(), CompareFilter::Differences);
    tree.set_rows(rows());
    let lines = tree.lines();
    let paths: Vec<&str> = lines.iter().map(|line| line.path.as_str()).collect();
    assert_eq!(paths, ["a", "a/x.txt", "a/b"]);
    assert!(!line(&lines, "a/b").expanded);
}
