use proptest::prelude::*;

use crate::{Location, NameError};

use super::super::check::{
    Existing, ExistingName, Fold, Problem, Row, RowState, check_rows, fold_key,
};

fn parent() -> Location {
    Location::new("/dir").unwrap()
}

fn row(old: &str, new: &str) -> Row {
    Row {
        parent: parent(),
        old: old.to_owned(),
        new: new.to_owned(),
        excluded: false,
    }
}

fn skipped(old: &str) -> Row {
    Row {
        excluded: true,
        ..row(old, old)
    }
}

fn listing(names: &[&str]) -> Existing {
    Existing::new(
        names
            .iter()
            .map(|name| ExistingName {
                parent: parent(),
                name: (*name).to_owned(),
            })
            .collect(),
    )
}

fn states(rows: &[Row], names: &[&str], fold: Fold) -> Vec<RowState> {
    check_rows(rows, &listing(names), fold)
}

const CHANGES: RowState = RowState::Changes { case_only: false };

#[test]
fn a_new_name_is_a_change() {
    assert_eq!(states(&[row("a", "b")], &["a"], Fold::Exact), [CHANGES]);
}

#[test]
fn the_same_name_is_unchanged() {
    assert_eq!(
        states(&[row("a", "a")], &["a"], Fold::Insensitive),
        [RowState::Unchanged]
    );
}

#[test]
fn an_excluded_row_stays_excluded_whatever_its_name() {
    let rows = [Row {
        excluded: true,
        ..row("a", "")
    }];
    assert_eq!(states(&rows, &["a"], Fold::Exact), [RowState::Excluded]);
}

#[test]
fn an_empty_name_is_a_problem() {
    assert_eq!(
        states(&[row("a", "")], &["a"], Fold::Exact),
        [RowState::Problem(Problem::Empty)]
    );
}

#[test]
fn a_blank_name_is_empty() {
    assert_eq!(
        states(&[row("a", "  ")], &["a"], Fold::Exact),
        [RowState::Problem(Problem::Empty)]
    );
}

#[test]
fn an_invalid_name_carries_the_reason() {
    let long = "x".repeat(256);
    let cases = [
        ("a:b", NameError::Character(':')),
        ("..", NameError::DotName),
        ("b.", NameError::Trailing),
        ("NUL.txt", NameError::Reserved),
        (long.as_str(), NameError::TooLong),
    ];
    for (new, error) in cases {
        assert_eq!(
            states(&[row("a", new)], &["a"], Fold::Exact),
            [RowState::Problem(Problem::Invalid(error))],
            "{new}"
        );
    }
}

#[test]
fn two_rows_with_one_new_name_both_fail() {
    let rows = [row("a", "x"), row("b", "x")];
    assert_eq!(
        states(&rows, &["a", "b"], Fold::Exact),
        [
            RowState::Problem(Problem::Duplicate(1)),
            RowState::Problem(Problem::Duplicate(0)),
        ]
    );
}

#[test]
fn three_rows_with_one_new_name_point_at_another_row() {
    let rows = [row("a", "x"), row("b", "x"), row("c", "x")];
    assert_eq!(
        states(&rows, &["a", "b", "c"], Fold::Exact),
        [
            RowState::Problem(Problem::Duplicate(1)),
            RowState::Problem(Problem::Duplicate(0)),
            RowState::Problem(Problem::Duplicate(0)),
        ]
    );
}

#[test]
fn a_case_differing_duplicate_is_a_problem_when_insensitive() {
    let rows = [row("a", "X"), row("b", "x")];
    assert_eq!(
        states(&rows, &["a", "b"], Fold::Insensitive),
        [
            RowState::Problem(Problem::Duplicate(1)),
            RowState::Problem(Problem::Duplicate(0)),
        ]
    );
}

#[test]
fn a_case_differing_duplicate_is_fine_when_exact() {
    let rows = [row("a", "X"), row("b", "x")];
    assert_eq!(states(&rows, &["a", "b"], Fold::Exact), [CHANGES, CHANGES]);
}

