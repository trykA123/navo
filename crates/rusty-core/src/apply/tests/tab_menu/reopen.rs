use super::*;
use crate::{WorkspaceAction, apply_workspace};

fn close(state: &mut AppState, index: usize) {
    let tab = tab_at(state, index);
    tab_command(state, TabCommand::Close { tab: Some(tab) });
}

#[test]
fn reopen_brings_back_the_last_closed_tab_first() {
    let mut state = with_tabs(&["/a", "/b", "/c"]);
    close(&mut state, 1);
    close(&mut state, 1);
    assert_eq!(folders(&state), ["/a"]);
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(folders(&state), ["/a", "/c"]);
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(folders(&state), ["/a", "/c", "/b"]);
    assert_eq!(state.active_tab().location(), &location("/b"));
    let error = refused(&mut state, TabCommand::Reopen);
    assert_eq!(error, StateError::NothingToReopen);
}

#[test]
fn reopen_restores_the_label_and_pin() {
    let mut state = with_tabs(&["/a", "/b"]);
    let tab = pin(&mut state, 1);
    tab_command(
        &mut state,
        TabCommand::Rename {
            tab: Some(tab),
            label: "Logs".into(),
        },
    );
    tab_command(&mut state, TabCommand::Close { tab: Some(tab) });
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(folders(&state), ["/b", "/a"]);
    let reopened = state.active_tab();
    assert!(reopened.is_pinned());
    assert_eq!(reopened.label(), Some("Logs"));
    assert_ne!(reopened.id(), tab);
}

#[test]
fn reopen_keeps_only_the_last_twenty_closed_tabs() {
    let names: Vec<String> = (0..23).map(|i| format!("/t{i}")).collect();
    let mut all: Vec<&str> = vec!["/keep"];
    all.extend(names.iter().map(String::as_str));
    let mut state = with_tabs(&all);
    for _ in 0..23 {
        close(&mut state, 1);
    }
    let mut reopened = Vec::new();
    while apply(&mut state, &Command::Tab(TabCommand::Reopen)).is_ok() {
        reopened.push(state.active_tab().location().as_str().to_owned());
    }
    let expected: Vec<String> = (3..23).rev().map(|i| format!("/t{i}")).collect();
    assert_eq!(reopened, expected);
}

#[test]
fn reopen_uses_the_focused_pane_when_the_old_pane_is_gone() {
    let mut state = with_tabs(&["/a"]);
    let left = state.focused_pane();
    let right = split(&mut state);
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(location("/r")),
        },
    );
    close(&mut state, 1);
    run(&mut state, &Command::LayoutClose);
    assert!(state.pane(right).is_none());
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(state.focused_pane(), left);
    assert_eq!(state.active_tab().location(), &location("/r"));
}

#[test]
fn reopen_returns_to_the_pane_it_came_from() {
    let mut state = with_tabs(&["/a", "/b"]);
    let left = state.focused_pane();
    close(&mut state, 1);
    split(&mut state);
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(state.focused_pane(), left);
    assert_eq!(pane_folders(&state, left), ["/a", "/b"]);
}

#[test]
fn close_others_and_pane_close_tab_feed_the_reopen_stack() {
    let mut state = with_tabs(&["/a", "/b", "/c"]);
    run(&mut state, &Command::PaneCloseTab { tab: Some(0) });
    let target = tab_at(&state, 0);
    tab_command(&mut state, TabCommand::CloseOthers { tab: Some(target) });
    assert_eq!(folders(&state), ["/b"]);
    tab_command(&mut state, TabCommand::Reopen);
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(folders(&state), ["/b", "/c", "/a"]);
}

#[test]
fn closing_a_tiled_panes_last_tab_can_be_reopened() {
    let mut state = with_tabs(&["/a"]);
    split(&mut state);
    run(
        &mut state,
        &Command::PaneNavigate {
            location: location("/r"),
        },
    );
    run(&mut state, &Command::PaneCloseTab { tab: None });
    assert_eq!(pane_count(&state), 1);
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(folders(&state), ["/a", "/r"]);
}

#[test]
fn reopen_after_the_source_pane_left_the_workspace_uses_the_focused_pane() {
    let mut state = with_tabs(&["/a", "/b"]);
    let source = state.focused_pane();
    close(&mut state, 1);
    apply_workspace(&mut state, &WorkspaceAction::New { name: None }).unwrap();
    assert!(state.pane(source).is_none());
    let here = state.focused_pane();
    let before = folders(&state);
    tab_command(&mut state, TabCommand::Reopen);
    assert_eq!(state.focused_pane(), here);
    assert_eq!(state.active_tab().location(), &location("/b"));
    assert_eq!(folders(&state).len(), before.len() + 1);
    apply_workspace(&mut state, &WorkspaceAction::Switch { n: 1 }).unwrap();
    assert_eq!(pane_folders(&state, source), ["/a"]);
}
