use super::*;
use crate::{WorkspaceAction, apply_workspace};

fn workspaces(state: &AppState) -> Vec<String> {
    let window = state.window(state.focused_window()).unwrap();
    window.workspaces().iter().map(|w| w.name.clone()).collect()
}

fn new_workspace(state: &mut AppState) {
    apply_workspace(state, &WorkspaceAction::New { name: None }).unwrap();
}

fn switch(state: &mut AppState, n: usize) {
    apply_workspace(state, &WorkspaceAction::Switch { n }).unwrap();
}

fn round_trips(state: &AppState) {
    let session = state.to_session(0);
    AppState::from_session(&session).expect("the saved session loads again");
}

#[test]
fn move_to_workspace_lands_at_the_end_of_its_focused_pane_and_keeps_the_pin() {
    let mut state = with_tabs(&["/a"]);
    new_workspace(&mut state);
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(location("/b")),
        },
    );
    let tab = pin(&mut state, 1);
    tab_command(
        &mut state,
        TabCommand::MoveToWorkspace {
            tab: Some(tab),
            n: 1,
        },
    );
    assert_eq!(folders(&state), ["/a"]);
    assert_eq!(workspaces(&state).len(), 2);
    round_trips(&state);
    switch(&mut state, 1);
    assert_eq!(folders(&state), ["/b", "/a"]);
    assert!(state.pane(state.focused_pane()).unwrap().tabs()[0].is_pinned());
    round_trips(&state);
}

fn pinned_named_results_tab(state: &mut AppState) -> TabId {
    let pane = state.focused_pane();
    let results = state.open_results_tab(pane, location("/r")).unwrap();
    for command in [
        TabCommand::Pin {
            tab: Some(results),
            pinned: true,
        },
        TabCommand::Rename {
            tab: Some(results),
            label: "Hits".into(),
        },
    ] {
        tab_command(state, command);
    }
    results
}

fn assert_moved_as_its_folder(state: &AppState) {
    let moved = &state.pane(state.focused_pane()).unwrap().tabs()[0];
    assert_eq!(moved.location(), &location("/r"));
    assert_eq!(moved.base_or_location(), moved.location());
    assert!(moved.is_pinned());
    assert_eq!(moved.label(), Some("Hits"));
}

#[test]
fn a_results_tab_moved_to_a_workspace_arrives_as_its_base_folder() {
    let mut state = with_tabs(&["/a"]);
    new_workspace(&mut state);
    let results = pinned_named_results_tab(&mut state);
    tab_command(
        &mut state,
        TabCommand::MoveToWorkspace {
            tab: Some(results),
            n: 1,
        },
    );
    round_trips(&state);
    switch(&mut state, 1);
    assert_eq!(folders(&state), ["/r", "/a"]);
    assert_moved_as_its_folder(&state);
}

#[test]
fn a_results_tab_moved_to_a_new_workspace_arrives_as_its_base_folder() {
    let mut state = with_tabs(&["/a"]);
    let results = pinned_named_results_tab(&mut state);
    tab_command(
        &mut state,
        TabCommand::MoveToNewWorkspace { tab: Some(results) },
    );
    assert_eq!(workspaces(&state), ["Workspace 1", "r"]);
    round_trips(&state);
    switch(&mut state, 2);
    assert_eq!(folders(&state), ["/r"]);
    assert_moved_as_its_folder(&state);
}

#[test]
fn moving_the_last_tab_of_a_tiled_pane_closes_that_pane() {
    let mut state = with_tabs(&["/a"]);
    new_workspace(&mut state);
    let right = split(&mut state);
    let tab = state.active_tab().id();
    tab_command(
        &mut state,
        TabCommand::MoveToWorkspace {
            tab: Some(tab),
            n: 1,
        },
    );
    assert_eq!(pane_count(&state), 1);
    assert!(state.pane(right).is_none());
}

#[test]
fn moving_the_only_tab_of_the_only_pane_leaves_a_fresh_tab() {
    let mut state = with_tabs(&["/a"]);
    new_workspace(&mut state);
    let tab = state.active_tab().id();
    tab_command(
        &mut state,
        TabCommand::MoveToWorkspace {
            tab: Some(tab),
            n: 1,
        },
    );
    assert_eq!(folders(&state), ["/a"]);
    assert_ne!(state.active_tab().id(), tab);
    switch(&mut state, 1);
    assert_eq!(folders(&state), ["/a", "/a"]);
}

#[test]
fn move_to_the_current_workspace_changes_nothing_and_a_missing_one_is_refused() {
    let mut state = with_tabs(&["/a", "/b"]);
    let before = state.clone();
    tab_command(&mut state, TabCommand::MoveToWorkspace { tab: None, n: 1 });
    assert_eq!(state, before);
    let error = refused(&mut state, TabCommand::MoveToWorkspace { tab: None, n: 2 });
    assert_eq!(error, StateError::NoWorkspace(2));
}

