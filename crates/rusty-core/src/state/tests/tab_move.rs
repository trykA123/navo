use super::loc;
use crate::{AppState, Direction, Location, PaneId, StateError, TabId, TabMove};

fn rig() -> (AppState, PaneId, PaneId) {
    let mut state = AppState::new(loc("/a0"));
    let left = state.focused_pane();
    let right = state.split(Direction::Right).unwrap();
    state.focus_number(1).unwrap();
    for name in ["/a1", "/a2"] {
        let tab = state.new_tab(loc(name));
        state.pane_mut(left).unwrap().open_tab(tab);
    }
    state.pane_mut(left).unwrap().activate(0).unwrap();
    state
        .tab_mut(state.pane(right).unwrap().active().id())
        .unwrap()
        .navigate(loc("/b0"));
    (state, left, right)
}

fn folders(state: &AppState, pane: PaneId) -> Vec<Location> {
    let pane = state.pane(pane).unwrap();
    pane.tabs()
        .iter()
        .map(|tab| tab.location().clone())
        .collect()
}

fn ids(state: &AppState, pane: PaneId) -> Vec<TabId> {
    let pane = state.pane(pane).unwrap();
    pane.tabs().iter().map(crate::Tab::id).collect()
}

fn change((from, index): (PaneId, usize), (to, at): (PaneId, usize), copy: bool) -> TabMove {
    TabMove {
        from,
        index,
        to,
        at,
        copy,
    }
}

#[test]
fn move_between_panes_inserts_at_the_index_and_focuses_it() {
    let (mut state, left, right) = rig();
    let moved = ids(&state, left)[1];
    let id = state
        .move_tab(change((left, 1), (right, 0), false))
        .unwrap();
    assert_eq!(id, moved);
    assert_eq!(folders(&state, left), [loc("/a0"), loc("/a2")]);
    assert_eq!(folders(&state, right), [loc("/a1"), loc("/b0")]);
    let pane = state.pane(right).unwrap();
    assert_eq!((pane.active().id(), state.focused_pane()), (moved, right));
}

#[test]
fn moved_tab_keeps_its_history() {
    let (mut state, left, right) = rig();
    let back = state.pane(right).unwrap().active().history().clone();
    state
        .move_tab(change((right, 0), (left, 3), false))
        .unwrap();
    let pane = state.pane(left).unwrap();
    assert_eq!(pane.active_index(), 3);
    assert_eq!(pane.active().history(), &back);
}

#[test]
fn moving_out_the_active_tab_activates_a_neighbour() {
    let (mut state, left, right) = rig();
    state
        .move_tab(change((left, 0), (right, 9), false))
        .unwrap();
    let pane = state.pane(left).unwrap();
    assert_eq!((pane.tabs().len(), pane.active_index()), (2, 0));
    assert_eq!(folders(&state, right), [loc("/b0"), loc("/a0")]);
}

#[test]
fn reorder_within_a_pane_moves_right_and_left() {
    let (mut state, left, _) = rig();
    let before = ids(&state, left);
    state.move_tab(change((left, 0), (left, 3), false)).unwrap();
    assert_eq!(ids(&state, left), [before[1], before[2], before[0]]);
    assert_eq!(state.pane(left).unwrap().active_index(), 2);
    state.move_tab(change((left, 2), (left, 0), false)).unwrap();
    assert_eq!(ids(&state, left), before);
    assert_eq!(state.pane(left).unwrap().active_index(), 0);
}

#[test]
fn reorder_onto_its_own_place_changes_nothing() {
    let (mut state, left, _) = rig();
    let before = ids(&state, left);
    for at in [1, 2] {
        state
            .move_tab(change((left, 1), (left, at), false))
            .unwrap();
        assert_eq!(ids(&state, left), before);
    }
}

#[test]
fn copy_keeps_the_source_and_adds_a_new_tab_with_its_history() {
    let (mut state, left, right) = rig();
    let source = state.pane(right).unwrap().active().clone();
    let id = state.move_tab(change((right, 0), (left, 1), true)).unwrap();
    assert_ne!(id, source.id());
    assert_eq!(ids(&state, right), [source.id()]);
    let pane = state.pane(left).unwrap();
    let copy = pane.active();
    assert_eq!((pane.active_index(), copy.id()), (1, id));
    assert_eq!(copy.location(), source.location());
    assert_eq!(copy.history(), source.history());
    assert!(state.tab(id).is_some_and(|tab| tab.listing().is_none()));
    assert_eq!(state.focused_pane(), left);
}

#[test]
fn copy_ids_never_collide_with_later_tabs() {
    let (mut state, left, right) = rig();
    let id = state.move_tab(change((left, 0), (right, 0), true)).unwrap();
    let next = state.new_tab(loc("/n")).id();
    assert!(next.get() > id.get());
}

