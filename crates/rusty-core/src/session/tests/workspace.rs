use super::*;
use crate::{WorkspaceAction as W, apply_workspace};

#[test]
fn workspace_session_round_trip_keeps_saved_drafts_and_active_state() {
    let mut state = AppState::new(loc("C:/start"));
    apply_workspace(
        &mut state,
        &W::New {
            name: Some("Project".into()),
        },
    )
    .unwrap();
    navigate(&mut state, "C:/draft");
    state.remember_marks(vec!["keep.txt".into()]);
    apply_workspace(&mut state, &W::Switch { n: 1 }).unwrap();
    navigate(&mut state, "C:/active");
    let session = state.to_session(0);
    let bytes = postcard::to_stdvec(&session).unwrap();
    let decoded = postcard::from_bytes(&bytes).unwrap();
    let mut restored = AppState::from_session(decoded).unwrap();
    assert_eq!(restored.to_session(0), session);
    apply_workspace(&mut restored, &W::Switch { n: 2 }).unwrap();
    assert_eq!(restored.active_tab().location(), &loc("C:/draft"));
    assert_eq!(restored.last_marks(), Some(["keep.txt".into()].as_slice()));
    apply_workspace(&mut restored, &W::Discard { n: 2 }).unwrap();
    assert_eq!(restored.active_tab().location(), &loc("C:/start"));
}

#[test]
fn workspace_session_without_workspaces_migrates_to_one_saved_workspace() {
    let mut legacy = serde_json::to_value(AppState::new(loc("C:/legacy")).to_session(0)).unwrap();
    let window = legacy["windows"][0].as_object_mut().unwrap();
    window.remove("workspaces");
    window.remove("active_workspace");
    let session = serde_json::from_value(legacy).unwrap();
    let restored = AppState::from_session(session).unwrap();
    let window = restored.window(restored.focused_window()).unwrap();
    assert_eq!(window.workspaces().len(), 1);
    assert_eq!(window.workspaces()[0].name, "Workspace 1");
    assert_eq!(window.workspaces()[0].saved, window.snapshot());
}

#[test]
fn workspace_session_rejects_ids_owned_by_another_workspace() {
    let mut state = AppState::new(loc("C:/start"));
    apply_workspace(&mut state, &W::New { name: None }).unwrap();
    let mut session = state.to_session(0);
    session.windows[0].workspaces[1].saved.panes[0].tabs[0].id = 3;
    assert!(AppState::from_session(session).is_err());
}

fn focus(state: &mut AppState, order: &[usize]) {
    for &n in order {
        run(state, Command::LayoutFocusPane { n });
    }
}

fn target_and_fallback(state: &AppState) -> (Option<PaneId>, Option<PaneId>) {
    let window = state.window(state.focused_window()).unwrap();
    let focused = window.layout().focused();
    let fallback = window
        .layout()
        .numbered()
        .into_iter()
        .find(|&id| id != focused);
    (window.target(), fallback)
}

fn two_workspaces_with_chosen_targets() -> AppState {
    let mut state = AppState::new(loc("C:/start"));
    split(&mut state, Direction::Right);
    split(&mut state, Direction::Right);
    focus(&mut state, &[3, 2]);
    apply_workspace(&mut state, &W::New { name: None }).unwrap();
    focus(&mut state, &[3, 1]);
    apply_workspace(&mut state, &W::Save).unwrap();
    apply_workspace(&mut state, &W::Switch { n: 1 }).unwrap();
    state
}

#[test]
fn workspace_session_round_trip_keeps_each_workspace_target() {
    let state = two_workspaces_with_chosen_targets();
    let first = target_and_fallback(&state);
    assert_ne!(first.0, first.1);
    let session = state.to_session(0);
    assert!(session.windows[0].target.is_some());
    let bytes = postcard::to_stdvec(&session).unwrap();
    let mut restored = AppState::from_session(postcard::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(restored.to_session(0), session);
    assert_eq!(target_and_fallback(&restored), first);
    let mut original = state;
    apply_workspace(&mut original, &W::Switch { n: 2 }).unwrap();
    apply_workspace(&mut restored, &W::Switch { n: 2 }).unwrap();
    let second = target_and_fallback(&original);
    assert_ne!(second.0, second.1);
    assert_eq!(target_and_fallback(&restored), second);
}

#[test]
fn workspace_session_without_targets_loads_with_the_fallback_target() {
    let mut old = serde_json::to_value(two_workspaces_with_chosen_targets().to_session(0)).unwrap();
    let window = old["windows"][0].as_object_mut().unwrap();
    window.remove("target");
    for workspace in window["workspaces"].as_array_mut().unwrap() {
        for key in ["saved", "live"] {
            if let Some(snapshot) = workspace[key].as_object_mut() {
                snapshot.remove("target");
            }
        }
    }
    let restored = AppState::from_session(serde_json::from_value(old).unwrap()).unwrap();
    let (target, fallback) = target_and_fallback(&restored);
    assert_eq!(target, fallback);
}

#[test]
fn workspace_session_replaces_a_target_that_is_not_a_leaf() {
    let session = two_workspaces_with_chosen_targets().to_session(0);
    let focused = session.windows[0].focused_pane;
    for stale in [focused, 9_999, session.windows[0].id] {
        let mut session = session.clone();
        session.windows[0].target = Some(stale);
        let workspace = &mut session.windows[0].workspaces[0];
        for snapshot in std::iter::once(&mut workspace.saved).chain(workspace.live.as_mut()) {
            snapshot.target = Some(stale);
        }
        let restored = AppState::from_session(session).unwrap();
        let (target, fallback) = target_and_fallback(&restored);
        assert_eq!(target, fallback, "{stale}");
        let window = restored.window(restored.focused_window()).unwrap();
        assert!(window.layout().contains(target.unwrap()));
    }
}
