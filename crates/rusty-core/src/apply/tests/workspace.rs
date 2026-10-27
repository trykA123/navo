use super::{files, load, loc, run};
use crate::{
    AppState, Command, Direction, Effect, StateError, WorkspaceAction as W, apply_workspace,
    changes,
};
use proptest::prelude::*;
use std::collections::HashSet;

fn state() -> AppState {
    AppState::new(loc("C:/start"))
}

fn action(state: &mut AppState, action: W) -> Vec<Effect> {
    apply_workspace(state, &action).unwrap()
}

#[test]
fn workspace_switch_restores_tabs_focus_layout_and_marks_without_listings() {
    let mut state = state();
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    run(
        &mut state,
        Command::PaneOpenTab {
            location: Some(loc("C:/work")),
        },
    );
    state.remember_marks(vec!["remember.txt".into()]);
    load(&mut state, files(3));
    let before = state.window(state.focused_window()).unwrap().snapshot();
    action(&mut state, W::New { name: None });
    run(&mut state, Command::LayoutClose);
    let effects = action(&mut state, W::Switch { n: 1 });
    let window = state.window(state.focused_window()).unwrap();
    assert_eq!(window.snapshot(), before);
    assert_eq!(window.active_workspace(), 0);
    assert!(window.workspaces()[0].live.is_none());
    assert_eq!(state.last_marks(), Some(["remember.txt".into()].as_slice()));
    assert!(
        window
            .panes()
            .iter()
            .flat_map(|p| p.tabs())
            .all(|t| t.listing().is_none())
    );
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, Effect::ListFolder { .. }))
            .count(),
        2
    );
}

#[test]
fn workspace_draft_survives_switch_and_save_clears_it() {
    let mut state = state();
    action(&mut state, W::New { name: None });
    action(&mut state, W::Switch { n: 1 });
    run(
        &mut state,
        Command::PaneOpenTab {
            location: Some(loc("C:/draft")),
        },
    );
    action(&mut state, W::Switch { n: 2 });
    let window = state.window(state.focused_window()).unwrap();
    let workspace = &window.workspaces()[0];
    assert_eq!(
        changes(&workspace.saved, workspace.live.as_ref().unwrap()),
        1
    );
    action(&mut state, W::Switch { n: 1 });
    action(&mut state, W::Save);
    action(&mut state, W::Switch { n: 2 });
    assert!(
        state.window(state.focused_window()).unwrap().workspaces()[0]
            .live
            .is_none()
    );
}

#[test]
fn workspace_discard_restores_saved_tabs_and_clears_inactive_draft() {
    let mut state = state();
    let id = state.active_tab().id();
    run(&mut state, Command::PaneOpenTab { location: None });
    action(&mut state, W::Discard { n: 1 });
    assert_eq!(state.active_tab().id(), id);
    action(&mut state, W::New { name: None });
    run(&mut state, Command::PaneOpenTab { location: None });
    action(&mut state, W::Switch { n: 1 });
    action(&mut state, W::Discard { n: 2 });
    action(&mut state, W::Switch { n: 2 });
    assert_eq!(state.pane(state.focused_pane()).unwrap().tabs().len(), 1);
}

#[test]
fn workspace_limits_and_missing_numbers_leave_state_unchanged() {
    let mut state = state();
    for _ in 1..3 {
        action(&mut state, W::New { name: None });
    }
    let before = state.clone();
    assert_eq!(
        apply_workspace(&mut state, &W::Switch { n: 5 }),
        Err(StateError::NoWorkspace(5))
    );
    assert_eq!(state, before);
    for _ in 3..9 {
        action(&mut state, W::New { name: None });
    }
    let before = state.clone();
    assert_eq!(
        apply_workspace(&mut state, &W::New { name: None }),
        Err(StateError::WorkspaceLimit)
    );
    assert_eq!(state, before);
}

#[test]
fn workspace_delete_restore_preserves_index_draft_and_ids() {
    let mut state = state();
    action(
        &mut state,
        W::New {
            name: Some("Middle".into()),
        },
    );
    run(
        &mut state,
        Command::PaneOpenTab {
            location: Some(loc("C:/draft")),
        },
    );
    let before = state.window(state.focused_window()).unwrap().snapshot();
    action(&mut state, W::New { name: None });
    action(&mut state, W::Delete { n: 2 });
    action(&mut state, W::Restore);
    let window = state.window(state.focused_window()).unwrap();
    assert_eq!(window.workspaces()[1].name, "Middle");
    assert_eq!(window.active_workspace(), 2);
    action(&mut state, W::Switch { n: 2 });
    assert_eq!(
        state.window(state.focused_window()).unwrap().snapshot(),
        before
    );
    assert_eq!(
        apply_workspace(&mut state, &W::Restore),
        Err(StateError::NothingToRestore)
    );
}

