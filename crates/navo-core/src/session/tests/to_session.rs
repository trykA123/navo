use super::*;

#[test]
fn initial_state_saves_one_window_one_pane_one_tab() {
    let expected = SessionState {
        windows: vec![WindowState {
            id: 1,
            placement: Vec::new(),
            workspaces: vec![WorkspaceState {
                name: "Workspace 1".into(),
                saved: SnapshotState {
                    layout: LayoutState::Pane(2),
                    focused_pane: 2,
                    panes: vec![pane_state(2, vec![tab_state(3, "C:/start")])],
                    last_marks: None,
                    target: None,
                },
                live: None,
            }],
            active_workspace: 0,
            target: None,
            layout: LayoutState::Pane(2),
            focused_pane: 2,
            panes: vec![pane_state(2, vec![tab_state(3, "C:/start")])],
        }],
        focused_window: 1,
        next_id: 4,
    };
    assert_eq!(AppState::new(loc("C:/start")).to_session(100), expected);
}

#[test]
fn split_right_saves_a_side_by_side_split_with_the_new_pane_second() {
    let mut state = home();
    split(&mut state, Direction::Right);
    let expected = split_state(true, LayoutState::Pane(2), LayoutState::Pane(4));
    assert_eq!(state.to_session(100).windows[0].layout, expected);
}

#[test]
fn split_down_saves_a_stacked_split() {
    let mut state = home();
    split(&mut state, Direction::Down);
    let expected = split_state(false, LayoutState::Pane(2), LayoutState::Pane(4));
    assert_eq!(state.to_session(100).windows[0].layout, expected);
}

#[test]
fn nested_splits_save_the_tree() {
    let mut state = home();
    split(&mut state, Direction::Right);
    split(&mut state, Direction::Down);
    let expected = split_state(
        true,
        LayoutState::Pane(2),
        split_state(false, LayoutState::Pane(4), LayoutState::Pane(6)),
    );
    assert_eq!(state.to_session(100).windows[0].layout, expected);
}

#[test]
fn panes_are_saved_in_layout_order() {
    let mut state = home();
    split(&mut state, Direction::Left);
    let ids: Vec<u64> = state.to_session(100).windows[0]
        .panes
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec![4, 2]);
}

#[test]
fn focused_pane_is_saved() {
    let mut state = home();
    split(&mut state, Direction::Right);
    run(
        &mut state,
        &Command::LayoutFocus {
            direction: Direction::Left,
        },
    );
    assert_eq!(state.to_session(100).windows[0].focused_pane, 2);
}

#[test]
fn history_saves_back_oldest_first_and_forward_next_first() {
    let mut state = home();
    navigate(&mut state, "/a");
    navigate(&mut state, "/b");
    navigate(&mut state, "/c");
    run(&mut state, &Command::PaneBack);
    run(&mut state, &Command::PaneBack);
    let tab = &state.to_session(100).windows[0].panes[0].tabs[0];
    let saved = (tab.location.as_str(), &tab.back, &tab.forward);
    assert_eq!(
        saved,
        (
            "/a",
            &vec!["/home".to_owned()],
            &vec!["/b".to_owned(), "/c".to_owned()]
        )
    );
}

#[test]
fn view_saves_cursor_and_scroll() {
    let mut state = home();
    load(&mut state, files(50));
    run(
        &mut state,
        &Command::PaneCursorMove {
            to: CursorMove::Row(17),
        },
    );
    state.active_tab_mut().set_scroll(9);
    let tab = &state.to_session(100).windows[0].panes[0].tabs[0];
    assert_eq!((tab.cursor, tab.scroll), (17, 9));
}

#[test]
fn tabs_and_active_index_are_saved() {
    let mut state = home();
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(loc("/a")),
        },
    );
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(loc("/b")),
        },
    );
    run(&mut state, &Command::PaneCloseTab { tab: Some(0) });
    let pane = &state.to_session(100).windows[0].panes[0];
    let tabs: Vec<(u64, &str)> = pane
        .tabs
        .iter()
        .map(|t| (t.id, t.location.as_str()))
        .collect();
    assert_eq!((pane.active, tabs), (1, vec![(4, "/a"), (5, "/b")]));
}

#[test]
fn page_rows_are_saved() {
    let mut state = home();
    let pane = state.focused_pane();
    apply_event(&mut state, Event::PageRowsChanged { pane, rows: 42 });
    assert_eq!(state.to_session(100).windows[0].panes[0].page_rows, 42);
}

#[test]
fn windows_are_saved_in_ascending_id_with_the_focused_one_named() {
    let mut state = home();
    state.open_window(loc("/w"));
    let session = state.to_session(100);
    let ids: Vec<u64> = session.windows.iter().map(|w| w.id).collect();
    assert_eq!((ids, session.focused_window), (vec![1, 4], 4));
}

#[test]
fn next_id_is_saved() {
    let mut state = home();
    split(&mut state, Direction::Right);
    run(&mut state, &Command::PaneOpenTab { location: None });
    assert_eq!(state.to_session(100).next_id, 7);
}
