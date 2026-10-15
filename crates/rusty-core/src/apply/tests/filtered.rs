use super::{at_work, cursor, entry, loc, marked, mixed, move_cursor, run};
use crate::{AppState, Command, CursorMove, Event, Listing, apply_event};

fn names(state: &AppState) -> Vec<String> {
    state
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

fn only(names: &[&str]) -> Listing {
    let all = mixed();
    Listing::new(
        all.entries()
            .iter()
            .filter(|e| names.contains(&e.name.as_str()))
            .cloned()
            .collect(),
    )
}

fn filter(state: &mut AppState, location: &str, listing: Listing) {
    let tab = state.active_tab().id();
    let event = Event::ListingFiltered {
        tab,
        location: loc(location),
        listing,
    };
    assert_eq!(apply_event(state, event), vec![]);
}

#[test]
fn a_filtered_listing_replaces_the_rows() {
    let mut state = at_work(mixed());
    filter(&mut state, "/work", only(&["a.rs", "c.rs"]));
    assert_eq!(names(&state), ["a.rs", "c.rs"]);
}

#[test]
fn the_cursor_stays_on_its_entry_when_it_is_still_shown() {
    let mut state = at_work(mixed());
    move_cursor(&mut state, CursorMove::Row(4));
    filter(&mut state, "/work", only(&["a.rs", "c.rs"]));
    assert_eq!(cursor(&state), 1);
}

#[test]
fn the_cursor_goes_to_the_first_row_when_its_entry_is_hidden() {
    let mut state = at_work(mixed());
    move_cursor(&mut state, CursorMove::Row(2));
    filter(&mut state, "/work", only(&["a.rs", "c.rs"]));
    assert_eq!(cursor(&state), 0);
}

#[test]
fn marks_follow_their_names_and_hidden_ones_drop() {
    let mut state = at_work(mixed());
    run(&mut state, Command::SelectionAll);
    filter(&mut state, "/work", only(&["a.rs", "c.rs"]));
    assert_eq!(marked(&state), [0, 1]);
}

#[test]
fn a_filtered_listing_for_another_folder_is_ignored() {
    let mut state = at_work(mixed());
    filter(
        &mut state,
        "/elsewhere",
        Listing::new(vec![entry("x", false)]),
    );
    assert_eq!(names(&state).len(), 5);
}

#[test]
fn a_filtered_listing_while_a_load_is_pending_is_ignored() {
    let mut state = at_work(mixed());
    run(&mut state, Command::PaneRefresh);
    filter(&mut state, "/work", only(&["a.rs"]));
    assert_eq!(names(&state).len(), 5);
}
