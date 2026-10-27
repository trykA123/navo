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