#[test]
fn workspace_rename_move_and_last_delete_validate_input() {
    let mut state = state();
    assert_eq!(
        apply_workspace(&mut state, &W::Delete { n: 1 }),
        Err(StateError::LastWorkspace)
    );
    for name in ["  ".to_owned(), "x".repeat(41)] {
        assert_eq!(
            apply_workspace(&mut state, &W::Rename { n: 1, name }),
            Err(StateError::InvalidWorkspaceName)
        );
    }
    action(
        &mut state,
        W::Rename {
            n: 1,
            name: "  Project  ".into(),
        },
    );
    action(&mut state, W::New { name: None });
    action(&mut state, W::Move { n: 2, by: -1 });
    let window = state.window(state.focused_window()).unwrap();
    assert_eq!(window.active_workspace(), 0);
    assert_eq!(window.workspaces()[1].name, "Project");
    let before = state.clone();
    assert!(action(&mut state, W::Switch { n: 1 }).is_empty());
    assert_eq!(state, before);
}

#[test]
fn workspace_changes_counts_folder_positions_active_tab_and_layout() {
    let mut state = state();
    let before = state.window(state.focused_window()).unwrap().snapshot();
    run(
        &mut state,
        Command::PaneNavigate {
            location: loc("C:/other"),
        },
    );
    assert_eq!(
        changes(
            &before,
            &state.window(state.focused_window()).unwrap().snapshot()
        ),
        1
    );
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    assert_eq!(
        changes(
            &before,
            &state.window(state.focused_window()).unwrap().snapshot()
        ),
        3
    );
}

#[test]
fn workspace_active_delete_restore_keeps_draft_and_selects_neighbour() {
    let mut state = state();
    action(&mut state, W::New { name: None });
    run(
        &mut state,
        Command::PaneNavigate {
            location: loc("C:/draft"),
        },
    );
    let snapshot = state.window(state.focused_window()).unwrap().snapshot();
    action(&mut state, W::Delete { n: 2 });
    assert_eq!(state.active_tab().location(), &loc("C:/start"));
    action(&mut state, W::Restore);
    action(&mut state, W::Switch { n: 2 });
    assert_eq!(
        state.window(state.focused_window()).unwrap().snapshot(),
        snapshot
    );
}

#[test]
fn workspace_switch_rejects_a_listing_from_the_previous_visit() {
    let mut state = state();
    let previous = run(&mut state, Command::PaneRefresh);
    action(&mut state, W::New { name: None });
    action(&mut state, W::Switch { n: 1 });
    for effect in previous {
        if let Effect::ListFolder {
            tab,
            location,
            request,
        } = effect
        {
            crate::apply_event(
                &mut state,
                crate::Event::ListingLoaded {
                    tab,
                    location,
                    request,
                    listing: files(3),
                },
            );
        }
    }
    assert!(state.active_tab().listing().is_none());
}

fn unique_ids(state: &AppState) {
    let mut owners = std::collections::HashMap::new();
    for id in state.window_ids() {
        let window = state.window(id).unwrap();
        assert!(owners.insert(id.get(), (id, usize::MAX)).is_none());
        for (index, workspace) in window.workspaces().iter().enumerate() {
            let current = window.snapshot();
            let effective = if index == window.active_workspace() {
                &current
            } else {
                workspace.live.as_ref().unwrap_or(&workspace.saved)
            };
            for value in [&workspace.saved, effective]
                .into_iter()
                .flat_map(snapshot_ids)
            {
                let owner = owners.entry(value).or_insert((id, index));
                assert_eq!(*owner, (id, index));
            }
        }
    }
}

fn snapshot_ids(snapshot: &crate::Snapshot) -> Vec<u64> {
    let ids: Vec<_> = snapshot
        .panes()
        .into_iter()
        .flat_map(|pane| {
            std::iter::once(pane.id().get()).chain(pane.tabs().iter().map(|tab| tab.id().get()))
        })
        .collect();
    assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
    ids
}

