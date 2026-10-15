use super::marked;
use crate::Selection;

#[test]
fn new_has_the_given_len() {
    assert_eq!(Selection::new(7).len(), 7);
}

#[test]
fn new_marks_nothing() {
    assert_eq!(Selection::new(7).count(), 0);
}

#[test]
fn new_is_empty() {
    assert!(Selection::new(7).is_empty());
}

#[test]
fn new_has_no_anchor() {
    assert_eq!(Selection::new(7).anchor(), None);
}

#[test]
fn new_has_no_marked_rows() {
    assert!(Selection::new(7).marked().is_empty());
}

#[test]
fn toggle_marks_an_unmarked_row() {
    let mut selection = Selection::new(5);
    assert!(selection.toggle(2));
}

#[test]
fn toggled_row_is_marked() {
    assert!(marked(&[2], 5).is_marked(2));
}

#[test]
fn toggle_leaves_other_rows_unmarked() {
    assert!(!marked(&[2], 5).is_marked(3));
}

#[test]
fn toggle_unmarks_a_marked_row() {
    let mut selection = marked(&[2], 5);
    assert!(!selection.toggle(2));
}

#[test]
fn toggle_twice_leaves_row_unmarked() {
    assert!(!marked(&[2, 2], 5).is_marked(2));
}

#[test]
fn toggle_sets_the_anchor() {
    assert_eq!(marked(&[1, 3], 5).anchor(), Some(3));
}

#[test]
fn toggle_off_also_sets_the_anchor() {
    assert_eq!(marked(&[1, 3, 1], 5).anchor(), Some(1));
}

#[test]
fn count_follows_toggles() {
    assert_eq!(marked(&[0, 4, 2, 4], 5).count(), 2);
}

#[test]
fn marked_selection_is_not_empty() {
    assert!(!marked(&[0], 5).is_empty());
}

#[test]
fn marked_lists_rows_ascending() {
    assert_eq!(marked(&[4, 0, 2], 5).marked(), vec![0, 2, 4]);
}

#[test]
fn toggle_past_the_end_returns_false() {
    let mut selection = Selection::new(5);
    assert!(!selection.toggle(5));
}

#[test]
fn toggle_past_the_end_changes_nothing() {
    let before = marked(&[1], 5);
    let mut after = before.clone();
    after.toggle(9);
    assert_eq!(after, before);
}

#[test]
fn is_marked_past_the_end_is_false() {
    assert!(!marked(&[4], 5).is_marked(5));
}

#[test]
fn toggle_on_empty_selection_returns_false() {
    let mut selection = Selection::new(0);
    assert!(!selection.toggle(0));
}

#[test]
fn range_marks_forward_from_the_anchor() {
    let mut selection = marked(&[2], 10);
    selection.mark_range(5);
    assert_eq!(selection.marked(), vec![2, 3, 4, 5]);
}

#[test]
fn range_marks_backward_from_the_anchor() {
    let mut selection = marked(&[6], 10);
    selection.mark_range(3);
    assert_eq!(selection.marked(), vec![3, 4, 5, 6]);
}

#[test]
fn range_includes_an_unmarked_anchor() {
    let mut selection = marked(&[2, 2], 10);
    selection.mark_range(4);
    assert_eq!(selection.marked(), vec![2, 3, 4]);
}

#[test]
fn range_to_the_anchor_marks_the_anchor() {
    let mut selection = marked(&[3, 3], 10);
    selection.mark_range(3);
    assert_eq!(selection.marked(), vec![3]);
}

#[test]
fn range_keeps_the_anchor() {
    let mut selection = marked(&[2], 10);
    selection.mark_range(7);
    assert_eq!(selection.anchor(), Some(2));
}

#[test]
fn range_keeps_rows_outside_it() {
    let mut selection = marked(&[0, 9, 4], 10);
    selection.mark_range(6);
    assert_eq!(selection.marked(), vec![0, 4, 5, 6, 9]);
}

#[test]
fn range_keeps_marked_rows_inside_it_marked() {
    let mut selection = marked(&[3, 1], 10);
    selection.mark_range(4);
    assert_eq!(selection.marked(), vec![1, 2, 3, 4]);
}

#[test]
fn range_without_anchor_marks_only_the_target() {
    let mut selection = Selection::new(10);
    selection.mark_range(4);
    assert_eq!(selection.marked(), vec![4]);
}

#[test]
fn range_past_the_end_is_clamped() {
    let mut selection = marked(&[7], 10);
    selection.mark_range(50);
    assert_eq!(selection.marked(), vec![7, 8, 9]);
}

#[test]
fn range_without_anchor_past_the_end_marks_nothing() {
    let mut selection = Selection::new(10);
    selection.mark_range(10);
    assert!(selection.is_empty());
}

#[test]
fn range_count_matches_the_rows() {
    let mut selection = marked(&[1], 10);
    selection.mark_range(8);
    assert_eq!(selection.count(), 8);
}

#[test]
fn clear_unmarks_everything() {
    let mut selection = marked(&[0, 3, 4], 5);
    selection.clear();
    assert!(selection.marked().is_empty());
}

#[test]
fn clear_sets_count_to_zero() {
    let mut selection = marked(&[0, 3, 4], 5);
    selection.clear();
    assert_eq!(selection.count(), 0);
}

#[test]
fn clear_drops_the_anchor() {
    let mut selection = marked(&[3], 5);
    selection.clear();
    assert_eq!(selection.anchor(), None);
}

#[test]
fn clear_keeps_the_len() {
    let mut selection = marked(&[3], 5);
    selection.clear();
    assert_eq!(selection.len(), 5);
}
