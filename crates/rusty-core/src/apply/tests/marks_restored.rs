use super::{at_work, cursor, files, home, marked, move_cursor, run};
use crate::{AppState, Command, CursorMove, Event, TabId, apply_event};

fn restore(state: &mut AppState, tab: TabId, names: &[&str]) {
    let names = names.iter().map(|n| (*n).to_owned()).collect();
    assert_eq!(
        apply_event(state, Event::MarksRestored { tab, names }),
        vec![]
    );
}

fn active(state: &AppState) -> TabId {
    state.active_tab().id()
}

#[test]
fn restored_names_add_to_the_existing_marks() {
    let mut state = at_work(files(5));
    move_cursor(&mut state, CursorMove::Row(1));
    run(&mut state, Command::SelectionToggle);
    let tab = active(&state);
    restore(&mut state, tab, &["f3.txt", "f4.txt"]);
    assert_eq!(marked(&state), vec![1, 3, 4]);
}

#[test]
fn names_missing_from_the_listing_are_ignored() {
    let mut state = at_work(files(3));
    let tab = active(&state);
    restore(&mut state, tab, &["gone.txt", "f2.txt"]);
    assert_eq!(marked(&state), vec![2]);
}

#[test]
fn restoring_marks_leaves_the_cursor_alone() {
    let mut state = at_work(files(5));
    move_cursor(&mut state, CursorMove::Row(2));
    let tab = active(&state);
    restore(&mut state, tab, &["f0.txt"]);
    assert_eq!(cursor(&state), 2);
}

#[test]
fn restoring_marks_ends_the_span() {
    let mut state = at_work(files(6));
    run(
        &mut state,
        Command::SelectionExtend {
            to: CursorMove::Down,
            unmark: false,
        },
    );
    let tab = active(&state);
    restore(&mut state, tab, &["f5.txt"]);
    run(
        &mut state,
        Command::SelectionExtend {
            to: CursorMove::Down,
            unmark: false,
        },
    );
    assert_eq!(marked(&state), vec![0, 1, 2, 5]);
}

#[test]
fn restoring_marks_on_an_unknown_tab_or_unloaded_tab_does_nothing() {
    let mut state = home();
    let tab = active(&state);
    restore(&mut state, tab, &["x"]);
    restore(&mut state, TabId::new(999), &["x"]);
    assert_eq!(marked(&state), Vec::<u64>::new());
}
