use super::{loc, navigate, run};
use crate::{AppState, Command, Effect, Location};

fn start(at: &str, then: &[&str]) -> AppState {
    let mut state = AppState::new(loc(at));
    for location in then {
        navigate(&mut state, location);
    }
    state
}

fn back(state: &mut AppState, times: usize) {
    for _ in 0..times {
        run(state, &Command::PaneBack);
    }
}

fn go_drive(state: &mut AppState, root: &str) -> Vec<Effect> {
    run(state, &Command::PaneGoDrive { root: loc(root) })
}

fn location(state: &AppState) -> &Location {
    state.active_tab().location()
}

fn last_back(state: &AppState) -> Option<&Location> {
    state.active_tab().history().back().last()
}

#[test]
fn on_the_current_drive_it_goes_to_the_root() {
    let mut state = start(r"C:\Users\admin", &[]);
    go_drive(&mut state, r"C:\");
    assert_eq!(location(&state), &loc(r"C:\"));
}

#[test]
fn on_the_current_drive_it_ignores_older_folders_there() {
    let mut state = start(r"C:\old", &[r"D:\x", r"C:\Users\admin"]);
    go_drive(&mut state, r"C:\");
    assert_eq!(location(&state), &loc(r"C:\"));
}

#[test]
fn the_current_drive_matches_in_any_letter_case() {
    let mut state = start(r"c:\users\admin", &[]);
    go_drive(&mut state, r"C:\");
    assert_eq!(location(&state), &loc(r"C:\"));
}

#[test]
fn going_to_a_drive_pushes_the_previous_folder_to_back_history() {
    let mut state = start(r"C:\Users\admin", &[]);
    go_drive(&mut state, r"D:\");
    assert_eq!(last_back(&state), Some(&loc(r"C:\Users\admin")));
}

#[test]
fn another_drive_returns_to_its_newest_folder_in_back_history() {
    let mut state = start(r"C:\start", &[r"D:\games\x", r"D:\games\y", r"C:\work"]);
    go_drive(&mut state, r"D:\");
    assert_eq!(location(&state), &loc(r"D:\games\y"));
}

#[test]
fn back_history_matches_the_drive_in_any_letter_case() {
    let mut state = start(r"C:\start", &[r"d:\old", r"C:\work"]);
    go_drive(&mut state, r"D:\");
    assert_eq!(location(&state), &loc(r"d:\old"));
}

#[test]
fn back_history_wins_over_forward_history() {
    let mut state = start(r"D:\old", &[r"C:\a", r"D:\newer", r"C:\b"]);
    back(&mut state, 2);
    go_drive(&mut state, r"D:\");
    assert_eq!(location(&state), &loc(r"D:\old"));
}

#[test]
fn forward_history_is_searched_nearest_first() {
    let mut state = start(r"C:\a", &[r"D:\near", r"D:\far"]);
    back(&mut state, 2);
    go_drive(&mut state, r"D:\");
    assert_eq!(location(&state), &loc(r"D:\near"));
}

#[test]
fn a_drive_never_visited_opens_its_root() {
    let mut state = start(r"C:\start", &[r"E:\elsewhere", r"C:\work"]);
    go_drive(&mut state, r"D:\");
    assert_eq!(location(&state), &loc(r"D:\"));
}

#[test]
fn a_share_returns_to_its_newest_folder() {
    let mut state = start(r"C:\start", &[r"\\nas\media\films", r"C:\work"]);
    go_drive(&mut state, r"\\nas\media\");
    assert_eq!(location(&state), &loc(r"\\nas\media\films"));
}

#[test]
fn a_unix_root_on_the_current_root_goes_to_slash() {
    let mut state = start("/home/admin", &[]);
    go_drive(&mut state, "/");
    assert_eq!(location(&state), &loc("/"));
}

#[test]
fn it_behaves_exactly_like_navigating_to_the_found_folder() {
    let mut driven = start(r"C:\start", &[r"D:\games\y", r"C:\work"]);
    let mut navigated = driven.clone();
    let effects = go_drive(&mut driven, r"D:\");
    let expected = navigate(&mut navigated, r"D:\games\y");
    assert_eq!((effects, driven), (expected, navigated));
}
