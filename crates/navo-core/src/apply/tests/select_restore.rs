use super::{at_work, cursor, entry, fail, files, home, load, marked, move_cursor, navigate, run};
use crate::{AppState, Command, CommandError, CursorMove, Direction, Listing};

#[test]
fn restore_after_clear_adds_saved_marks_to_existing_marks() {
    let mut state = at_work(files(5));
    move_cursor(&mut state, CursorMove::Row(1));
    run(&mut state, &Command::SelectionToggle);
    run(&mut state, &Command::SelectionClear);
    move_cursor(&mut state, CursorMove::Row(3));
    run(&mut state, &Command::SelectionToggle);
    assert_eq!(run(&mut state, &Command::SelectionRestore), vec![]);
    assert_eq!((marked(&state), cursor(&state)), (vec![1, 3], 3));
}

#[test]
fn clearing_an_empty_selection_preserves_saved_marks() {
    let mut state = at_work(files(3));
    run(&mut state, &Command::SelectionAll);
    run(&mut state, &Command::SelectionClear);
    run(&mut state, &Command::SelectionClear);
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(marked(&state), vec![0, 1, 2]);
}

#[test]
fn leaving_a_folder_restores_by_name_in_another_folder() {
    let mut state = at_work(files(3));
    run(&mut state, &Command::SelectionAll);
    navigate(&mut state, "/copies");
    load(
        &mut state,
        Listing::new(vec![entry("other", false), entry("f1.txt", false)]),
    );
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(marked(&state), vec![1]);
}

#[test]
fn back_and_forward_save_marks_when_leaving() {
    let mut state = at_work(files(3));
    navigate(&mut state, "/copies");
    load(&mut state, files(3));
    move_cursor(&mut state, CursorMove::Row(2));
    run(&mut state, &Command::SelectionToggle);
    run(&mut state, &Command::PaneBack);
    load(&mut state, files(3));
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(marked(&state), vec![2]);
    move_cursor(&mut state, CursorMove::Row(0));
    run(&mut state, &Command::SelectionToggle);
    run(&mut state, &Command::PaneForward);
    load(&mut state, files(3));
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(marked(&state), vec![0, 2]);
}

#[test]
fn restore_without_saved_names_or_matches_reports_nothing_to_restore() {
    let mut state = at_work(files(3));
    assert_eq!(
        fail(&mut state, &Command::SelectionRestore),
        CommandError::NothingToRestore
    );
    run(&mut state, &Command::SelectionAll);
    run(&mut state, &Command::SelectionClear);
    navigate(&mut state, "/empty");
    load(&mut state, Listing::new(vec![entry("other", false)]));
    run(&mut state, &Command::SelectionToggle);
    assert_eq!(
        fail(&mut state, &Command::SelectionRestore),
        CommandError::NothingToRestore
    );
    assert_eq!(marked(&state), vec![0]);
    assert_eq!(
        CommandError::NothingToRestore.to_string(),
        "No saved marks match this folder"
    );
}

#[test]
fn restore_requires_a_listing() {
    assert_eq!(
        fail(&mut home(), &Command::SelectionRestore),
        CommandError::NoListing
    );
}

#[test]
fn saved_marks_are_shared_by_panes_in_one_window() {
    let mut state = at_work(files(3));
    run(&mut state, &Command::SelectionAll);
    run(&mut state, &Command::SelectionClear);
    run(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    load(&mut state, files(3));
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(marked(&state), vec![0, 1, 2]);
}

#[test]
fn saved_marks_are_isolated_between_windows() {
    let mut state = at_work(files(3));
    let first = state.focused_window();
    run(&mut state, &Command::SelectionToggle);
    run(&mut state, &Command::SelectionClear);
    run(&mut state, &Command::WindowOpen { location: None });
    load(&mut state, files(3));
    assert_eq!(
        fail(&mut state, &Command::SelectionRestore),
        CommandError::NothingToRestore
    );
    move_cursor(&mut state, CursorMove::Row(2));
    run(&mut state, &Command::SelectionToggle);
    navigate(&mut state, "/copies");
    load(&mut state, files(3));
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(marked(&state), vec![2]);
    run(
        &mut state,
        &Command::WindowFocus {
            window: first.get(),
        },
    );
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(marked(&state), vec![0]);
}

#[test]
fn saved_marks_survive_the_session_in_the_workspace_snapshot() {
    let mut state = at_work(files(3));
    run(&mut state, &Command::SelectionAll);
    run(&mut state, &Command::SelectionClear);
    let mut restored = AppState::from_session(&state.to_session(100)).unwrap();
    load(&mut restored, files(3));
    run(&mut restored, &Command::SelectionRestore);
    assert_eq!(marked(&restored), vec![0, 1, 2]);
}

#[test]
fn clear_caps_saved_names_at_one_hundred_thousand() {
    let mut state = at_work(files(100_005));
    run(&mut state, &Command::SelectionAll);
    run(&mut state, &Command::SelectionClear);
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(state.active_tab().selection().count(), 100_000);
    assert!(state.active_tab().selection().is_marked(99_999));
    assert!(!state.active_tab().selection().is_marked(100_000));
}

#[test]
fn leaving_a_folder_caps_saved_names_at_one_hundred_thousand() {
    let mut state = at_work(files(100_005));
    run(&mut state, &Command::SelectionAll);
    navigate(&mut state, "/copies");
    load(&mut state, files(100_005));
    run(&mut state, &Command::SelectionRestore);
    assert_eq!(state.active_tab().selection().count(), 100_000);
}
