use super::{at_work, fail, files, home, load, loc, marked, navigate, run};
use crate::{AppState, Command, CommandError, Direction, Effect, StateError, TabId};

fn open_tab(location: Option<&str>) -> Command {
    Command::PaneOpenTab {
        location: location.map(loc),
    }
}

fn tab_ids(state: &AppState) -> Vec<TabId> {
    state
        .pane(state.focused_pane())
        .unwrap()
        .tabs()
        .iter()
        .map(|t| t.id())
        .collect()
}

fn active_index(state: &AppState) -> usize {
    state.pane(state.focused_pane()).unwrap().active_index()
}

#[test]
fn open_tab_activates_a_new_tab_after_the_active_one() {
    let mut state = home();
    let first = state.active_tab().id();
    run(&mut state, &open_tab(Some("/a")));
    let ids = tab_ids(&state);
    assert_eq!((ids.len(), ids[0], active_index(&state)), (2, first, 1));
}

#[test]
fn open_tab_opens_the_given_location() {
    let mut state = home();
    run(&mut state, &open_tab(Some("/a")));
    assert_eq!(state.active_tab().location(), &loc("/a"));
}

#[test]
fn open_tab_without_location_uses_the_active_tab_location() {
    let mut state = home();
    navigate(&mut state, "/work");
    run(&mut state, &open_tab(None));
    assert_eq!(state.active_tab().location(), &loc("/work"));
}

#[test]
fn open_tab_lists_the_new_tab() {
    let mut state = home();
    let effects = run(&mut state, &open_tab(Some("/a")));
    assert_eq!(
        effects,
        vec![Effect::ListFolder {
            tab: state.active_tab().id(),
            location: loc("/a"),
            request: state.active_tab().pending_request().unwrap_or(0),
        }]
    );
}

#[test]
fn close_tab_without_index_closes_the_active_tab() {
    let mut state = home();
    let first = state.active_tab().id();
    run(&mut state, &open_tab(Some("/a")));
    run(&mut state, &Command::PaneCloseTab { tab: None });
    assert_eq!(tab_ids(&state), vec![first]);
}

#[test]
fn close_tab_with_index_closes_that_tab() {
    let mut state = home();
    run(&mut state, &open_tab(Some("/a")));
    let second = state.active_tab().id();
    run(&mut state, &Command::PaneCloseTab { tab: Some(0) });
    assert_eq!(tab_ids(&state), vec![second]);
}

#[test]
fn close_tab_has_no_effects() {
    let mut state = home();
    run(&mut state, &open_tab(Some("/a")));
    assert_eq!(
        run(&mut state, &Command::PaneCloseTab { tab: None }),
        vec![]
    );
}

#[test]
fn close_tab_refuses_the_last_tab() {
    let mut state = home();
    assert_eq!(
        fail(&mut state, &Command::PaneCloseTab { tab: None }),
        CommandError::State(StateError::LastTab)
    );
}

#[test]
fn close_tab_on_the_last_tab_of_a_tiled_pane_drops_the_pane_and_its_tab() {
    let mut state = home();
    run(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let closing = state.active_tab().id();
    run(&mut state, &Command::PaneCloseTab { tab: None });
    let state = &state;
    assert_eq!(
        state.window(state.focused_window()).unwrap().panes().len(),
        1
    );
    assert!(state.tab(closing).is_none());
    assert_eq!(tab_ids(state).len(), 1);
}

#[test]
fn close_tab_refuses_a_missing_index() {
    let mut state = home();
    run(&mut state, &open_tab(Some("/a")));
    assert_eq!(
        fail(&mut state, &Command::PaneCloseTab { tab: Some(5) }),
        CommandError::State(StateError::NoSuchTab(5))
    );
}

#[test]
fn navigate_moves_the_tab_to_the_location() {
    let mut state = home();
    navigate(&mut state, "/work");
    assert_eq!(state.active_tab().location(), &loc("/work"));
}

#[test]
fn navigate_lists_the_new_location() {
    let mut state = home();
    let tab = state.active_tab().id();
    assert_eq!(
        navigate(&mut state, "/work"),
        vec![Effect::ListFolder {
            tab,
            location: loc("/work"),
            request: state.active_tab().pending_request().unwrap_or(0),
        }]
    );
}

#[test]
fn navigate_clears_the_selection() {
    let mut state = at_work(files(3));
    run(&mut state, &Command::SelectionToggle);
    navigate(&mut state, "/other");
    assert!(state.active_tab().selection().is_empty());
}

#[test]
fn back_returns_to_the_previous_location() {
    let mut state = home();
    navigate(&mut state, "/work");
    run(&mut state, &Command::PaneBack);
    assert_eq!(state.active_tab().location(), &loc("/home"));
}

#[test]
fn back_lists_the_previous_location() {
    let mut state = home();
    let tab = state.active_tab().id();
    navigate(&mut state, "/work");
    assert_eq!(
        run(&mut state, &Command::PaneBack),
        vec![Effect::ListFolder {
            tab,
            location: loc("/home"),
            request: state.active_tab().pending_request().unwrap_or(0),
        }]
    );
}

#[test]
fn back_clears_the_selection() {
    let mut state = at_work(files(3));
    navigate(&mut state, "/other");
    load(&mut state, files(3));
    run(&mut state, &Command::SelectionToggle);
    run(&mut state, &Command::PaneBack);
    assert_eq!(marked(&state), Vec::<u64>::new());
}

#[test]
fn back_with_no_history_fails() {
    let mut state = home();
    assert_eq!(
        fail(&mut state, &Command::PaneBack),
        CommandError::State(StateError::NothingBack)
    );
}

#[test]
fn forward_returns_to_the_location_left_by_back() {
    let mut state = home();
    navigate(&mut state, "/work");
    run(&mut state, &Command::PaneBack);
    run(&mut state, &Command::PaneForward);
    assert_eq!(state.active_tab().location(), &loc("/work"));
}

#[test]
fn forward_lists_the_next_location() {
    let mut state = home();
    let tab = state.active_tab().id();
    navigate(&mut state, "/work");
    run(&mut state, &Command::PaneBack);
    assert_eq!(
        run(&mut state, &Command::PaneForward),
        vec![Effect::ListFolder {
            tab,
            location: loc("/work"),
            request: state.active_tab().pending_request().unwrap_or(0),
        }]
    );
}

#[test]
fn forward_clears_the_selection() {
    let mut state = at_work(files(3));
    run(&mut state, &Command::PaneBack);
    load(&mut state, files(3));
    run(&mut state, &Command::SelectionToggle);
    run(&mut state, &Command::PaneForward);
    assert_eq!(marked(&state), Vec::<u64>::new());
}

#[test]
fn forward_with_nothing_ahead_fails() {
    let mut state = home();
    navigate(&mut state, "/work");
    assert_eq!(
        fail(&mut state, &Command::PaneForward),
        CommandError::State(StateError::NothingForward)
    );
}
