use super::{files, marked, mixed, pat};
use crate::Selection;

#[test]
fn mark_pattern_marks_matching_files() {
    let mut selection = Selection::new(6);
    selection.mark_pattern(&mixed(), &pat("*.rs"), false);
    assert_eq!(selection.marked(), vec![1, 4]);
}

#[test]
fn mark_pattern_skips_folders() {
    let mut selection = Selection::new(6);
    selection.mark_pattern(&mixed(), &pat("*.rs"), false);
    assert!(!selection.is_marked(5));
}

#[test]
fn mark_pattern_includes_folders_when_asked() {
    let mut selection = Selection::new(6);
    selection.mark_pattern(&mixed(), &pat("*.rs"), true);
    assert_eq!(selection.marked(), vec![1, 4, 5]);
}

#[test]
fn mark_pattern_keeps_other_marks() {
    let mut selection = marked(&[2], 6);
    selection.mark_pattern(&mixed(), &pat("*.rs"), false);
    assert_eq!(selection.marked(), vec![1, 2, 4]);
}

#[test]
fn mark_pattern_keeps_a_skipped_folder_marked() {
    let mut selection = marked(&[5], 6);
    selection.mark_pattern(&mixed(), &pat("*.rs"), false);
    assert!(selection.is_marked(5));
}

#[test]
fn mark_pattern_keeps_the_anchor() {
    let mut selection = marked(&[2], 6);
    selection.mark_pattern(&mixed(), &pat("*"), true);
    assert_eq!(selection.anchor(), Some(2));
}

#[test]
fn mark_pattern_count_matches() {
    let mut selection = Selection::new(6);
    selection.mark_pattern(&mixed(), &pat("*.*"), false);
    assert_eq!(selection.count(), 3);
}

#[test]
fn unmark_pattern_unmarks_matching_files() {
    let mut selection = marked(&[1, 2, 4], 6);
    selection.unmark_pattern(&mixed(), &pat("*.rs"), false);
    assert_eq!(selection.marked(), vec![2]);
}

#[test]
fn unmark_pattern_keeps_a_skipped_folder_marked() {
    let mut selection = marked(&[1, 5], 6);
    selection.unmark_pattern(&mixed(), &pat("*.rs"), false);
    assert_eq!(selection.marked(), vec![5]);
}

#[test]
fn unmark_pattern_includes_folders_when_asked() {
    let mut selection = marked(&[1, 5], 6);
    selection.unmark_pattern(&mixed(), &pat("*.rs"), true);
    assert!(selection.is_empty());
}

#[test]
fn unmark_pattern_keeps_the_anchor() {
    let mut selection = marked(&[1], 6);
    selection.unmark_pattern(&mixed(), &pat("*"), true);
    assert_eq!(selection.anchor(), Some(1));
}

#[test]
fn mark_all_marks_every_file() {
    let mut selection = Selection::new(6);
    selection.mark_all(&mixed(), false);
    assert_eq!(selection.marked(), vec![1, 2, 4]);
}

#[test]
fn mark_all_includes_folders_when_asked() {
    let mut selection = Selection::new(6);
    selection.mark_all(&mixed(), true);
    assert_eq!(selection.count(), 6);
}

#[test]
fn mark_all_keeps_a_skipped_folder_marked() {
    let mut selection = marked(&[3], 6);
    selection.mark_all(&mixed(), false);
    assert_eq!(selection.marked(), vec![1, 2, 3, 4]);
}

#[test]
fn mark_all_keeps_the_anchor() {
    let mut selection = marked(&[3], 6);
    selection.mark_all(&mixed(), true);
    assert_eq!(selection.anchor(), Some(3));
}

#[test]
fn invert_flips_files() {
    let mut selection = marked(&[1], 6);
    selection.invert(&mixed(), false);
    assert_eq!(selection.marked(), vec![2, 4]);
}

#[test]
fn invert_keeps_skipped_folders() {
    let mut selection = marked(&[0, 1], 6);
    selection.invert(&mixed(), false);
    assert_eq!(selection.marked(), vec![0, 2, 4]);
}

#[test]
fn invert_flips_folders_when_asked() {
    let mut selection = marked(&[0, 1], 6);
    selection.invert(&mixed(), true);
    assert_eq!(selection.marked(), vec![2, 3, 4, 5]);
}

#[test]
fn invert_twice_is_identity() {
    let before = marked(&[0, 1, 5], 6);
    let mut after = before.clone();
    after.invert(&mixed(), true);
    after.invert(&mixed(), true);
    assert_eq!(after, before);
}

#[test]
fn invert_keeps_the_anchor() {
    let mut selection = marked(&[4], 6);
    selection.invert(&mixed(), true);
    assert_eq!(selection.anchor(), Some(4));
}

#[test]
fn invert_count_matches() {
    let mut selection = marked(&[1], 6);
    selection.invert(&mixed(), true);
    assert_eq!(selection.count(), 5);
}

#[test]
fn shorter_listing_acts_on_its_rows_only() {
    let mut selection = Selection::new(6);
    selection.mark_all(&files(3), false);
    assert_eq!(selection.marked(), vec![0, 1, 2]);
}

#[test]
fn longer_listing_acts_on_the_selection_rows_only() {
    let mut selection = Selection::new(3);
    selection.invert(&files(6), false);
    assert_eq!(selection.marked(), vec![0, 1, 2]);
}

#[test]
fn longer_listing_keeps_the_len() {
    let mut selection = Selection::new(3);
    selection.mark_pattern(&files(6), &pat("*"), false);
    assert_eq!(selection.len(), 3);
}
