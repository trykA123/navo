use super::{at_work, cursor, fail, files, home, load, marked, mixed, move_cursor, page_rows, run};
use crate::{Command, CommandError, CursorMove, Entry, Listing, SpanMode};

fn extend(state: &mut crate::AppState, to: CursorMove, mode: SpanMode) {
    run(state, &Command::SelectionExtend { to, mode });
}

#[test]
fn shift_sequence_grows_and_shrinks() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Down, SpanMode::Mark);
    extend(&mut state, CursorMove::Down, SpanMode::Mark);
    extend(&mut state, CursorMove::Down, SpanMode::Mark);
    assert_eq!((marked(&state), cursor(&state)), (vec![0, 1, 2, 3], 3));
    extend(&mut state, CursorMove::Up, SpanMode::Mark);
    assert_eq!((marked(&state), cursor(&state)), (vec![0, 1, 2], 2));
}

#[test]
fn extend_resolves_pages_endpoints_and_rows_like_cursor_move() {
    let mut state = at_work(files(12));
    page_rows(&mut state, 4);
    extend(&mut state, CursorMove::PageDown, SpanMode::Mark);
    assert_eq!(cursor(&state), 3);
    extend(&mut state, CursorMove::PageDown, SpanMode::Mark);
    assert_eq!((cursor(&state), state.active_tab().view().scroll), (6, 3));
    extend(&mut state, CursorMove::PageUp, SpanMode::Mark);
    assert_eq!(marked(&state), vec![0, 1, 2, 3]);
    extend(&mut state, CursorMove::Last, SpanMode::Mark);
    assert_eq!(marked(&state), (0..12).collect::<Vec<_>>());
    extend(&mut state, CursorMove::First, SpanMode::Mark);
    assert_eq!((marked(&state), cursor(&state)), (vec![0], 0));
    extend(&mut state, CursorMove::Row(u64::MAX), SpanMode::Mark);
    assert_eq!((marked(&state), cursor(&state)), (vec![], 11));
}

#[test]
fn a_scroll_keeps_the_span_so_moving_back_shrinks_to_the_base() {
    let mut state = at_work(files(8));
    run(&mut state, &Command::SelectionToggle);
    move_cursor(&mut state, CursorMove::Row(2));
    extend(&mut state, CursorMove::Row(5), SpanMode::Mark);
    run(&mut state, &Command::PaneScroll { to: 3 });
    extend(&mut state, CursorMove::Row(3), SpanMode::Mark);
    assert_eq!(marked(&state), vec![0, 2, 3]);
}

#[test]
fn toggle_mode_flips_each_row_against_the_base() {
    let mut state = at_work(files(8));
    run(&mut state, &Command::SelectionToggle);
    extend(&mut state, CursorMove::Row(3), SpanMode::Toggle);
    assert_eq!(marked(&state), vec![1, 2, 3]);
    extend(&mut state, CursorMove::Row(0), SpanMode::Toggle);
    assert!(marked(&state).is_empty());
}

#[test]
fn toggle_ends_the_span() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Row(3), SpanMode::Mark);
    run(&mut state, &Command::SelectionToggle);
    extend(&mut state, CursorMove::Row(5), SpanMode::Mark);
    extend(&mut state, CursorMove::Row(4), SpanMode::Mark);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 4]);
}

#[test]
fn plain_cursor_move_ends_the_span_and_keeps_marks() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Row(3), SpanMode::Mark);
    move_cursor(&mut state, CursorMove::Row(6));
    assert_eq!(marked(&state), vec![0, 1, 2, 3]);
    extend(&mut state, CursorMove::Row(7), SpanMode::Mark);
    extend(&mut state, CursorMove::Row(6), SpanMode::Mark);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 6]);
}

#[test]
fn listing_reload_ends_the_span() {
    let mut state = at_work(files(8));
    extend(&mut state, CursorMove::Row(3), SpanMode::Mark);
    let tab = state.active_tab();
    let event = crate::Event::ListingFiltered {
        tab: tab.id(),
        location: tab.location().clone(),
        listing: files(8),
    };
    crate::apply_event(&mut state, event);
    extend(&mut state, CursorMove::Row(5), SpanMode::Mark);
    extend(&mut state, CursorMove::Row(4), SpanMode::Mark);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 4]);
}

#[test]
fn extend_unmarks_and_shrinks() {
    let mut state = at_work(files(8));
    run(&mut state, &Command::SelectionAll);
    move_cursor(&mut state, CursorMove::Row(2));
    extend(&mut state, CursorMove::Row(5), SpanMode::Unmark);
    assert_eq!(marked(&state), vec![0, 1, 6, 7]);
    extend(&mut state, CursorMove::Row(3), SpanMode::Unmark);
    assert_eq!(marked(&state), vec![0, 1, 4, 5, 6, 7]);
}

#[test]
fn extend_requires_a_nonempty_listing() {
    let command = Command::SelectionExtend {
        to: CursorMove::Down,
        mode: SpanMode::Mark,
    };
    assert_eq!(fail(&mut home(), &command), CommandError::NoListing);
    assert_eq!(
        fail(&mut at_work(files(0)), &command),
        CommandError::EmptyListing
    );
}

#[test]
fn extension_marks_and_unmarks_the_cursor_file_suffix() {
    let mut state = at_work(mixed());
    move_cursor(&mut state, CursorMove::Row(1));
    run(&mut state, &Command::SelectionExtension { mark: true });
    assert_eq!(marked(&state), vec![1, 4]);
    run(&mut state, &Command::SelectionExtension { mark: false });
    assert!(marked(&state).is_empty());
}

#[test]
fn extension_on_a_folder_leaves_marks_unchanged() {
    let mut state = at_work(mixed());
    run(&mut state, &Command::SelectionAll);
    move_cursor(&mut state, CursorMove::Row(3));
    for mark in [true, false] {
        assert_eq!(
            run(&mut state, &Command::SelectionExtension { mark }),
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
    run(&mut state, &Command::SelectionExtension { mark: true });
    assert_eq!(marked(&state), vec![0, 1]);
}

#[test]
fn extension_requires_a_nonempty_listing() {
    let command = Command::SelectionExtension { mark: true };
    assert_eq!(fail(&mut home(), &command), CommandError::NoListing);
    assert_eq!(
        fail(&mut at_work(files(0)), &command),
        CommandError::EmptyListing
    );
}

#[test]
fn loaded_listing_reload_ends_the_span() {
    let mut state = at_work(files(8));
    run(&mut state, &Command::PaneRefresh);
    extend(&mut state, CursorMove::Row(3), SpanMode::Mark);
    load(&mut state, files(8));
    extend(&mut state, CursorMove::Row(5), SpanMode::Mark);
    extend(&mut state, CursorMove::Row(4), SpanMode::Mark);
    assert_eq!(marked(&state), vec![0, 1, 2, 3, 4]);
}
