use std::collections::BTreeSet;

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::{listing, loc};
use crate::{AppState, Direction, History, Location, PaneId, StateError, WindowId};

const POOL: [&str; 5] = ["/a", "/b", "/c", "/d", "/e"];
const UNKNOWN: u64 = u64::MAX;

#[derive(Debug, Clone)]
enum Op {
    Navigate(usize),
    Back,
    Forward,
    SetListing(usize),
    SetCursor(u64),
    SetScroll(u64),
    OpenTab(usize),
    CloseTab(usize),
    Activate(usize),
    Split(Direction),
    ClosePane(Option<usize>),
    FocusDirection(Direction),
    OpenWindow(usize),
    CloseWindow(Option<usize>),
    FocusWindow(Option<usize>),
}

fn place() -> impl Strategy<Value = usize> {
    0..POOL.len()
}

fn direction() -> impl Strategy<Value = Direction> {
    prop::sample::select(Direction::ALL.to_vec())
}

fn pick() -> impl Strategy<Value = Option<usize>> {
    prop::option::weighted(0.8, any::<usize>())
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => place().prop_map(Op::Navigate),
        2 => Just(Op::Back),
        2 => Just(Op::Forward),
        2 => (0..6_usize).prop_map(Op::SetListing),
        2 => (0..8_u64).prop_map(Op::SetCursor),
        1 => (0..8_u64).prop_map(Op::SetScroll),
        2 => place().prop_map(Op::OpenTab),
        2 => (0..4_usize).prop_map(Op::CloseTab),
        1 => (0..4_usize).prop_map(Op::Activate),
        2 => direction().prop_map(Op::Split),
        2 => pick().prop_map(Op::ClosePane),
        1 => direction().prop_map(Op::FocusDirection),
        1 => place().prop_map(Op::OpenWindow),
        1 => pick().prop_map(Op::CloseWindow),
        1 => pick().prop_map(Op::FocusWindow),
    ]
}

fn at(i: usize) -> Location {
    loc(POOL[i])
}

fn all_panes(state: &AppState) -> Vec<PaneId> {
    state
        .window_ids()
        .into_iter()
        .flat_map(|id| {
            state
                .window(id)
                .map(|w| w.layout().panes())
                .unwrap_or_default()
        })
        .collect()
}

fn pane_target(state: &AppState, pick: Option<usize>) -> PaneId {
    let panes = all_panes(state);
    pick.map_or(PaneId::new(UNKNOWN), |i| panes[i % panes.len()])
}

fn window_target(state: &AppState, pick: Option<usize>) -> WindowId {
    let ids = state.window_ids();
    pick.map_or(WindowId::new(UNKNOWN), |i| ids[i % ids.len()])
}

fn apply_tab_op(state: &mut AppState, op: &Op) -> Result<(), StateError> {
    let tab = state.active_tab_mut();
    match *op {
        Op::Navigate(i) => tab.navigate(at(i)),
        Op::Back => return tab.back(),
        Op::Forward => return tab.forward(),
        Op::SetListing(len) => tab.set_listing(listing(len)),
        Op::SetCursor(row) => tab.set_cursor(row),
        Op::SetScroll(row) => tab.set_scroll(row),
        _ => {}
    }
    Ok(())
}

fn apply(state: &mut AppState, op: &Op) -> Result<(), StateError> {
    match *op {
        Op::OpenTab(i) => {
            let tab = state.new_tab(at(i));
            state.focused_pane_mut().open_tab(tab);
            Ok(())
        }
        Op::CloseTab(i) => state.focused_pane_mut().close_tab(i),
        Op::Activate(i) => state.focused_pane_mut().activate(i),
        Op::Split(d) => state.split(d).map(|_| ()),
        Op::ClosePane(pick) => state.close_pane(pane_target(state, pick)),
        Op::FocusDirection(d) => state.focus_direction(d).map(|_| ()),
        Op::OpenWindow(i) => {
            state.open_window(at(i));
            Ok(())
        }
        Op::CloseWindow(pick) => state.close_window(window_target(state, pick)),
        Op::FocusWindow(pick) => state.focus_window(window_target(state, pick)),
        _ => apply_tab_op(state, op),
    }
}

fn check_ids(state: &AppState) -> Result<(), TestCaseError> {
    let mut seen = BTreeSet::new();
    for id in state.window_ids() {
        prop_assert!(seen.insert(id.get()), "window id {} repeats", id.get());
        let window = state
            .window(id)
            .ok_or_else(|| TestCaseError::fail("missing window"))?;
        let panes: Vec<PaneId> = window.panes().iter().map(|p| p.id()).collect();
        prop_assert_eq!(window.layout().panes(), panes);
        for pane in window.panes() {
            prop_assert!(
                seen.insert(pane.id().get()),
                "pane id {} repeats",
                pane.id().get()
            );
            for tab in pane.tabs() {
                prop_assert!(
                    seen.insert(tab.id().get()),
                    "tab id {} repeats",
                    tab.id().get()
                );
            }
        }
    }
    Ok(())
}

fn check_focus(state: &AppState) -> Result<(), TestCaseError> {
    prop_assert!(state.window_ids().contains(&state.focused_window()));
    let window = state
        .window(state.focused_window())
        .ok_or_else(|| TestCaseError::fail("missing focused window"))?;
    prop_assert!(window.pane(state.focused_pane()).is_some());
    Ok(())
}

fn check_tabs(state: &AppState) -> Result<(), TestCaseError> {
    for pane_id in all_panes(state) {
        let pane = state
            .pane(pane_id)
            .ok_or_else(|| TestCaseError::fail("missing pane"))?;
        prop_assert!(!pane.tabs().is_empty());
        prop_assert!(pane.active_index() < pane.tabs().len());
        for tab in pane.tabs() {
            prop_assert!(tab.history().back().len() <= History::LIMIT);
            prop_assert!(tab.history().forward().len() <= History::LIMIT);
            if let Some(list) = tab.listing() {
                let last = list.len().saturating_sub(1);
                prop_assert!(tab.view().cursor <= last, "{:?}", tab.view());
                prop_assert!(tab.view().scroll <= last, "{:?}", tab.view());
            }
        }
    }
    Ok(())
}

fn check(state: &AppState) -> Result<(), TestCaseError> {
    check_ids(state)?;
    check_focus(state)?;
    check_tabs(state)
}

proptest! {
    #[test]
    fn random_operations_keep_the_state_valid(ops in prop::collection::vec(op(), 0..=60)) {
        let mut state = AppState::new(loc("/home"));
        check(&state)?;
        for op in &ops {
            let before = state.clone();
            if apply(&mut state, op).is_err() {
                prop_assert_eq!(&state, &before, "failed {:?} changed the state", op);
            }
            check(&state)?;
        }
    }
}
