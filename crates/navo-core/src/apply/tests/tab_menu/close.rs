use super::*;

fn five_with_two_pinned() -> AppState {
    let mut state = with_tabs(&["/a", "/b", "/c", "/d", "/e"]);
    pin(&mut state, 3);
    pin(&mut state, 4);
    assert_eq!(folders(&state), ["/d", "/e", "/a", "/b", "/c"]);
    state
}

#[test]
fn close_others_keeps_pinned_tabs_and_activates_the_target() {
    let mut state = five_with_two_pinned();
    let target = tab_at(&state, 3);
    tab_command(&mut state, TabCommand::CloseOthers { tab: Some(target) });
    assert_eq!(folders(&state), ["/d", "/e", "/b"]);
    assert_eq!(state.active_tab().id(), target);
}

#[test]
fn close_others_on_a_pinned_tab_keeps_every_pinned_tab() {
    let mut state = five_with_two_pinned();
    let target = tab_at(&state, 0);
    tab_command(&mut state, TabCommand::CloseOthers { tab: Some(target) });
    assert_eq!(folders(&state), ["/d", "/e"]);
}

#[test]
fn close_right_keeps_pinned_tabs_and_tabs_on_the_left() {
    let mut state = five_with_two_pinned();
    let target = tab_at(&state, 0);
    tab_command(&mut state, TabCommand::CloseRight { tab: Some(target) });
    assert_eq!(folders(&state), ["/d", "/e"]);
    let mut state = five_with_two_pinned();
    let target = tab_at(&state, 2);
    tab_command(&mut state, TabCommand::CloseRight { tab: Some(target) });
    assert_eq!(folders(&state), ["/d", "/e", "/a"]);
}

#[test]
fn close_right_keeps_the_active_tab_when_it_stays() {
    let mut state = with_tabs(&["/a", "/b", "/c"]);
    run(&mut state, &Command::PaneActivateTab { tab: 0 });
    let target = tab_at(&state, 1);
    tab_command(&mut state, TabCommand::CloseRight { tab: Some(target) });
    assert_eq!(folders(&state), ["/a", "/b"]);
    assert_eq!(state.active_tab().location(), &location("/a"));
}

#[test]
fn an_explicit_close_removes_a_pinned_tab() {
    let mut state = five_with_two_pinned();
    let pinned = tab_at(&state, 1);
    tab_command(&mut state, TabCommand::Close { tab: Some(pinned) });
    assert_eq!(folders(&state), ["/d", "/a", "/b", "/c"]);
    run(&mut state, &Command::PaneActivateTab { tab: 0 });
    run(&mut state, &Command::PaneCloseTab { tab: None });
    assert_eq!(folders(&state), ["/a", "/b", "/c"]);
}

#[test]
fn closing_the_only_tab_of_the_only_pane_is_refused_and_not_remembered() {
    let mut state = with_tabs(&["/a"]);
    let tab = tab_at(&state, 0);
    let error = refused(&mut state, TabCommand::Close { tab: Some(tab) });
    assert_eq!(error, StateError::LastTab);
    let error = refused(&mut state, TabCommand::Reopen);
    assert_eq!(error, StateError::NothingToReopen);
}

#[test]
fn closing_the_only_tab_of_a_tiled_pane_closes_the_pane() {
    let mut state = with_tabs(&["/a"]);
    let right = split(&mut state);
    let tab = state.pane(right).unwrap().active().id();
    tab_command(&mut state, TabCommand::Close { tab: Some(tab) });
    assert_eq!(pane_count(&state), 1);
    assert!(state.pane(right).is_none());
}

#[test]
fn pane_close_tab_still_refuses_the_last_tab_of_the_only_pane() {
    let mut state = with_tabs(&["/a"]);
    let error = apply(&mut state, &Command::PaneCloseTab { tab: None }).unwrap_err();
    assert_eq!(error, CommandError::State(StateError::LastTab));
}

#[test]
fn close_right_on_a_pinned_tab_closes_every_unpinned_tab_to_its_right() {
    let mut state = with_tabs(&["/a", "/b", "/c", "/d"]);
    let pinned = pin(&mut state, 1);
    assert_eq!(folders(&state), ["/b", "/a", "/c", "/d"]);
    tab_command(&mut state, TabCommand::CloseRight { tab: Some(pinned) });
    assert_eq!(folders(&state), ["/b"]);
    assert_eq!(state.active_tab().id(), pinned);
    tab_command(&mut state, TabCommand::Reopen);
    tab_command(&mut state, TabCommand::Reopen);
    tab_command(&mut state, TabCommand::Reopen);
    let mut reopened = folders(&state);
    reopened.sort();
    assert_eq!(reopened, ["/a", "/b", "/c", "/d"]);
}
