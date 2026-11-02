use super::loc;
use crate::{AppState, PaneId, StateError, TabId, WindowId};

fn home() -> AppState {
    AppState::new(loc("/home"))
}

fn w(raw: u64) -> WindowId {
    WindowId::new(raw)
}

#[test]
fn open_window_returns_a_fresh_id() {
    assert_eq!(home().open_window(loc("/b")), w(4));
}

#[test]
fn open_window_focuses_the_new_window() {
    let mut state = home();
    state.open_window(loc("/b"));
    assert_eq!(state.focused_window(), w(4));
}

#[test]
fn open_window_has_one_pane_with_the_next_id() {
    let mut state = home();
    state.open_window(loc("/b"));
    assert_eq!(
        state.window(w(4)).unwrap().layout().panes(),
        vec![PaneId::new(5)]
    );
}

#[test]
fn open_window_has_one_tab_at_the_location() {
    let mut state = home();
    state.open_window(loc("/b"));
    let tabs = state.pane(PaneId::new(5)).unwrap().tabs();
    assert_eq!(
        tabs.iter()
            .map(|t| (t.id(), t.location()))
            .collect::<Vec<_>>(),
        vec![(TabId::new(6), &loc("/b"))]
    );
}

#[test]
fn focused_pane_follows_the_focused_window() {
    let mut state = home();
    state.open_window(loc("/b"));
    assert_eq!(state.focused_pane(), PaneId::new(5));
}

#[test]
fn window_ids_are_ascending() {
    let mut state = home();
    state.open_window(loc("/b"));
    state.open_window(loc("/c"));
    state.focus_window(w(1)).unwrap();
    assert_eq!(state.window_ids(), vec![w(1), w(4), w(7)]);
}

#[test]
fn pane_finds_panes_in_any_window() {
    let mut state = home();
    state.open_window(loc("/b"));
    assert_eq!(
        state.pane(PaneId::new(2)).map(|p| p.id()),
        Some(PaneId::new(2))
    );
}

#[test]
fn focus_window_changes_the_focused_window() {
    let mut state = home();
    state.open_window(loc("/b"));
    state.focus_window(w(1)).unwrap();
    assert_eq!(state.active_tab().id(), TabId::new(3));
}

#[test]
fn focus_window_unknown_is_unknown_window() {
    assert_eq!(
        home().focus_window(w(9)),
        Err(StateError::UnknownWindow(w(9)))
    );
}

#[test]
fn close_window_removes_it() {
    let mut state = home();
    state.open_window(loc("/b"));
    state.close_window(w(4)).unwrap();
    assert_eq!(state.window_ids(), vec![w(1)]);
}

#[test]
fn close_window_removes_its_panes() {
    let mut state = home();
    state.open_window(loc("/b"));
    state.close_window(w(4)).unwrap();
    assert!(state.pane(PaneId::new(5)).is_none());
}

#[test]
fn closing_the_focused_window_focuses_the_lowest_remaining_id() {
    let mut state = home();
    state.open_window(loc("/b"));
    state.open_window(loc("/c"));
    state.close_window(w(7)).unwrap();
    assert_eq!(state.focused_window(), w(1));
}

#[test]
fn closing_another_window_keeps_the_focus() {
    let mut state = home();
    state.open_window(loc("/b"));
    state.open_window(loc("/c"));
    state.close_window(w(1)).unwrap();
    assert_eq!(state.focused_window(), w(7));
}

#[test]
fn closing_the_only_window_is_last_window() {
    assert_eq!(home().close_window(w(1)), Err(StateError::LastWindow));
}

#[test]
fn closing_an_unknown_window_is_unknown_window() {
    let mut state = home();
    state.open_window(loc("/b"));
    assert_eq!(
        state.close_window(w(9)),
        Err(StateError::UnknownWindow(w(9)))
    );
}

#[test]
fn ids_are_not_reused_after_closing() {
    let mut state = home();
    state.open_window(loc("/b"));
    state.close_window(w(4)).unwrap();
    assert_eq!(state.open_window(loc("/c")), w(7));
}