#[test]
fn discarding_after_a_move_keeps_the_session_loadable() {
    let mut state = with_tabs(&["/a", "/b"]);
    new_workspace(&mut state);
    switch(&mut state, 1);
    tab_command(&mut state, TabCommand::MoveToWorkspace { tab: None, n: 2 });
    apply_workspace(&mut state, &WorkspaceAction::Discard { n: 1 }).unwrap();
    round_trips(&state);
}

#[test]
fn move_to_new_workspace_names_it_after_the_folder_and_stays_here() {
    let mut state = with_tabs(&["/a", "/projects/report"]);
    tab_command(&mut state, TabCommand::MoveToNewWorkspace { tab: None });
    assert_eq!(workspaces(&state), ["Workspace 1", "report"]);
    assert_eq!(folders(&state), ["/a"]);
    round_trips(&state);
    switch(&mut state, 2);
    assert_eq!(folders(&state), ["/projects/report"]);
    assert_eq!(pane_count(&state), 1);
}

#[test]
fn move_to_new_workspace_is_refused_at_nine_workspaces() {
    let mut state = with_tabs(&["/a", "/b"]);
    for _ in 1..9 {
        new_workspace(&mut state);
    }
    let error = refused(&mut state, TabCommand::MoveToNewWorkspace { tab: None });
    assert_eq!(error, StateError::WorkspaceLimit);
}

#[test]
fn move_to_pane_appends_to_the_target_and_focuses_it() {
    let mut state = with_tabs(&["/a", "/b"]);
    let left = state.focused_pane();
    let moved = tab_at(&state, 0);
    let right = split(&mut state);
    tab_command(
        &mut state,
        TabCommand::MoveToPane {
            tab: Some(moved),
            n: 2,
        },
    );
    assert_eq!(pane_folders(&state, left), ["/b"]);
    assert_eq!(pane_folders(&state, right), ["/b", "/a"]);
    assert_eq!(state.focused_pane(), right);
    assert_eq!(state.active_tab().id(), moved);
}

#[test]
fn a_pinned_tab_moved_to_a_pane_joins_its_pinned_group() {
    let mut state = with_tabs(&["/a", "/b"]);
    let moved = pin(&mut state, 1);
    let right = split(&mut state);
    tab_command(
        &mut state,
        TabCommand::MoveToPane {
            tab: Some(moved),
            n: 2,
        },
    );
    assert_eq!(pane_folders(&state, right), ["/b", "/b"]);
    assert_eq!(state.pane(right).unwrap().tabs()[0].id(), moved);
}

#[test]
fn move_to_a_missing_pane_is_refused() {
    let mut state = with_tabs(&["/a", "/b"]);
    let error = refused(&mut state, TabCommand::MoveToPane { tab: None, n: 3 });
    assert_eq!(error, StateError::NoSuchPane(3));
}

#[test]
fn move_to_new_pane_splits_right_of_the_tabs_pane() {
    let mut state = with_tabs(&["/a", "/b"]);
    let left = state.focused_pane();
    let moved = tab_at(&state, 0);
    tab_command(&mut state, TabCommand::MoveToNewPane { tab: Some(moved) });
    assert_eq!(pane_count(&state), 2);
    assert_eq!(pane_folders(&state, left), ["/b"]);
    let pane = state.pane(state.focused_pane()).unwrap();
    assert_eq!(pane.tabs().len(), 1);
    assert_eq!(pane.active().id(), moved);
    let numbered = state
        .window(state.focused_window())
        .unwrap()
        .layout()
        .numbered();
    assert_eq!(numbered, [left, pane.id()]);
}

#[test]
fn move_to_new_pane_of_a_lone_tab_leaves_a_fresh_tab_behind() {
    let mut state = with_tabs(&["/a"]);
    let left = state.focused_pane();
    let moved = tab_at(&state, 0);
    tab_command(&mut state, TabCommand::MoveToNewPane { tab: Some(moved) });
    assert_eq!(pane_count(&state), 2);
    assert_eq!(pane_folders(&state, left), ["/a"]);
    let fresh = state.pane(left).unwrap().active();
    assert_ne!(fresh.id(), moved);
    assert!(fresh.history().back().is_empty());
    let shown = state.pane(state.focused_pane()).unwrap();
    assert_eq!(shown.tabs().len(), 1);
    assert_eq!(shown.active().id(), moved);
}

#[test]
fn move_to_new_pane_is_refused_at_four_panes() {
    let mut state = with_tabs(&["/a", "/b"]);
    for _ in 1..4 {
        split(&mut state);
    }
    assert_eq!(pane_count(&state), 4);
    let error = refused(&mut state, TabCommand::MoveToNewPane { tab: None });
    assert_eq!(error, StateError::TooManyPanes);
}
