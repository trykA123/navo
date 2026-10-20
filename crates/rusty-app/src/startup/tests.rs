use rusty_core::{AppState, Command, Location};

use super::*;

fn home() -> Location {
    Location::new("/home/admin").unwrap()
}

fn pane_count(state: &AppState) -> usize {
    state
        .window(state.focused_window())
        .map_or(0, |w| w.panes().len())
}

#[test]
fn without_a_session_two_panes_open_at_home() {
    let dir = tempfile::tempdir().unwrap();
    let state = initial_state(&dir.path().join("session.bin"), home());
    assert_eq!(pane_count(&state), 2);
    assert_eq!(state.active_tab().location(), &home());
}

#[test]
fn a_saved_session_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("session.bin");
    let mut state = initial_state(&file, home());
    let work = Location::new("/work").unwrap();
    rusty_core::apply(
        &mut state,
        &Command::PaneNavigate {
            location: work.clone(),
        },
    )
    .unwrap();
    save(&state, &file).unwrap();
    let restored = initial_state(&file, home());
    assert_eq!(restored.active_tab().location(), &work);
    assert_eq!(pane_count(&restored), 2);
}

#[test]
fn a_corrupt_session_falls_back_to_home() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("session.bin");
    std::fs::write(&file, b"not a session").unwrap();
    let state = initial_state(&file, home());
    assert_eq!(state.active_tab().location(), &home());
}

#[test]
fn home_comes_from_the_profile_variable() {
    let location =
        home_location(|name| (name == "USERPROFILE").then(|| r"C:\Users\admin".to_owned()));
    assert_eq!(location.as_str(), r"C:\Users\admin");
}

#[test]
fn home_falls_back_to_home_then_root() {
    let location = home_location(|name| (name == "HOME").then(|| "/home/admin".to_owned()));
    assert_eq!(location.as_str(), "/home/admin");
    let fallback = home_location(|_| None);
    assert!(fallback.as_str() == r"C:\" || fallback.as_str() == "/");
}

#[test]
fn session_folders_are_every_tab_of_every_pane_once() {
    let mut state = AppState::new(home());
    let work = Location::new("/srv/work").unwrap();
    let open = |location: &Location| Command::PaneOpenTab {
        location: Some(location.clone()),
    };
    rusty_core::apply(&mut state, &open(&work)).unwrap();
    rusty_core::apply(&mut state, &open(&home())).unwrap();
    rusty_core::apply(
        &mut state,
        &Command::LayoutSplit {
            direction: rusty_core::Direction::Right,
        },
    )
    .unwrap();
    assert_eq!(session_folders(&state), [home(), work]);
}

fn pane_locations(state: &AppState) -> Vec<Location> {
    state
        .window(state.focused_window())
        .map(|w| {
            w.panes()
                .iter()
                .map(|p| p.active().location().clone())
                .collect()
        })
        .unwrap_or_default()
}

fn fresh_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    initial_state(&dir.path().join("session.bin"), home())
}

#[test]
fn given_folders_open_in_the_left_and_right_panes() {
    let mut state = fresh_state();
    let left = Location::new("/fixture/flat").unwrap();
    let right = Location::new("/fixture/nested").unwrap();
    let first = pane_locations(&state);
    assert_eq!(first.len(), 2);
    place_panes(&mut state, [Some(left.clone()), Some(right.clone())]);
    assert_eq!(pane_locations(&state), [left.clone(), right]);
    assert_eq!(state.active_tab().location(), &left);
}

#[test]
fn only_the_given_side_moves() {
    let mut state = fresh_state();
    let right = Location::new("/fixture/nested").unwrap();
    place_panes(&mut state, [None, Some(right.clone())]);
    assert_eq!(pane_locations(&state), [home(), right]);
}

#[test]
fn placed_panes_are_listed_at_start() {
    let mut state = fresh_state();
    let left = Location::new("/fixture/flat").unwrap();
    place_panes(&mut state, [Some(left.clone()), None]);
    let listed: Vec<Location> = rusty_core::list_missing(&mut state)
        .into_iter()
        .filter_map(|effect| match effect {
            rusty_core::Effect::ListFolder { location, .. } => Some(location),
            _ => None,
        })
        .collect();
    assert!(listed.contains(&left), "{listed:?}");
}

#[test]
fn a_relative_folder_becomes_absolute() {
    let location = path_location(std::path::Path::new("fixture")).unwrap();
    let expected = std::env::current_dir().unwrap().join("fixture");
    assert_eq!(location.as_str(), expected.to_str().unwrap());
}

#[test]
fn no_folder_overrides_keep_the_restored_focused_pane() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("session.bin");
    let mut state = initial_state(&file, home());
    let right = pane_ids(&state)[1];
    focus(&mut state, right);
    save(&state, &file).unwrap();
    let mut restored = initial_state(&file, home());
    assert_eq!(restored.focused_pane(), right);
    place_panes(&mut restored, [None, None]);
    assert_eq!(restored.focused_pane(), right);
}

#[test]
fn a_newer_session_starts_at_home_and_stays_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("session.bin");
    save(&initial_state(&file, home()), &file).unwrap();
    let mut newer = std::fs::read(&file).unwrap();
    newer[16..18].copy_from_slice(&(rusty_store::SESSION_VERSION + 1).to_le_bytes());
    std::fs::write(&file, &newer).unwrap();
    let state = initial_state(&file, home());
    assert_eq!(state.active_tab().location(), &home());
    assert_eq!(std::fs::read(&file).unwrap(), newer);
}
