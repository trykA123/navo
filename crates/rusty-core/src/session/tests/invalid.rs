use super::*;

#[track_caller]
fn assert_invalid(session: SessionState) {
    let result = AppState::from_session(session);
    assert!(
        matches!(result, Err(SessionError::Invalid(_))),
        "{result:?}"
    );
}

#[test]
fn no_windows_is_invalid() {
    let mut session = valid();
    session.windows.clear();
    assert_invalid(session);
}

#[test]
fn a_tab_reusing_a_pane_id_is_invalid() {
    let mut session = valid();
    session.windows[0].panes[1].tabs[1].id = 2;
    assert_invalid(session);
}

#[test]
fn unknown_focused_window_is_invalid() {
    let mut session = valid();
    session.focused_window = 9;
    assert_invalid(session);
}

#[test]
fn focused_pane_outside_its_window_is_invalid() {
    let mut session = valid();
    session.windows[0].focused_pane = 9;
    assert_invalid(session);
}

#[test]
fn layout_naming_other_panes_than_the_list_is_invalid() {
    let mut session = valid();
    session.windows[0].layout = split_state(true, LayoutState::Pane(2), LayoutState::Pane(9));
    assert_invalid(session);
}

#[test]
fn pane_without_tabs_is_invalid() {
    let mut session = valid();
    session.windows[0].panes[0].tabs.clear();
    assert_invalid(session);
}

#[test]
fn active_tab_out_of_range_is_invalid() {
    let mut session = valid();
    session.windows[0].panes[1].active = 2;
    assert_invalid(session);
}

#[test]
fn ratio_outside_the_allowed_range_is_invalid() {
    let mut session = valid();
    session.windows[0].layout = LayoutState::Split {
        side_by_side: true,
        ratio: 901,
        first: Box::new(LayoutState::Pane(2)),
        second: Box::new(LayoutState::Pane(4)),
    };
    assert_invalid(session);
}

#[test]
fn location_that_location_new_rejects_is_invalid() {
    let mut session = valid();
    session.windows[0].panes[0].tabs[0].location = String::new();
    assert_invalid(session);
}

#[test]
fn next_id_not_above_every_id_is_invalid() {
    let mut session = valid();
    session.next_id = 6;
    assert_invalid(session);
}

#[test]
fn an_implausibly_large_next_id_is_invalid() {
    let mut session = home().to_session(100);
    session.next_id = u64::MAX;
    assert_invalid(session);
}
