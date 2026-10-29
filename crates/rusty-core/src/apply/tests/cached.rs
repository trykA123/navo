use super::{at_work, cursor, entry, files, load, marked, move_cursor, navigate, page_rows, run};
use crate::{AppState, Command, CursorMove, Event, Listing, apply_event};

fn show_cached(state: &mut AppState, listing: Listing) {
    let tab = state.active_tab();
    let event = Event::ListingCached {
        tab: tab.id(),
        location: tab.location().clone(),
        request: tab.pending_request().unwrap_or(0),
        listing,
    };
    let _ = apply_event(state, event);
}

fn named(names: &[&str]) -> Listing {
    Listing::new(names.iter().map(|name| entry(name, false)).collect())
}

fn cursor_name(state: &AppState) -> Option<String> {
    let tab = state.active_tab();
    tab.listing()
        .and_then(|listing| listing.get(tab.view().cursor))
        .map(|entry| entry.name.clone())
}

fn away_and_back(state: &mut AppState) {
    navigate(state, "/other");
    load(state, files(1));
    run(state, &Command::PaneBack);
}

#[test]
fn cached_rows_show_while_the_fresh_listing_is_still_pending() {
    let mut state = at_work(files(5));
    away_and_back(&mut state);
    show_cached(&mut state, files(5));
    let tab = state.active_tab();
    assert_eq!(tab.listing(), Some(&files(5)));
    assert!(tab.pending_request().is_some());
}

#[test]
fn a_cached_listing_for_a_finished_request_is_ignored() {
    let mut state = at_work(files(5));
    show_cached(&mut state, files(2));
    assert_eq!(state.active_tab().listing(), Some(&files(5)));
}

#[test]
fn back_puts_the_cursor_on_the_remembered_name() {
    let mut state = at_work(named(&["a", "b", "c", "d"]));
    move_cursor(&mut state, CursorMove::Row(2));
    away_and_back(&mut state);
    show_cached(&mut state, named(&["0", "a", "b", "c", "d"]));
    assert_eq!(cursor_name(&state).as_deref(), Some("c"));
}

#[test]
fn the_fresh_listing_keeps_the_cursor_on_its_name() {
    let mut state = at_work(named(&["a", "b", "c", "d"]));
    move_cursor(&mut state, CursorMove::Row(2));
    away_and_back(&mut state);
    show_cached(&mut state, named(&["a", "b", "c", "d"]));
    load(&mut state, named(&["0", "1", "a", "c", "d", "e"]));
    assert_eq!(cursor_name(&state).as_deref(), Some("c"));
    assert!(state.active_tab().pending_request().is_none());
}

#[test]
fn a_vanished_cursor_name_moves_to_the_next_row() {
    let mut state = at_work(named(&["a", "b", "c", "d"]));
    move_cursor(&mut state, CursorMove::Row(1));
    load(&mut state, named(&["0", "a", "c", "d"]));
    assert_eq!(cursor_name(&state).as_deref(), Some("c"));
}

#[test]
fn a_vanished_last_row_moves_the_cursor_to_the_new_last_row() {
    let mut state = at_work(named(&["a", "b", "c"]));
    move_cursor(&mut state, CursorMove::Row(2));
    load(&mut state, named(&["a", "b"]));
    assert_eq!(cursor_name(&state).as_deref(), Some("b"));
}

#[test]
fn the_cursor_keeps_its_place_on_screen_when_rows_appear_above() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 20);
    move_cursor(&mut state, CursorMove::Row(50));
    let before = state.active_tab().view();
    let mut grown: Vec<_> = (0..10).map(|i| entry(&format!("new{i}"), false)).collect();
    grown.extend(files(100).entries().iter().cloned());
    load(&mut state, Listing::new(grown));
    let after = state.active_tab().view();
    assert_eq!(after.cursor, before.cursor + 10);
    assert_eq!(after.cursor - after.scroll, before.cursor - before.scroll);
}

#[test]
fn marks_stay_on_their_names_through_the_fresh_listing() {
    let mut state = at_work(named(&["a", "b", "c"]));
    move_cursor(&mut state, CursorMove::Row(2));
    run(&mut state, &Command::SelectionToggle);
    away_and_back(&mut state);
    show_cached(&mut state, named(&["a", "b", "c"]));
    move_cursor(&mut state, CursorMove::Row(1));
    run(&mut state, &Command::SelectionToggle);
    load(&mut state, named(&["0", "a", "b", "c"]));
    assert_eq!(marked(&state), [2]);
    assert_eq!(cursor(&state), 2);
}