#[test]
fn duplicates_in_different_folders_are_fine() {
    let other = Location::new("/other").unwrap();
    let rows = [
        row("a", "x"),
        Row {
            parent: other,
            ..row("b", "x")
        },
    ];
    assert_eq!(states(&rows, &["a"], Fold::Exact), [CHANGES, CHANGES]);
}

#[test]
fn a_row_clashing_with_an_unchanged_row_is_a_duplicate() {
    let rows = [row("a", "b"), row("b", "b")];
    assert_eq!(
        states(&rows, &["a", "b"], Fold::Exact),
        [
            RowState::Problem(Problem::Duplicate(1)),
            RowState::Unchanged,
        ]
    );
}

#[test]
fn a_name_outside_the_batch_exists() {
    assert_eq!(
        states(&[row("a", "z")], &["a", "z"], Fold::Exact),
        [RowState::Problem(Problem::Exists)]
    );
}

#[test]
fn a_name_outside_the_batch_exists_in_another_case_when_insensitive() {
    assert_eq!(
        states(&[row("a", "z")], &["a", "Z"], Fold::Insensitive),
        [RowState::Problem(Problem::Exists)]
    );
    assert_eq!(
        states(&[row("a", "z")], &["a", "Z"], Fold::Exact),
        [CHANGES]
    );
}

#[test]
fn an_excluded_rows_name_exists_even_if_the_listing_misses_it() {
    let rows = [row("a", "b"), skipped("b")];
    assert_eq!(
        states(&rows, &[], Fold::Exact),
        [RowState::Problem(Problem::Exists), RowState::Excluded]
    );
}

#[test]
fn a_target_equal_to_another_rows_old_name_is_fine() {
    let rows = [row("a", "b"), row("b", "c")];
    assert_eq!(states(&rows, &["a", "b"], Fold::Exact), [CHANGES, CHANGES]);
}

#[test]
fn a_swap_is_fine() {
    let rows = [row("a", "b"), row("b", "a")];
    assert_eq!(states(&rows, &["a", "b"], Fold::Exact), [CHANGES, CHANGES]);
}

#[test]
fn a_case_only_rename_does_not_exist_against_itself() {
    assert_eq!(
        states(&[row("a.txt", "A.txt")], &["a.txt"], Fold::Insensitive),
        [RowState::Changes { case_only: true }]
    );
}

#[test]
fn a_case_only_rename_is_a_plain_change_when_exact() {
    assert_eq!(
        states(&[row("a.txt", "A.txt")], &["a.txt"], Fold::Exact),
        [CHANGES]
    );
}

#[test]
fn a_case_and_name_change_is_not_case_only() {
    assert_eq!(
        states(&[row("a.txt", "B.txt")], &["a.txt"], Fold::Insensitive),
        [CHANGES]
    );
}

#[test]
fn folding_maps_simple_case_pairs_and_keeps_multi_char_ones() {
    assert_eq!(fold_key("Straße ÀΣς", Fold::Insensitive), "straße àσσ");
    assert_eq!(fold_key("AbC", Fold::Exact), "AbC");
}

proptest! {
    #[test]
    fn random_names_never_panic(
        olds in proptest::collection::vec("\\PC{0,12}", 0..8),
        news in proptest::collection::vec("\\PC{0,12}", 0..8),
        listed in proptest::collection::vec("\\PC{0,12}", 0..8),
        excluded in proptest::collection::vec(any::<bool>(), 0..8),
        insensitive in any::<bool>(),
    ) {
        let rows: Vec<Row> = olds
            .iter()
            .zip(&news)
            .enumerate()
            .map(|(index, (old, new))| Row {
                excluded: excluded.get(index).copied().unwrap_or(false),
                ..row(old, new)
            })
            .collect();
        let names: Vec<&str> = listed.iter().map(String::as_str).collect();
        let fold = if insensitive { Fold::Insensitive } else { Fold::Exact };
        let result = states(&rows, &names, fold);
        prop_assert_eq!(result.len(), rows.len());
    }
}
