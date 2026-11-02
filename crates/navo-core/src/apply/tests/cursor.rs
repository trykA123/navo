use super::{at_work, cursor, fail, files, home, move_cursor, navigate, run};
use crate::{AppState, Command, CommandError, CursorMove};

fn at_row(len: usize, row: u64) -> AppState {
    let mut state = at_work(files(len));
    move_cursor(&mut state, CursorMove::Row(row));
    state
}

#[test]
fn row_moves_the_cursor_to_that_row() {
    assert_eq!(cursor(&at_row(10, 4)), 4);
}

#[test]
fn row_past_the_end_clamps_to_the_last_row() {
    assert_eq!(cursor(&at_row(10, 99)), 9);
}

#[test]
fn down_moves_one_row_down() {
    let mut state = at_row(10, 4);
    move_cursor(&mut state, CursorMove::Down);
    assert_eq!(cursor(&state), 5);
}

#[test]
fn down_on_the_last_row_stays() {
    let mut state = at_row(10, 9);
    move_cursor(&mut state, CursorMove::Down);
    assert_eq!(cursor(&state), 9);
}

#[test]
fn up_moves_one_row_up() {
    let mut state = at_row(10, 4);
    move_cursor(&mut state, CursorMove::Up);
    assert_eq!(cursor(&state), 3);
}

#[test]
fn up_on_the_first_row_stays() {
    let mut state = at_row(10, 0);
    move_cursor(&mut state, CursorMove::Up);
    assert_eq!(cursor(&state), 0);
}

#[test]
fn first_moves_to_the_first_row() {
    let mut state = at_row(10, 6);
    move_cursor(&mut state, CursorMove::First);
    assert_eq!(cursor(&state), 0);
}

#[test]
fn last_moves_to_the_last_row() {
    let mut state = at_row(10, 2);
    move_cursor(&mut state, CursorMove::Last);
    assert_eq!(cursor(&state), 9);
}

#[test]
fn page_down_moves_one_row_less_than_the_default_twenty() {
    let mut state = at_row(50, 0);
    move_cursor(&mut state, CursorMove::PageDown);
    assert_eq!(cursor(&state), 19);
}

#[test]
fn page_down_clamps_to_the_last_row() {
    let mut state = at_row(50, 40);
    move_cursor(&mut state, CursorMove::PageDown);
    assert_eq!(cursor(&state), 49);
}

#[test]
fn page_up_moves_one_row_less_than_the_default_twenty() {
    let mut state = at_row(50, 30);
    move_cursor(&mut state, CursorMove::PageUp);
    assert_eq!(cursor(&state), 11);
}

#[test]
fn page_up_clamps_to_the_first_row() {
    let mut state = at_row(50, 5);
    move_cursor(&mut state, CursorMove::PageUp);
    assert_eq!(cursor(&state), 0);
}

#[test]
fn cursor_move_has_no_effects() {
    let mut state = at_work(files(10));
    assert_eq!(
        run(
            &mut state,
            &Command::PaneCursorMove {
                to: CursorMove::Down
            }
        ),
        vec![]
    );
}

#[test]
fn cursor_move_without_a_listing_fails() {
    let mut state = home();
    navigate(&mut state, "/work");
    assert_eq!(
        fail(
            &mut state,
            &Command::PaneCursorMove {
                to: CursorMove::Down
            }
        ),
        CommandError::NoListing
    );
}

#[test]
fn cursor_move_on_an_empty_listing_fails() {
    let mut state = at_work(files(0));
    assert_eq!(
        fail(
            &mut state,
            &Command::PaneCursorMove {
                to: CursorMove::Last
            }
        ),
        CommandError::EmptyListing
    );
}
