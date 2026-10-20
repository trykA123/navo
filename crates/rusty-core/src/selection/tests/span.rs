use proptest::prelude::*;

use super::{entry, marked};
use crate::{Listing, Selection};

#[test]
fn extend_grows_and_shrinks_from_the_cursor() {
    let mut selection = Selection::new(10);
    selection.extend(2, 4, false);
    assert_eq!(selection.marked(), vec![2, 3, 4]);
    selection.extend(4, 6, false);
    assert_eq!(selection.marked(), vec![2, 3, 4, 5, 6]);
    selection.extend(6, 3, false);
    assert_eq!(selection.marked(), vec![2, 3]);
}

#[test]
fn extend_crosses_the_anchor() {
    let mut selection = Selection::new(10);
    selection.extend(4, 7, false);
    selection.extend(7, 2, false);
    assert_eq!(selection.marked(), vec![2, 3, 4]);
}

#[test]
fn extend_preserves_the_base_inside_and_outside_the_span() {
    let mut selection = marked(&[0, 4, 9], 10);
    selection.extend(2, 6, false);
    selection.extend(6, 3, false);
    assert_eq!(selection.marked(), vec![0, 2, 3, 4, 9]);
}

#[test]
fn extend_unmarks_and_restores_the_base_when_shrinking() {
    let mut selection = marked(&[0, 2, 3, 4, 5, 9], 10);
    selection.extend(2, 5, true);
    assert_eq!(selection.marked(), vec![0, 9]);
    selection.extend(5, 3, true);
    assert_eq!(selection.marked(), vec![0, 4, 5, 9]);
    selection.extend(3, 1, true);
    assert_eq!(selection.marked(), vec![0, 3, 4, 5, 9]);
}

#[test]
fn extend_keeps_the_initial_mode() {
    let mut selection = Selection::new(5);
    selection.extend(1, 2, false);
    selection.extend(2, 3, true);
    assert_eq!(selection.marked(), vec![1, 2, 3]);
}

#[test]
fn extend_clamps_both_endpoints() {
    let mut selection = Selection::new(5);
    selection.extend(u64::MAX, 2, false);
    assert_eq!(selection.marked(), vec![2, 3, 4]);
    selection.extend(2, u64::MAX, false);
    assert_eq!(selection.marked(), vec![4]);
}

#[test]
fn extend_on_empty_selection_is_empty() {
    let mut selection = Selection::new(0);
    selection.extend(0, u64::MAX, false);
    assert!(selection.is_empty());
}

#[test]
fn end_span_starts_again_from_the_current_marks_and_cursor() {
    let mut selection = Selection::new(8);
    selection.extend(1, 3, false);
    selection.end_span();
    selection.extend(5, 6, false);
    selection.extend(6, 5, false);
    assert_eq!(selection.marked(), vec![1, 2, 3, 5]);
}

#[test]
fn clear_ends_the_span() {
    let mut selection = marked(&[7], 8);
    selection.extend(1, 3, false);
    selection.clear();
    selection.extend(5, 6, false);
    assert_eq!(selection.marked(), vec![5, 6]);
}

fn extensions() -> Listing {
    Listing::new(vec![
        entry("a.TXT", false),
        entry("b.txt", false),
        entry("archive.tar.gz", false),
        entry(".profile", false),
        entry("README", false),
        entry("name.", false),
        entry("folder.txt", true),
        entry("folder", true),
        entry(".config.txt", false),
    ])
}

#[test]
fn extension_matches_case_insensitively_and_skips_folders() {
    let mut selection = marked(&[6], 9);
    selection.mark_extension(&extensions(), Some("tXt"), true);
    assert_eq!(selection.marked(), vec![0, 1, 6, 8]);
    selection.mark_extension(&extensions(), Some("TXT"), false);
    assert_eq!(selection.marked(), vec![6]);
}

#[test]
fn extension_uses_only_the_last_suffix() {
    let mut selection = Selection::new(9);
    selection.mark_extension(&extensions(), Some("tar.gz"), true);
    assert!(selection.is_empty());
    selection.mark_extension(&extensions(), Some("gz"), true);
    assert_eq!(selection.marked(), vec![2]);
}

#[test]
fn no_extension_includes_dotfiles_and_trailing_dots_but_skips_folders() {
    let mut selection = Selection::new(9);
    selection.mark_extension(&extensions(), None, true);
    assert_eq!(selection.marked(), vec![3, 4, 5]);
    selection.mark_extension(&extensions(), None, false);
    assert!(selection.is_empty());
}

proptest! {
    #[test]
    fn extend_matches_base_and_range_for_any_sequence(
        len in 0..150_u64,
        base in prop::collection::vec(0..160_u64, 0..100),
        anchor in 0..160_u64,
        targets in prop::collection::vec(0..160_u64, 1..60),
        unmark in any::<bool>(),
    ) {
        let mut selection = Selection::new(len);
        selection.mark_rows(base.iter().copied());
        for to in targets {
            selection.extend(anchor, to, unmark);
            let expected: Vec<u64> = (0..len).filter(|row| {
                let anchor = anchor.min(len.saturating_sub(1));
                let to = to.min(len.saturating_sub(1));
                let in_span = (anchor.min(to)..=anchor.max(to)).contains(row);
                if in_span { !unmark } else { base.contains(row) }
            }).collect();
            prop_assert_eq!(selection.marked(), expected);
        }
    }
}
