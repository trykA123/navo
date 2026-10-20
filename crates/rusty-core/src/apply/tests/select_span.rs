use super::{at_work, cursor, fail, files, home, load, marked, mixed, move_cursor, page_rows, run};
use crate::{Command, CommandError, CursorMove, Entry, Listing};

fn extend(state: &mut crate::AppState, to: CursorMove, unmark: bool) {
    run(state, Command::SelectionExtend { to, unmark });
}

#[test]
fn shift_sequence_grows_and_shrinks() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Down, false);
    extend(&mut state, CursorMove::Down, false);
    extend(&mut state, CursorMove::Down, false);
    assert_eq!((marked(&state), cursor(&state)), (vec![0, 1, 2, 3], 3));
    extend(&mut state, CursorMove::Up, false);
    assert_eq!((marked(&state), cursor(&state)), (vec![0, 1, 2], 2));
}

#[test]
fn extend_resolves_pages_endpoints_and_rows_like_cursor_move() {
    let mut state = at_work(files(12));
    page_rows(&mut state, 4);
    extend(&mut state, CursorMove::PageDown, false);
    assert_eq!(cursor(&state), 3);
    extend(&mut state, CursorMove::PageDown, false);
    assert_eq!((cursor(&state), state.active_tab().view().scroll), (6, 3));
    extend(&mut state, CursorMove::PageUp, false);
    assert_eq!(marked(&state), vec![0, 1, 2, 3]);
    extend(&mut state, CursorMove::Last, false);
    assert_eq!(marked(&state), (0..12).collect::<Vec<_>>());
    extend(&mut state, CursorMove::First, false);
    assert_eq!((marked(&state), cursor(&state)), (vec![0], 0));
    extend(&mut state, CursorMove::Row(u64::MAX), false);
    assert_eq!(cursor(&state), 11);
}

#[test]
fn toggle_ends_the_span() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Row(3), false);
    run(&mut state, Command::SelectionToggle);
    extend(&mut state, CursorMove::Row(5), false);
    extend(&mut state, CursorMove::Row(4), false);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 4]);
}

#[test]
fn plain_cursor_move_ends_the_span_and_keeps_marks() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Row(3), false);
    move_cursor(&mut state, CursorMove::Row(6));
    assert_eq!(marked(&state), vec![0, 1, 2, 3]);
    extend(&mut state, CursorMove::Row(7), false);
    extend(&mut state, CursorMove::Row(6), false);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 6]);
}

#[test]
fn listing_reload_ends_the_span() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Row(3), false);
    let tab = state.active_tab();
    let event = crate::Event::ListingFiltered {
        tab: tab.id(),
        location: tab.location().clone(),
        listing: files(8),
    };
    crate::apply_event(&mut state, event);
    extend(&mut state, CursorMove::Row(5), false);
    extend(&mut state, CursorMove::Row(4), false);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 4]);
}

#[test]
fn extend_unmarks_and_shrinks() {
    let mut state = at_work(files(8));
    run(&mut state, Command::SelectionAll);
    move_cursor(&mut state, CursorMove::Row(2));
    extend(&mut state, CursorMove::Row(5), true);
    assert_eq!(marked(&state), vec![0, 1, 6, 7]);
    extend(&mut state, CursorMove::Row(3), true);
    assert_eq!(marked(&state), vec![0, 1, 4, 5, 6, 7]);
}

#[test]
fn extend_requires_a_nonempty_listing() {
    let command = Command::SelectionExtend {
        to: CursorMove::Down,
        unmark: false,
    };
    assert_eq!(fail(&mut home(), command.clone()), CommandError::NoListing);
    assert_eq!(
        fail(&mut at_work(files(0)), command),
        CommandError::EmptyListing
    );
}

#[test]
fn extension_marks_and_unmarks_the_cursor_file_suffix() {
    let mut state = at_work(mixed());
    move_cursor(&mut state, CursorMove::Row(1));
    run(&mut state, Command::SelectionExtension { mark: true });
    assert_eq!(marked(&state), vec![1, 4]);
    run(&mut state, Command::SelectionExtension { mark: false });
    assert!(marked(&state).is_empty());
}

#[test]
fn extension_on_a_folder_leaves_marks_unchanged() {
    let mut state = at_work(mixed());
    run(&mut state, Command::SelectionAll);
    move_cursor(&mut state, CursorMove::Row(3));
    for mark in [true, false] {
        assert_eq!(
            run(&mut state, Command::SelectionExtension { mark }),
            vec![]
        );
        assert_eq!(marked(&state), vec![0, 1, 2, 3, 4]);
    }
}

#[test]
fn extension_on_a_file_without_a_suffix_marks_other_unsuffixed_files() {
    let mut state = at_work(Listing::new(vec![
        Entry {
            name: ".profile".into(),
            is_folder: false,
        },
        Entry {
            name: "README".into(),
            is_folder: false,
        },
        Entry {
            name: "a.txt".into(),
            is_folder: false,
        },
    ]));
    run(&mut state, Command::SelectionExtension { mark: true });
    assert_eq!(marked(&state), vec![0, 1]);
}

#[test]
fn extension_requires_a_nonempty_listing() {
    let command = Command::SelectionExtension { mark: true };
    assert_eq!(fail(&mut home(), command.clone()), CommandError::NoListing);
    assert_eq!(
        fail(&mut at_work(files(0)), command),
        CommandError::EmptyListing
    );
}

#[test]
fn loaded_listing_reload_ends_the_span() {
    let mut state = at_work(files(8));
    run(&mut state, Command::PaneRefresh);
    extend(&mut state, CursorMove::Row(3), false);
    load(&mut state, files(8));
    extend(&mut state, CursorMove::Row(5), false);
    extend(&mut state, CursorMove::Row(4), false);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 4]);
}
