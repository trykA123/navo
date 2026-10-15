use super::{
    at_work, cursor, files, find_tab, home, load, loc, marked, move_cursor, navigate, page_rows,
    run,
};
use crate::{Command, CursorMove, Direction, Event, PaneId, Selection, TabId, apply_event};

fn loaded(tab: TabId, location: &str, len: usize) -> Event {
    Event::ListingLoaded {
        tab,
        location: loc(location),
        listing: files(len),
    }
}

#[test]
fn listing_loaded_sets_the_listing() {
    let state = at_work(files(4));
    assert_eq!(state.active_tab().listing(), Some(&files(4)));
}

#[test]
fn listing_loaded_resets_the_selection() {
    let mut state = at_work(files(4));
    run(&mut state, Command::SelectionToggle);
    load(&mut state, files(6));
    assert_eq!(state.active_tab().selection(), &Selection::new(6));
}

#[test]
fn listing_loaded_clamps_the_remembered_cursor() {
    let mut state = at_work(files(10));
    move_cursor(&mut state, CursorMove::Row(8));
    navigate(&mut state, "/other");
    run(&mut state, Command::PaneBack);
    load(&mut state, files(3));
    assert_eq!(cursor(&state), 2);
}

#[test]
fn listing_loaded_after_navigating_away_is_ignored() {
    let mut state = home();
    let tab = state.active_tab().id();
    navigate(&mut state, "/work");
    navigate(&mut state, "/other");
    let before = state.clone();
    let _ = apply_event(&mut state, loaded(tab, "/work", 3));
    assert_eq!(state, before);
}

#[test]
fn listing_loaded_for_a_closed_tab_is_ignored() {
    let mut state = home();
    run(&mut state, Command::PaneOpenTab { location: None });
    let closed = state.active_tab().id();
    run(&mut state, Command::PaneCloseTab { tab: None });
    let before = state.clone();
    let _ = apply_event(&mut state, loaded(closed, "/home", 3));
    assert_eq!(state, before);
}

#[test]
fn listing_loaded_reaches_a_tab_in_another_pane() {
    let mut state = home();
    let first = state.active_tab().id();
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let _ = apply_event(&mut state, loaded(first, "/home", 3));
    assert_eq!(find_tab(&state, first).unwrap().listing(), Some(&files(3)));
}

#[test]
fn listing_loaded_for_an_inactive_tab_leaves_the_active_tab_alone() {
    let mut state = home();
    let first = state.active_tab().id();
    run(&mut state, Command::PaneOpenTab { location: None });
    let _ = apply_event(&mut state, loaded(first, "/home", 3));
    assert_eq!(state.active_tab().listing(), None);
}

#[test]
fn page_rows_changed_sets_the_page_down_step() {
    let mut state = at_work(files(50));
    page_rows(&mut state, 10);
    move_cursor(&mut state, CursorMove::PageDown);
    assert_eq!(cursor(&state), 9);
}

#[test]
fn page_rows_changed_sets_the_page_up_step() {
    let mut state = at_work(files(50));
    page_rows(&mut state, 10);
    move_cursor(&mut state, CursorMove::Row(30));
    move_cursor(&mut state, CursorMove::PageUp);
    assert_eq!(cursor(&state), 21);
}

#[test]
fn page_down_with_one_row_pages_still_moves_one_row() {
    let mut state = at_work(files(50));
    page_rows(&mut state, 1);
    move_cursor(&mut state, CursorMove::PageDown);
    assert_eq!(cursor(&state), 1);
}

#[test]
fn page_rows_changed_to_zero_counts_as_one() {
    let mut state = at_work(files(50));
    page_rows(&mut state, 0);
    move_cursor(&mut state, CursorMove::PageDown);
    assert_eq!(cursor(&state), 1);
}

#[test]
fn page_rows_belong_to_their_pane() {
    let mut state = at_work(files(50));
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    page_rows(&mut state, 5);
    run(
        &mut state,
        Command::LayoutFocus {
            direction: Direction::Left,
        },
    );
    move_cursor(&mut state, CursorMove::PageDown);
    assert_eq!(cursor(&state), 19);
}

#[test]
fn page_rows_changed_for_an_unknown_pane_is_ignored() {
    let mut state = at_work(files(50));
    let before = state.clone();
    let _ = apply_event(
        &mut state,
        Event::PageRowsChanged {
            pane: PaneId::new(u64::MAX),
            rows: 5,
        },
    );
    assert_eq!(state, before);
}

#[test]
fn listing_loaded_for_another_location_keeps_the_marks() {
    let mut state = at_work(files(4));
    run(&mut state, Command::SelectionToggle);
    let tab = state.active_tab().id();
    let _ = apply_event(&mut state, loaded(tab, "/elsewhere", 2));
    assert_eq!(marked(&state), vec![0]);
}

#[test]
fn listing_loaded_produces_no_effects() {
    let mut state = home();
    let tab = state.active_tab().id();
    assert_eq!(apply_event(&mut state, loaded(tab, "/home", 3)), vec![]);
}
