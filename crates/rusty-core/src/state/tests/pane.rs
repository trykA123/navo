use super::{loc, tab};
use crate::{Pane, PaneId, StateError, TabId};

fn pane_with(count: u64) -> Pane {
    let mut pane = Pane::new(PaneId::new(1), tab(10, "/t0"));
    for i in 1..count {
        pane.open_tab(tab(10 + i, &format!("/t{i}")));
    }
    pane
}

fn tab_ids(pane: &Pane) -> Vec<u64> {
    pane.tabs().iter().map(|t| t.id().get()).collect()
}

#[test]
fn new_pane_has_its_id() {
    assert_eq!(pane_with(1).id(), PaneId::new(1));
}

#[test]
fn new_pane_has_its_first_tab_active() {
    let pane = pane_with(1);
    assert_eq!(
        (tab_ids(&pane), pane.active_index(), pane.active().id()),
        (vec![10], 0, TabId::new(10))
    );
}

#[test]
fn open_tab_appends_after_the_last_active_tab() {
    let pane = pane_with(3);
    assert_eq!((tab_ids(&pane), pane.active_index()), (vec![10, 11, 12], 2));
}

#[test]
fn open_tab_inserts_right_after_the_active_tab_and_activates_it() {
    let mut pane = pane_with(3);
    pane.activate(0).unwrap();
    pane.open_tab(tab(20, "/new"));
    assert_eq!(
        (tab_ids(&pane), pane.active_index()),
        (vec![10, 20, 11, 12], 1)
    );
}

#[test]
fn activate_makes_the_tab_active() {
    let mut pane = pane_with(3);
    pane.activate(1).unwrap();
    assert_eq!(pane.active().id(), TabId::new(11));
}

#[test]
fn activate_a_bad_index_is_no_such_tab() {
    assert_eq!(pane_with(2).activate(2), Err(StateError::NoSuchTab(2)));
}

#[test]
fn active_mut_changes_the_active_tab() {
    let mut pane = pane_with(2);
    pane.active_mut().navigate(loc("/elsewhere"));
    assert_eq!(pane.tabs()[1].location(), &loc("/elsewhere"));
}

#[test]
fn closing_the_only_tab_is_last_tab() {
    assert_eq!(pane_with(1).close_tab(0), Err(StateError::LastTab));
}

#[test]
fn closing_a_bad_index_is_no_such_tab() {
    assert_eq!(pane_with(2).close_tab(5), Err(StateError::NoSuchTab(5)));
}

#[test]
fn close_tab_removes_the_tab() {
    let mut pane = pane_with(3);
    pane.close_tab(1).unwrap();
    assert_eq!(tab_ids(&pane), vec![10, 12]);
}

#[test]
fn closing_the_active_tab_activates_the_tab_that_took_its_index() {
    let mut pane = pane_with(3);
    pane.activate(1).unwrap();
    pane.close_tab(1).unwrap();
    assert_eq!(pane.active().id(), TabId::new(12));
}

#[test]
fn closing_the_active_last_tab_activates_the_one_before() {
    let mut pane = pane_with(3);
    pane.close_tab(2).unwrap();
    assert_eq!(pane.active().id(), TabId::new(11));
}

#[test]
fn closing_a_tab_before_the_active_one_keeps_the_active_tab() {
    let mut pane = pane_with(3);
    pane.close_tab(0).unwrap();
    assert_eq!(pane.active().id(), TabId::new(12));
}

#[test]
fn closing_a_tab_after_the_active_one_keeps_the_active_tab() {
    let mut pane = pane_with(3);
    pane.activate(0).unwrap();
    pane.close_tab(2).unwrap();
    assert_eq!(pane.active().id(), TabId::new(10));
}
