use super::*;

fn first_tab_rows(state: &AppState, rows_per_pane: usize) -> Option<RowsState> {
    state.to_session(rows_per_pane).windows[0].panes[0].tabs[0]
        .rows
        .clone()
}

fn scrolled(len: usize, scroll: u64) -> AppState {
    let mut state = home();
    load(&mut state, files(len));
    state.active_tab_mut().set_scroll(scroll);
    state
}

#[test]
fn tab_without_listing_saves_no_rows() {
    assert_eq!(first_tab_rows(&home(), 100), None);
}

#[test]
fn rows_are_the_slice_starting_at_scroll() {
    let rows = first_tab_rows(&scrolled(300, 50), 100).unwrap();
    assert_eq!((rows.first, rows.entries), (50, entries(50..150)));
}

#[test]
fn rows_stop_at_the_end_of_the_listing() {
    let rows = first_tab_rows(&scrolled(120, 100), 100).unwrap();
    assert_eq!((rows.first, rows.entries), (100, entries(100..120)));
}

#[test]
fn rows_save_the_listing_total() {
    let rows = first_tab_rows(&scrolled(300, 50), 100).unwrap();
    assert_eq!(rows.total, 300);
}

#[test]
fn rows_follow_rows_per_pane() {
    let rows = first_tab_rows(&scrolled(300, 0), 7).unwrap();
    assert_eq!(rows.entries, entries(0..7));
}

#[test]
fn empty_listing_saves_empty_rows() {
    let rows = first_tab_rows(&scrolled(0, 0), 100);
    let expected = RowsState {
        first: 0,
        total: 0,
        entries: Vec::new(),
    };
    assert_eq!(rows, Some(expected));
}

#[test]
fn inactive_tabs_with_a_listing_save_rows_too() {
    let mut state = home();
    run(&mut state, Command::PaneOpenTab { location: None });
    load_tab(&mut state, TabId::new(3), "/home", files(4));
    assert_eq!(
        first_tab_rows(&state, 100).map(|r| r.entries),
        Some(entries(0..4))
    );
}
