use super::{at_work, cursor, fail, files, home, loc, marked, mixed, move_cursor, navigate, run};
use crate::{AppState, Command, CommandError, CursorMove, Effect, Listing, PatternError};

fn at_row(listing: Listing, row: u64) -> AppState {
    let mut state = at_work(listing);
    move_cursor(&mut state, CursorMove::Row(row));
    state
}

fn unloaded() -> AppState {
    let mut state = home();
    navigate(&mut state, "/work");
    state
}

fn add(pattern: &str) -> Command {
    Command::SelectionAddPattern {
        pattern: pattern.to_owned(),
    }
}

fn remove(pattern: &str) -> Command {
    Command::SelectionRemovePattern {
        pattern: pattern.to_owned(),
    }
}

#[test]
fn toggle_marks_the_cursor_row() {
    let mut state = at_row(files(5), 2);
    run(&mut state, Command::SelectionToggle);
    assert_eq!(marked(&state), vec![2]);
}

#[test]
fn toggle_twice_unmarks_the_cursor_row() {
    let mut state = at_row(files(5), 2);
    run(&mut state, Command::SelectionToggle);
    run(&mut state, Command::SelectionToggle);
    assert_eq!(marked(&state), Vec::<u64>::new());
}

#[test]
fn toggle_keeps_the_cursor() {
    let mut state = at_row(files(5), 2);
    run(&mut state, Command::SelectionToggle);
    assert_eq!(cursor(&state), 2);
}

#[test]
fn toggle_on_a_file_has_no_effects() {
    let mut state = at_row(mixed(), 1);
    assert_eq!(run(&mut state, Command::SelectionToggle), vec![]);
}

#[test]
fn toggle_marking_a_folder_measures_it() {
    let mut state = at_row(mixed(), 3);
    let tab = state.active_tab().id();
    assert_eq!(
        run(&mut state, Command::SelectionToggle),
        vec![Effect::MeasureFolder {
            tab,
            location: loc("/work"),
            name: "src.rs".to_owned(),
        }]
    );
}

#[test]
fn toggle_unmarking_a_folder_does_not_measure_it() {
    let mut state = at_row(mixed(), 3);
    run(&mut state, Command::SelectionToggle);
    assert_eq!(run(&mut state, Command::SelectionToggle), vec![]);
}

#[test]
fn toggle_without_a_listing_fails() {
    assert_eq!(
        fail(&mut unloaded(), Command::SelectionToggle),
        CommandError::NoListing
    );
}

#[test]
fn toggle_on_an_empty_listing_fails() {
    assert_eq!(
        fail(&mut at_work(files(0)), Command::SelectionToggle),
        CommandError::EmptyListing
    );
}

#[test]
fn toggle_down_marks_the_cursor_row() {
    let mut state = at_row(files(5), 2);
    run(&mut state, Command::SelectionToggleDown);
    assert_eq!(marked(&state), vec![2]);
}

#[test]
fn toggle_down_moves_the_cursor_down() {
    let mut state = at_row(files(5), 2);
    run(&mut state, Command::SelectionToggleDown);
    assert_eq!(cursor(&state), 3);
}

#[test]
fn toggle_down_on_the_last_row_keeps_the_cursor() {
    let mut state = at_row(files(5), 4);
    run(&mut state, Command::SelectionToggleDown);
    assert_eq!((marked(&state), cursor(&state)), (vec![4], 4));
}

#[test]
fn toggle_down_on_a_folder_does_not_measure_it() {
    let mut state = at_row(mixed(), 3);
    assert_eq!(run(&mut state, Command::SelectionToggleDown), vec![]);
}

#[test]
fn toggle_down_without_a_listing_fails() {
    assert_eq!(
        fail(&mut unloaded(), Command::SelectionToggleDown),
        CommandError::NoListing
    );
}

#[test]
fn toggle_down_on_an_empty_listing_fails() {
    assert_eq!(
        fail(&mut at_work(files(0)), Command::SelectionToggleDown),
        CommandError::EmptyListing
    );
}

#[test]
fn range_marks_from_the_anchor_to_the_row() {
    let mut state = at_row(files(10), 2);
    run(&mut state, Command::SelectionToggle);
    run(&mut state, Command::SelectionRange { to: 5 });
    assert_eq!(marked(&state), vec![2, 3, 4, 5]);
}

#[test]
fn range_moves_the_cursor_to_the_row() {
    let mut state = at_row(files(10), 2);
    run(&mut state, Command::SelectionToggle);
    run(&mut state, Command::SelectionRange { to: 5 });
    assert_eq!(cursor(&state), 5);
}

