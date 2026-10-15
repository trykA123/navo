use super::loc;
use crate::{AppState, Direction, LayoutError, PaneId, StateError, TabId, ViewState, WindowId};

fn home() -> AppState {
    AppState::new(loc("/home"))
}

fn w(raw: u64) -> WindowId {
    WindowId::new(raw)
}

fn p(raw: u64) -> PaneId {
    PaneId::new(raw)
}

fn layout_panes(state: &AppState) -> Vec<PaneId> {
    state
        .window(state.focused_window())
        .unwrap()
        .layout()
        .panes()
}

#[test]
fn new_state_has_one_focused_window_with_id_1() {
    let state = home();
    assert_eq!(
        (state.window_ids(), state.focused_window()),
        (vec![w(1)], w(1))
    );
}

#[test]
fn new_state_focuses_pane_2() {
    assert_eq!(home().focused_pane(), p(2));
}

#[test]
fn new_state_window_holds_only_pane_2() {
    let state = home();
    let window = state.window(w(1)).unwrap();
    let ids: Vec<PaneId> = window.panes().iter().map(|pane| pane.id()).collect();
    assert_eq!(
        (window.id(), window.layout().panes(), ids),
        (w(1), vec![p(2)], vec![p(2)])
    );
}

#[test]
fn new_state_has_tab_3_at_home() {
    let state = home();
    let tab = state.active_tab();
    assert_eq!((tab.id(), tab.location()), (TabId::new(3), &loc("/home")));
}

#[test]
fn new_state_pane_has_one_tab() {
    assert_eq!(home().pane(p(2)).unwrap().tabs().len(), 1);
}

#[test]
fn unknown_window_is_none() {
    assert!(home().window(w(9)).is_none());
}

#[test]
fn unknown_pane_is_none() {
    assert!(home().pane(p(9)).is_none());
}

#[test]
fn window_pane_finds_its_own_pane() {
    let state = home();
    assert_eq!(
        state.window(w(1)).unwrap().pane(p(2)).map(|pane| pane.id()),
        Some(p(2))
    );
}

#[test]
fn window_pane_unknown_is_none() {
    assert!(home().window(w(1)).unwrap().pane(p(9)).is_none());
}

#[test]
fn new_tab_takes_fresh_ids() {
    let mut state = home();
    let first = state.new_tab(loc("/a")).id();
    let second = state.new_tab(loc("/b")).id();
    assert_eq!((first, second), (TabId::new(4), TabId::new(5)));
}

#[test]
fn new_tab_is_at_its_location() {
    assert_eq!(home().new_tab(loc("/a")).location(), &loc("/a"));
}

#[test]
fn active_tab_mut_changes_the_active_tab() {
    let mut state = home();
    state.active_tab_mut().navigate(loc("/a"));
    assert_eq!(state.active_tab().location(), &loc("/a"));
}

#[test]
fn focused_pane_mut_opens_tabs_in_the_focused_pane() {
    let mut state = home();
    let tab = state.new_tab(loc("/a"));
    state.focused_pane_mut().open_tab(tab);
    assert_eq!(state.active_tab().id(), TabId::new(4));
}

#[test]
fn split_returns_the_new_pane_with_a_fresh_id() {
    assert_eq!(home().split(Direction::Right), Ok(p(4)));
}

#[test]
fn split_focuses_the_new_pane() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    assert_eq!(state.focused_pane(), p(4));
}

#[test]
fn split_right_puts_the_new_pane_after_the_old_one() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    assert_eq!(layout_panes(&state), vec![p(2), p(4)]);
}

#[test]
fn split_left_puts_the_new_pane_before_the_old_one() {
    let mut state = home();
    state.split(Direction::Left).unwrap();
    assert_eq!(layout_panes(&state), vec![p(4), p(2)]);
}

#[test]
fn split_gives_the_new_pane_one_tab_with_the_next_id() {
    let mut state = home();
    state.split(Direction::Down).unwrap();
    let ids: Vec<TabId> = state
        .pane(p(4))
        .unwrap()
        .tabs()
        .iter()
        .map(|t| t.id())
        .collect();
    assert_eq!(ids, vec![TabId::new(5)]);
}

#[test]
fn split_opens_the_new_tab_at_the_focused_tab_location() {
    let mut state = home();
    state.active_tab_mut().navigate(loc("/a"));
    state.split(Direction::Down).unwrap();
    assert_eq!(state.active_tab().location(), &loc("/a"));
}

#[test]
fn split_gives_the_new_tab_a_fresh_history() {
    let mut state = home();
    state.active_tab_mut().navigate(loc("/a"));
    state.split(Direction::Down).unwrap();
    assert!(state.active_tab().history().back().is_empty());
}

#[test]
fn split_gives_the_new_tab_a_default_view() {
    let mut state = home();
    state.active_tab_mut().set_cursor(5);
    state.split(Direction::Down).unwrap();
    assert_eq!(state.active_tab().view(), ViewState::default());
}

#[test]
fn split_keeps_the_old_pane() {
    let mut state = home();
    state.split(Direction::Down).unwrap();
    assert_eq!(state.pane(p(2)).unwrap().active().id(), TabId::new(3));
}

#[test]
fn close_pane_removes_it_from_the_layout() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    state.close_pane(p(4)).unwrap();
    assert_eq!(layout_panes(&state), vec![p(2)]);
}

#[test]
fn close_pane_removes_the_pane_and_its_tabs() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    state.close_pane(p(4)).unwrap();
    assert!(state.pane(p(4)).is_none());
}

#[test]
fn closing_the_focused_pane_moves_focus_to_a_remaining_pane() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    state.close_pane(p(4)).unwrap();
    assert_eq!(state.focused_pane(), p(2));
}

#[test]
fn closing_the_only_pane_is_a_last_pane_layout_error() {
    assert_eq!(
        home().close_pane(p(2)),
        Err(StateError::Layout(LayoutError::LastPane))
    );
}

#[test]
fn closing_an_unknown_pane_is_unknown_pane() {
    assert_eq!(home().close_pane(p(9)), Err(StateError::UnknownPane(p(9))));
}

#[test]
fn close_pane_works_in_a_window_that_is_not_focused() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    state.open_window(loc("/b"));
    state.close_pane(p(4)).unwrap();
    assert_eq!(state.window(w(1)).unwrap().layout().panes(), vec![p(2)]);
}

#[test]
fn focus_direction_moves_to_the_neighbour() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    assert_eq!(state.focus_direction(Direction::Left), Ok(p(2)));
}

#[test]
fn focus_direction_changes_the_focused_pane() {
    let mut state = home();
    state.split(Direction::Right).unwrap();
    state.focus_direction(Direction::Left).unwrap();
    assert_eq!(state.active_tab().id(), TabId::new(3));
}

#[test]
fn focus_direction_without_a_neighbour_is_a_layout_error() {
    assert_eq!(
        home().focus_direction(Direction::Up),
        Err(StateError::Layout(LayoutError::NoNeighbour(Direction::Up)))
    );
}