#[test]
fn copy_of_search_results_opens_their_folder() {
    let (mut state, left, right) = rig();
    let results = state.open_results_tab(left, loc("/a0")).unwrap();
    let index = ids(&state, left).iter().position(|id| *id == results);
    let id = state
        .move_tab(change((left, index.unwrap()), (right, 0), true))
        .unwrap();
    assert_eq!(state.tab(id).unwrap().location(), &loc("/a0"));
    assert!(state.tab(results).is_some());
}

#[test]
fn moving_the_last_tab_leaves_a_fresh_tab_at_the_same_folder() {
    let (mut state, left, right) = rig();
    let moved = ids(&state, right)[0];
    state
        .move_tab(change((right, 0), (left, 0), false))
        .unwrap();
    let pane = state.pane(right).unwrap();
    assert_eq!(pane.tabs().len(), 1);
    assert_ne!(pane.active().id(), moved);
    assert_eq!(pane.active().location(), &loc("/b0"));
    assert!(pane.active().history().back().is_empty());
    assert_eq!(state.pane(left).unwrap().active().id(), moved);
    let fresh = pane.active().id();
    assert!(state.new_tab(loc("/n")).id().get() > fresh.get());
}

#[test]
fn reorder_of_a_lone_tab_keeps_it() {
    let (mut state, _, right) = rig();
    let before = ids(&state, right);
    state
        .move_tab(change((right, 0), (right, 1), false))
        .unwrap();
    assert_eq!(ids(&state, right), before);
    assert_eq!(state.focused_pane(), right);
}

#[test]
fn bad_tab_or_pane_changes_nothing() {
    let (mut state, left, right) = rig();
    let before = state.clone();
    assert_eq!(
        state.move_tab(change((left, 7), (right, 0), false)),
        Err(StateError::NoSuchTab(7))
    );
    let ghost = PaneId::new(999);
    for (from, to) in [(ghost, right), (left, ghost)] {
        assert_eq!(
            state.move_tab(change((from, 0), (to, 0), false)),
            Err(StateError::UnknownPane(ghost))
        );
    }
    assert_eq!(state, before);
}

fn pin(state: &mut AppState, pane: PaneId, index: usize) {
    let id = ids(state, pane)[index];
    state.tab_mut(id).unwrap().set_pinned(true);
    state.pane_mut(pane).unwrap().keep_pinned_first();
}

fn pinned_flags(state: &AppState, pane: PaneId) -> Vec<bool> {
    let pane = state.pane(pane).unwrap();
    pane.tabs().iter().map(crate::Tab::is_pinned).collect()
}

#[test]
fn dragging_an_unpinned_tab_before_the_pinned_group_lands_after_it() {
    let (mut state, left, _) = rig();
    pin(&mut state, left, 0);
    let before = ids(&state, left);
    state.move_tab(change((left, 2), (left, 0), false)).unwrap();
    assert_eq!(ids(&state, left), [before[0], before[2], before[1]]);
    assert_eq!(pinned_flags(&state, left), [true, false, false]);
    let pane = state.pane(left).unwrap();
    assert_eq!(pane.active().id(), before[2]);
}

#[test]
fn dragging_a_pinned_tab_past_unpinned_tabs_stays_in_the_pinned_group() {
    let (mut state, left, _) = rig();
    pin(&mut state, left, 0);
    pin(&mut state, left, 1);
    let before = ids(&state, left);
    state.move_tab(change((left, 0), (left, 3), false)).unwrap();
    assert_eq!(ids(&state, left), [before[1], before[0], before[2]]);
    assert_eq!(pinned_flags(&state, left), [true, true, false]);
    assert_eq!(state.pane(left).unwrap().active().id(), before[0]);
}

#[test]
fn dragging_an_unpinned_tab_to_the_front_of_another_panes_pinned_group_lands_after_it() {
    let (mut state, left, right) = rig();
    pin(&mut state, right, 0);
    let moved = ids(&state, left)[1];
    let pinned = ids(&state, right)[0];
    state
        .move_tab(change((left, 1), (right, 0), false))
        .unwrap();
    assert_eq!(ids(&state, right), [pinned, moved]);
    assert_eq!(pinned_flags(&state, right), [true, false]);
    assert_eq!(state.pane(right).unwrap().active().id(), moved);
}

#[test]
fn dragging_a_pinned_tab_to_the_end_of_another_pane_joins_its_pinned_group() {
    let (mut state, left, right) = rig();
    pin(&mut state, left, 0);
    let moved = ids(&state, left)[0];
    let other = ids(&state, right)[0];
    state
        .move_tab(change((left, 0), (right, 9), false))
        .unwrap();
    assert_eq!(ids(&state, right), [moved, other]);
    assert_eq!(pinned_flags(&state, right), [true, false]);
    assert_eq!(state.pane(right).unwrap().active().id(), moved);
    assert_eq!(pinned_flags(&state, left), [false, false]);
}