#[test]
fn range_past_the_end_clamps_the_cursor() {
    let mut state = at_row(files(10), 2);
    run(&mut state, Command::SelectionToggle);
    run(&mut state, Command::SelectionRange { to: 40 });
    assert_eq!((marked(&state).len(), cursor(&state)), (8, 9));
}

#[test]
fn range_has_no_effects() {
    let mut state = at_row(mixed(), 0);
    assert_eq!(run(&mut state, Command::SelectionRange { to: 4 }), vec![]);
}

#[test]
fn range_without_a_listing_fails() {
    assert_eq!(
        fail(&mut unloaded(), Command::SelectionRange { to: 1 }),
        CommandError::NoListing
    );
}

#[test]
fn range_on_an_empty_listing_fails() {
    assert_eq!(
        fail(&mut at_work(files(0)), Command::SelectionRange { to: 0 }),
        CommandError::EmptyListing
    );
}

#[test]
fn add_pattern_marks_matching_files_only() {
    let mut state = at_work(mixed());
    run(&mut state, add("*.rs"));
    assert_eq!(marked(&state), vec![1, 4]);
}

#[test]
fn add_pattern_has_no_effects() {
    let mut state = at_work(mixed());
    assert_eq!(run(&mut state, add("*.rs")), vec![]);
}

#[test]
fn add_pattern_with_no_items_fails() {
    assert_eq!(
        fail(&mut at_work(mixed()), add(" ; ")),
        CommandError::Pattern(PatternError::Empty)
    );
}

#[test]
fn add_pattern_without_a_listing_fails() {
    assert_eq!(fail(&mut unloaded(), add("*.rs")), CommandError::NoListing);
}

#[test]
fn remove_pattern_unmarks_matching_files_only() {
    let mut state = at_row(mixed(), 3);
    run(&mut state, Command::SelectionToggle);
    run(&mut state, Command::SelectionAll);
    run(&mut state, remove("*.rs"));
    assert_eq!(marked(&state), vec![2, 3]);
}

#[test]
fn remove_pattern_has_no_effects() {
    let mut state = at_work(mixed());
    assert_eq!(run(&mut state, remove("*.rs")), vec![]);
}

#[test]
fn remove_pattern_with_no_items_fails() {
    assert_eq!(
        fail(&mut at_work(mixed()), remove("")),
        CommandError::Pattern(PatternError::Empty)
    );
}

#[test]
fn remove_pattern_without_a_listing_fails() {
    assert_eq!(
        fail(&mut unloaded(), remove("*.rs")),
        CommandError::NoListing
    );
}

#[test]
fn invert_flips_files_only() {
    let mut state = at_row(mixed(), 1);
    run(&mut state, Command::SelectionToggle);
    run(&mut state, Command::SelectionInvert);
    assert_eq!(marked(&state), vec![2, 4]);
}

#[test]
fn invert_has_no_effects() {
    let mut state = at_work(mixed());
    assert_eq!(run(&mut state, Command::SelectionInvert), vec![]);
}

#[test]
fn invert_without_a_listing_fails() {
    assert_eq!(
        fail(&mut unloaded(), Command::SelectionInvert),
        CommandError::NoListing
    );
}

#[test]
fn all_marks_files_only() {
    let mut state = at_work(mixed());
    run(&mut state, Command::SelectionAll);
    assert_eq!(marked(&state), vec![1, 2, 4]);
}

#[test]
fn all_has_no_effects() {
    let mut state = at_work(mixed());
    assert_eq!(run(&mut state, Command::SelectionAll), vec![]);
}

#[test]
fn all_without_a_listing_fails() {
    assert_eq!(
        fail(&mut unloaded(), Command::SelectionAll),
        CommandError::NoListing
    );
}

#[test]
fn clear_unmarks_every_row() {
    let mut state = at_row(mixed(), 0);
    run(&mut state, Command::SelectionToggle);
    run(&mut state, Command::SelectionRange { to: 4 });
    run(&mut state, Command::SelectionClear);
    assert_eq!(marked(&state), Vec::<u64>::new());
}

#[test]
fn clear_has_no_effects() {
    let mut state = at_work(mixed());
    assert_eq!(run(&mut state, Command::SelectionClear), vec![]);
}

#[test]
fn clear_without_a_listing_succeeds() {
    assert_eq!(run(&mut unloaded(), Command::SelectionClear), vec![]);
}
