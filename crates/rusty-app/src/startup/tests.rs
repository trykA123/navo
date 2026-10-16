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
