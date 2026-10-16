use super::{
    at_work, cursor, files, find_tab, home, load, loc, marked, move_cursor, navigate, page_rows,
    run,
};
use crate::{
    Command, CursorMove, Direction, Effect, Event, PaneId, TabId, apply_event, list_unloaded,
};

fn loaded(tab: TabId, location: &str, request: u64, len: usize) -> Event {
    Event::ListingLoaded {
        tab,
        location: loc(location),
        request,
        listing: files(len),
    }
}

fn request_of(effects: &[Effect]) -> u64 {
    effects
        .iter()
        .find_map(|e| match e {
            Effect::ListFolder { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap_or(0)
}

#[test]
fn listing_loaded_sets_the_listing() {
    let state = at_work(files(4));
    assert_eq!(state.active_tab().listing(), Some(&files(4)));
}

#[test]
fn a_refresh_keeps_the_marks_by_name() {
    let mut state = at_work(files(4));
    move_cursor(&mut state, CursorMove::Row(2));
    run(&mut state, Command::SelectionToggle);
    load(&mut state, files(6));
    assert_eq!(marked(&state), vec![2]);
}

#[test]
fn a_refresh_drops_marks_of_names_that_are_gone() {
    let mut state = at_work(files(4));
    move_cursor(&mut state, CursorMove::Row(3));
    run(&mut state, Command::SelectionToggle);
    load(&mut state, files(2));
    assert_eq!(marked(&state), Vec::<u64>::new());
}

#[test]
fn an_older_answer_for_the_same_folder_is_ignored() {
    let mut state = home();
    let tab = state.active_tab().id();
    let first = request_of(&navigate(&mut state, "/work"));
    navigate(&mut state, "/other");
    run(&mut state, Command::PaneBack);
    let before = state.clone();
    let _ = apply_event(&mut state, loaded(tab, "/work", first, 3));
    assert_eq!(state, before);
}

#[test]
fn requests_count_up() {
    let mut state = home();
    let first = request_of(&navigate(&mut state, "/a"));
    let second = request_of(&navigate(&mut state, "/b"));
    assert_eq!((first, second), (1, 2));
}

#[test]
fn list_unloaded_asks_for_every_active_tab_without_a_listing() {
    let mut state = home();
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let effects = list_unloaded(&mut state);
    assert_eq!(effects.len(), 1);
}

#[test]
fn list_unloaded_skips_loaded_and_pending_tabs() {
    let mut state = at_work(files(3));
    assert_eq!(list_unloaded(&mut state), vec![]);
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
    let request = request_of(&navigate(&mut state, "/work"));
    navigate(&mut state, "/other");
    let before = state.clone();
    let _ = apply_event(&mut state, loaded(tab, "/work", request, 3));
    assert_eq!(state, before);
}

#[test]
fn listing_loaded_for_a_closed_tab_is_ignored() {
    let mut state = home();
    let request = request_of(&run(&mut state, Command::PaneOpenTab { location: None }));
    let closed = state.active_tab().id();
    run(&mut state, Command::PaneCloseTab { tab: None });
    let before = state.clone();
    let _ = apply_event(&mut state, loaded(closed, "/home", request, 3));
    assert_eq!(state, before);
}

#[test]
fn listing_loaded_reaches_a_tab_in_another_pane() {
    let mut state = home();
    let first = state.active_tab().id();
    let request = request_of(&list_unloaded(&mut state));
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let _ = apply_event(&mut state, loaded(first, "/home", request, 3));
    assert_eq!(find_tab(&state, first).unwrap().listing(), Some(&files(3)));
}

#[test]
fn listing_loaded_for_an_inactive_tab_leaves_the_active_tab_alone() {
    let mut state = home();
    let first = state.active_tab().id();
    let request = request_of(&list_unloaded(&mut state));
    run(&mut state, Command::PaneOpenTab { location: None });
    let _ = apply_event(&mut state, loaded(first, "/home", request, 3));
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
    let _ = apply_event(&mut state, loaded(tab, "/elsewhere", 999, 2));
    assert_eq!(marked(&state), vec![0]);
}

#[test]
fn listing_loaded_produces_no_effects() {
    let mut state = home();
    let tab = state.active_tab().id();
    let request = request_of(&list_unloaded(&mut state));
    assert_eq!(
        apply_event(&mut state, loaded(tab, "/home", request, 3)),
        vec![]
    );
}

#[test]
fn list_missing_also_relists_pending_tabs_at_start_up() {
    let mut state = home();
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    assert_eq!(crate::list_missing(&mut state).len(), 2);
}

#[test]
fn list_missing_skips_loaded_tabs() {
    let mut state = at_work(files(3));
    assert_eq!(crate::list_missing(&mut state), vec![]);
}

#[test]
fn list_active_tabs_relists_every_pane_after_an_operation() {
    let mut state = at_work(files(3));
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    assert_eq!(crate::list_active_tabs(&mut state).len(), 2);
}

#[test]
fn relist_tab_asks_for_that_tab_again_with_a_new_request() {
    let mut state = at_work(files(3));
    let tab = state.active_tab().id();
    let effects = crate::relist_tab(&mut state, tab);
    let [
        Effect::ListFolder {
            tab: listed,
            request,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("one listing is asked for");
    };
    assert_eq!(*listed, tab);
    assert_eq!(state.active_tab().pending_request(), Some(*request));
    assert!(state.active_tab().listing().is_some());
}
