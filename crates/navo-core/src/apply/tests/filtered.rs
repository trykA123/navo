use super::{at_work, cursor, entry, loc, marked, mixed, move_cursor, navigate, run};
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
    run(&mut state, &Command::SelectionAll);
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
fn a_filtered_listing_applies_to_rows_shown_while_a_load_is_pending() {
    let mut state = at_work(mixed());
    run(&mut state, &Command::PaneRefresh);
    filter(&mut state, "/work", only(&["a.rs"]));
    assert_eq!(names(&state), ["a.rs"]);
    assert!(state.active_tab().pending_request().is_some());
}

#[test]
fn a_filtered_listing_before_any_rows_arrive_is_ignored() {
    let mut state = at_work(mixed());
    navigate(&mut state, "/work/docs");
    filter(&mut state, "/work/docs", only(&["a.rs"]));
    assert!(state.active_tab().listing().is_none());
}

#[test]
fn widening_the_filter_keeps_the_cursor_on_screen() {
    let all = super::files(500);
    let mut state = at_work(all.clone());
    super::page_rows(&mut state, 20);
    let picked = Listing::new(
        [10_u64, 300, 450]
            .iter()
            .filter_map(|row| all.get(*row).cloned())
            .collect(),
    );
    filter(&mut state, "/work", picked);
    move_cursor(&mut state, CursorMove::Row(2));
    filter(&mut state, "/work", all);
    let view = state.active_tab().view();
    assert_eq!(view.cursor, 450);
    assert!(view.scroll <= 450 && 450 < view.scroll + 20);
}

fn hide(state: &mut AppState, listing: Listing) {
    let tab = state.active_tab().id();
    let event = Event::ListingRefiltered {
        tab,
        location: loc("/work"),
        listing,
    };
    assert_eq!(apply_event(state, event), vec![]);
}

#[test]
fn hiding_the_cursor_entry_moves_to_the_next_shown_entry() {
    let mut state = at_work(mixed());
    let all = names(&state);
    move_cursor(&mut state, CursorMove::Row(2));
    let kept: Vec<&str> = all
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 2)
        .map(|(_, name)| name.as_str())
        .collect();
    hide(&mut state, only(&kept));
    assert_eq!(names(&state)[cursor(&state) as usize], all[3]);
}

#[test]
fn hiding_the_last_entries_moves_back_to_the_previous_shown_entry() {
    let mut state = at_work(mixed());
    let all = names(&state);
    let last = all.len() - 1;
    move_cursor(&mut state, CursorMove::Row(last as u64));
    let kept: Vec<&str> = all[..last - 1].iter().map(String::as_str).collect();
    hide(&mut state, only(&kept));
    assert_eq!(names(&state)[cursor(&state) as usize], all[last - 2]);
}

#[test]
fn a_still_shown_cursor_entry_keeps_the_cursor_after_hiding() {
    let mut state = at_work(mixed());
    let all = names(&state);
    move_cursor(&mut state, CursorMove::Row(4));
    let kept: Vec<&str> = all
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 1)
        .map(|(_, name)| name.as_str())
        .collect();
    hide(&mut state, only(&kept));
    assert_eq!(names(&state)[cursor(&state) as usize], all[4]);
}
