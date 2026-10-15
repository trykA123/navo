use proptest::prelude::*;

use super::entry;
use crate::{Listing, Pattern, Selection};

#[derive(Debug, Clone)]
enum Op {
    Toggle(u64),
    Range(u64),
    MarkPattern(String, bool),
    UnmarkPattern(String, bool),
    MarkAll(bool),
    Invert(bool),
    Clear,
}

fn name() -> impl Strategy<Value = String> {
    "[ab.]{1,4}"
}

fn pattern_text() -> impl Strategy<Value = String> {
    "[ab.*?]{1,4}"
}

fn listing() -> impl Strategy<Value = Listing> {
    prop::collection::vec((name(), any::<bool>()), 0..=300).prop_map(|rows| {
        Listing::new(
            rows.iter()
                .map(|(name, is_folder)| entry(name, *is_folder))
                .collect(),
        )
    })
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..310_u64).prop_map(Op::Toggle),
        2 => (0..310_u64).prop_map(Op::Range),
        2 => (pattern_text(), any::<bool>()).prop_map(|(p, f)| Op::MarkPattern(p, f)),
        2 => (pattern_text(), any::<bool>()).prop_map(|(p, f)| Op::UnmarkPattern(p, f)),
        1 => any::<bool>().prop_map(Op::MarkAll),
        1 => any::<bool>().prop_map(Op::Invert),
        1 => Just(Op::Clear),
    ]
}

fn bulk_op(folders: bool) -> impl Strategy<Value = Op> {
    prop_oneof![
        pattern_text().prop_map(move |p| Op::MarkPattern(p, folders)),
        pattern_text().prop_map(move |p| Op::UnmarkPattern(p, folders)),
        Just(Op::MarkAll(folders)),
        Just(Op::Invert(folders)),
    ]
}

fn apply(selection: &mut Selection, listing: &Listing, op: &Op) {
    match op {
        Op::Toggle(row) => {
            selection.toggle(*row);
        }
        Op::Range(to) => selection.mark_range(*to),
        Op::MarkPattern(text, folders) => {
            selection.mark_pattern(listing, &Pattern::parse(text).unwrap(), *folders);
        }
        Op::UnmarkPattern(text, folders) => {
            selection.unmark_pattern(listing, &Pattern::parse(text).unwrap(), *folders);
        }
        Op::MarkAll(folders) => selection.mark_all(listing, *folders),
        Op::Invert(folders) => selection.invert(listing, *folders),
        Op::Clear => selection.clear(),
    }
}

fn scenario() -> impl Strategy<Value = (Listing, Selection)> {
    (listing(), prop::collection::vec(op(), 0..20)).prop_map(|(listing, ops)| {
        let mut selection = Selection::new(listing.len());
        for op in &ops {
            apply(&mut selection, &listing, op);
        }
        (listing, selection)
    })
}

fn reference(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|skip| reference(rest, &name[skip..])),
        Some(('?', rest)) => !name.is_empty() && reference(rest, &name[1..]),
        Some((c, rest)) => name.first() == Some(c) && reference(rest, &name[1..]),
    }
}

fn reference_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    pattern == ['*', '.', '*'] || reference(&pattern, &name)
}

proptest! {
    #[test]
    fn count_equals_marked_rows((_, selection) in scenario()) {
        prop_assert_eq!(selection.count(), selection.marked().len() as u64);
    }

    #[test]
    fn marked_rows_are_below_len((_, selection) in scenario()) {
        prop_assert!(selection.marked().iter().all(|&row| row < selection.len()));
    }

    #[test]
    fn marked_rows_are_ascending((_, selection) in scenario()) {
        prop_assert!(selection.marked().windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn invert_twice_restores((listing, before) in scenario(), folders in any::<bool>()) {
        let mut after = before.clone();
        after.invert(&listing, folders);
        after.invert(&listing, folders);
        prop_assert_eq!(after, before);
    }

    #[test]
    fn mark_all_then_invert_leaves_only_skipped_folders(
        (listing, before) in scenario(),
        folders in any::<bool>(),
    ) {
        let mut after = before.clone();
        after.mark_all(&listing, folders);
        after.invert(&listing, folders);
        for (row, entry) in (0..).zip(listing.entries()) {
            let expected = entry.is_folder && !folders && before.is_marked(row);
            prop_assert_eq!(after.is_marked(row), expected, "row {}", row);
        }
    }

    #[test]
    fn bulk_ops_without_folders_keep_folder_rows(
        (listing, before) in scenario(),
        op in bulk_op(false),
    ) {
        let mut after = before.clone();
        apply(&mut after, &listing, &op);
        for (row, entry) in (0..).zip(listing.entries()) {
            if entry.is_folder {
                prop_assert_eq!(after.is_marked(row), before.is_marked(row), "row {}", row);
            }
        }
    }

    #[test]
    fn matches_agrees_with_reference(pattern in "[ab.*?]{1,6}", name in "[ab.]{0,8}") {
        let parsed = Pattern::parse(&pattern).unwrap();
        prop_assert_eq!(parsed.matches(&name), reference_matches(&pattern, &name));
    }
}
