use rusty_core::rename::check::{Fold, RowState};

use crate::multi_rename::Form;

use super::names::{items, names, preview};

#[test]
fn counters_continue_globally_by_default_and_restart_per_folder_when_enabled() {
    let mut form = Form::new();
    form.set_name_mask("[C:2]");
    form.rules.counter.start = 5;
    form.rules.counter.step = 2;
    form.toggle_excluded(2);
    let items = items(&[
        ("/one", "a.txt"),
        ("/two", "b.txt"),
        ("/one", "c.txt"),
        ("/two", "d.txt"),
        ("/one", "e.txt"),
    ]);
    assert_eq!(
        names(&preview(&form, &items, Fold::Exact).unwrap()),
        ["05.txt", "07.txt", "c.txt", "09.txt", "11.txt"]
    );
    form.counter_per_folder = true;
    let result = preview(&form, &items, Fold::Exact).unwrap();
    assert_eq!(
        names(&result),
        ["05.txt", "05.txt", "c.txt", "07.txt", "07.txt"]
    );
    assert_eq!(result.rows[2].state, RowState::Excluded);
    assert!(result.can_run());
}

#[test]
fn folder_counter_identity_respects_listing_case_folding() {
    let mut form = Form::new();
    form.set_name_mask("[C]");
    form.counter_per_folder = true;
    let items = items(&[("C:\\One", "a"), ("C:\\one", "b")]);
    assert_eq!(
        names(&preview(&form, &items, Fold::Exact).unwrap()),
        ["1", "1"]
    );
    assert_eq!(
        names(&preview(&form, &items, Fold::Insensitive).unwrap()),
        ["1", "2"]
    );
}

#[test]
fn reordered_rows_change_counter_order_and_keep_exclusion_identity() {
    let mut form = Form::new();
    form.set_name_mask("[N]-[C]");
    form.toggle_excluded(0);
    let items = items(&[("/one", "a"), ("/one", "b"), ("/one", "c")]);
    let order = [2, 0, 1];
    let reordered: Vec<_> = order.iter().map(|&old| items[old].clone()).collect();
    assert!(form.reorder(&order));
    let result = preview(&form, &reordered, Fold::Exact).unwrap();
    assert_eq!(names(&result), ["c-1", "a", "b-2"]);
    assert!(!form.is_excluded(0));
    assert!(form.is_excluded(1));
    assert_eq!(result.rows[1].state, RowState::Excluded);
}

#[test]
fn reordering_keeps_edited_names_with_their_original_rows() {
    let mut form = Form::new();
    form.set_edited_names("edited-a\nedited-b\nedited-c");
    form.toggle_excluded(1);
    let items = items(&[("/one", "a"), ("/one", "b"), ("/one", "c")]);
    let order = [1, 2, 0];
    let reordered: Vec<_> = order.iter().map(|&old| items[old].clone()).collect();
    assert!(form.reorder(&order));
    assert_eq!(
        form.edited_names().unwrap(),
        ["edited-b", "edited-c", "edited-a"]
    );
    assert_eq!(
        names(&preview(&form, &reordered, Fold::Exact).unwrap()),
        ["b", "edited-c", "edited-a"]
    );
    form.toggle_excluded(0);
    assert_eq!(
        names(&preview(&form, &reordered, Fold::Exact).unwrap()),
        ["edited-b", "edited-c", "edited-a"]
    );
}

#[test]
fn invalid_permutations_leave_names_and_exclusions_unchanged() {
    let mut form = Form::new();
    form.set_edited_names("first\nsecond\nthird");
    form.toggle_excluded(1);
    let before = form.clone();
    for order in [&[0, 0, 2][..], &[0, 1, 3], &[1, 0], &[]] {
        assert!(!form.reorder(order));
        assert_eq!(form, before);
    }
}

#[test]
fn excluded_indices_outside_a_permutation_are_rejected_atomically() {
    let mut form = Form::new();
    form.toggle_excluded(3);
    let before = form.clone();
    assert!(!form.reorder(&[1, 0]));
    assert_eq!(form, before);
}
