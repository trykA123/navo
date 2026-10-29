use super::{loc, run};
use crate::{
    AppState, Command, CommandError, Direction, Effect, Location, PaneId, StateError, TabCommand,
    TabId, apply,
};

mod close;
mod pin;
mod relocate;
mod reopen;

fn tab_command(state: &mut AppState, command: TabCommand) -> Vec<Effect> {
    run(state, &Command::Tab(command))
}

fn refused(state: &mut AppState, command: TabCommand) -> StateError {
    let before = state.clone();
    let error = apply(state, &Command::Tab(command)).unwrap_err();
    assert_eq!(
        *state, before,
        "a refused tab command leaves the state as it was"
    );
    match error {
        CommandError::State(error) => error,
        other => panic!("expected a state error, got {other:?}"),
    }
}

fn with_tabs(folders: &[&str]) -> AppState {
    let mut state = AppState::new(loc(folders[0]));
    for folder in &folders[1..] {
        run(
            &mut state,
            &Command::PaneOpenTab {
                location: Some(loc(folder)),
            },
        );
    }
    state
}

fn pane_folders(state: &AppState, pane: PaneId) -> Vec<String> {
    state
        .pane(pane)
        .unwrap()
        .tabs()
        .iter()
        .map(|tab| tab.location().as_str().to_owned())
        .collect()
}

fn folders(state: &AppState) -> Vec<String> {
    pane_folders(state, state.focused_pane())
}

fn tab_at(state: &AppState, index: usize) -> TabId {
    state.pane(state.focused_pane()).unwrap().tabs()[index].id()
}

fn pin(state: &mut AppState, index: usize) -> TabId {
    let tab = tab_at(state, index);
    tab_command(
        state,
        TabCommand::Pin {
            tab: Some(tab),
            pinned: true,
        },
    );
    tab
}

fn split(state: &mut AppState) -> PaneId {
    run(
        state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    state.focused_pane()
}

fn pane_count(state: &AppState) -> usize {
    state.window(state.focused_window()).unwrap().panes().len()
}

fn location(text: &str) -> Location {
    loc(text)
}
