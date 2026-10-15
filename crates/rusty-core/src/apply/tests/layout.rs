use super::{fail, home, loc, navigate, run};
use crate::{AppState, Command, CommandError, Direction, Effect, LayoutError, PaneId, StateError};

fn split(state: &mut AppState, direction: Direction) -> Vec<Effect> {
    run(state, Command::LayoutSplit { direction })
}

fn panes(state: &AppState) -> Vec<PaneId> {
    state
        .window(state.focused_window())
        .unwrap()
        .layout()
        .panes()
}

#[test]
fn split_adds_a_pane_and_focuses_it() {
    let mut state = home();
    let first = state.focused_pane();
    split(&mut state, Direction::Right);
    let second = state.focused_pane();
    assert_eq!(panes(&state), vec![first, second]);
}

#[test]
fn split_opens_the_new_pane_at_the_same_location() {
    let mut state = home();
    navigate(&mut state, "/work");
    split(&mut state, Direction::Down);
    assert_eq!(state.active_tab().location(), &loc("/work"));
}

#[test]
fn split_lists_the_new_pane_tab() {
    let mut state = home();
    navigate(&mut state, "/work");
    let effects = split(&mut state, Direction::Left);
    assert_eq!(
        effects,
        vec![Effect::ListFolder {
            tab: state.active_tab().id(),
            location: loc("/work"),
            request: state.active_tab().pending_request().unwrap_or(0),
        }]
    );
}

#[test]
fn close_removes_the_focused_pane() {
    let mut state = home();
    let first = state.focused_pane();
    split(&mut state, Direction::Right);
    run(&mut state, Command::LayoutClose);
    assert_eq!(panes(&state), vec![first]);
}

#[test]
fn close_has_no_effects() {
    let mut state = home();
    split(&mut state, Direction::Right);
    assert_eq!(run(&mut state, Command::LayoutClose), vec![]);
}

#[test]
fn close_refuses_the_last_pane() {
    assert_eq!(
        fail(&mut home(), Command::LayoutClose),
        CommandError::State(StateError::Layout(LayoutError::LastPane))
    );
}

#[test]
fn focus_moves_to_the_neighbour() {
    let mut state = home();
    let first = state.focused_pane();
    split(&mut state, Direction::Right);
    run(
        &mut state,
        Command::LayoutFocus {
            direction: Direction::Left,
        },
    );
    assert_eq!(state.focused_pane(), first);
}

#[test]
fn focus_has_no_effects() {
    let mut state = home();
    split(&mut state, Direction::Right);
    assert_eq!(
        run(
            &mut state,
            Command::LayoutFocus {
                direction: Direction::Left
            }
        ),
        vec![]
    );
}

#[test]
fn focus_without_a_neighbour_fails() {
    assert_eq!(
        fail(
            &mut home(),
            Command::LayoutFocus {
                direction: Direction::Up
            }
        ),
        CommandError::State(StateError::Layout(LayoutError::NoNeighbour(Direction::Up)))
    );
}

#[test]
fn save_session_asks_to_save_the_session() {
    assert_eq!(
        run(&mut home(), Command::AppSaveSession),
        vec![Effect::SaveSession]
    );
}

#[test]
fn save_session_leaves_the_state_alone() {
    let mut state = home();
    navigate(&mut state, "/work");
    let before = state.clone();
    run(&mut state, Command::AppSaveSession);
    assert_eq!(state, before);
}