proptest! {
    #[test]
    fn workspace_random_operations_preserve_unique_ids(steps in prop::collection::vec((0u8..13, 0usize..11), 0..100)) {
        let mut state = state();
        for (step, n) in steps {
            let command = match step {
                0 => W::New { name: None },
                1 => W::Switch { n },
                2 => W::Delete { n },
                3 => W::Restore,
                4 => W::Save,
                5 => W::Discard { n },
                6 => W::Move { n, by: if n.is_multiple_of(2) { -1 } else { 1 } },
                7 => { run(&mut state, Command::PaneOpenTab { location: None }); unique_ids(&state); continue; },
                8 => { let _ = crate::apply(&mut state, &Command::PaneCloseTab { tab: None }); unique_ids(&state); continue; },
                9 => { let _ = crate::apply(&mut state, &Command::LayoutSplit { direction: Direction::Right }); unique_ids(&state); continue; },
                10 => { let _ = crate::apply(&mut state, &Command::LayoutClose); unique_ids(&state); continue; },
                11 => { state = AppState::from_session(state.to_session(0)).unwrap(); unique_ids(&state); continue; },
                _ => W::Rename { n, name: "Name".into() },
            };
            let before = state.clone();
            if apply_workspace(&mut state, &command).is_err() { assert_eq!(state, before); }
            unique_ids(&state);
        }
    }
}

#[test]
fn workspace_new_command_copies_layout_and_lists_visible_tabs() {
    let mut state = state();
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let original = state.active_tab().id();
    let effects = run(&mut state, Command::WorkspaceNew { name: None });
    let window = state.window(state.focused_window()).unwrap();
    assert_eq!(window.active_workspace(), 1);
    assert_eq!(window.workspaces()[1].name, "Workspace 2");
    assert_eq!(window.panes().len(), 2);
    assert_eq!(state.active_tab().location(), &loc("C:/start"));
    assert_ne!(state.active_tab().id(), original);
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, Effect::ListFolder { .. }))
            .count(),
        2
    );
    assert_eq!(effects.last(), Some(&Effect::SaveSession));
}

#[test]
fn workspace_switch_command_preserves_drafts_and_refuses_missing_numbers() {
    let mut state = state();
    run(
        &mut state,
        Command::WorkspaceNew {
            name: Some("Project".into()),
        },
    );
    run(
        &mut state,
        Command::PaneNavigate {
            location: loc("C:/draft"),
        },
    );
    let effects = run(&mut state, Command::WorkspaceSwitch { n: 1 });
    assert!(matches!(
        effects.as_slice(),
        [Effect::ListFolder { .. }, Effect::SaveSession]
    ));
    run(&mut state, Command::WorkspaceSwitch { n: 2 });
    assert_eq!(state.active_tab().location(), &loc("C:/draft"));
    let before = state.clone();
    assert!(run(&mut state, Command::WorkspaceSwitch { n: 2 }).is_empty());
    assert_eq!(state, before);
    assert_eq!(
        crate::apply(&mut state, &Command::WorkspaceSwitch { n: 5 }),
        Err(crate::CommandError::State(StateError::NoWorkspace(5)))
    );
    assert_eq!(state, before);
}

#[test]
fn workspace_commands_rename_move_save_discard_delete_and_restore() {
    let mut state = state();
    run(&mut state, Command::WorkspaceNew { name: None });
    run(
        &mut state,
        Command::WorkspaceRename {
            n: 2,
            name: "  Project  ".into(),
        },
    );
    run(&mut state, Command::WorkspaceMove { n: 2, by: -1 });
    let window = state.window(state.focused_window()).unwrap();
    assert_eq!(window.active_workspace(), 0);
    assert_eq!(window.workspaces()[0].name, "Project");
    run(
        &mut state,
        Command::PaneNavigate {
            location: loc("C:/saved"),
        },
    );
    assert_eq!(
        run(&mut state, Command::WorkspaceSave),
        vec![Effect::SaveSession]
    );
    run(
        &mut state,
        Command::PaneNavigate {
            location: loc("C:/draft"),
        },
    );
    run(&mut state, Command::WorkspaceDiscard { n: 1 });
    assert_eq!(state.active_tab().location(), &loc("C:/saved"));
    run(&mut state, Command::WorkspaceDelete { n: 1 });
    assert_eq!(state.active_tab().location(), &loc("C:/start"));
    run(&mut state, Command::WorkspaceRestore);
    run(&mut state, Command::WorkspaceSwitch { n: 1 });
    assert_eq!(state.active_tab().location(), &loc("C:/saved"));
    assert_eq!(
        state.window(state.focused_window()).unwrap().workspaces()[0].name,
        "Project"
    );
}
