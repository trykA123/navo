use super::{listing, loc, tab};
use crate::{StateError, TabId, ViewState};

fn view(cursor: u64, scroll: u64) -> ViewState {
    ViewState { cursor, scroll }
}

#[test]
fn new_tab_has_its_id_and_location() {
    let t = tab(7, "/home");
    assert_eq!((t.id(), t.location()), (TabId::new(7), &loc("/home")));
}

#[test]
fn entry_location_joins_relative_segments_under_the_base() {
    let mut t = tab(1, "find:1");
    t.set_base(Some(loc(r"C:\work")));
    for name in [r"src\a.c", "src/a.c", r"src/nested\a.c"] {
        let expected = format!(r"C:\work\{}", name.replace('/', r"\"));
        assert_eq!(t.entry_location(name), Ok(loc(&expected)));
    }
}

#[test]
fn absolute_entry_locations_ignore_the_base() {
    let mut t = tab(1, "find:1");
    for base in [Some(loc(r"C:\work")), Some(loc("/"))] {
        t.set_base(base);
        for name in [r"D:\x\y.c", "D:/x/y.c", r"\\server\share\x\y.c"] {
            assert_eq!(t.entry_location(name), Ok(loc(name)));
        }
    }
}

#[test]
fn ordinary_entry_location_uses_the_tabs_folder() {
    let mut t = tab(1, r"C:\work");
    assert_eq!(t.base_or_location(), t.location());
    assert_eq!(t.entry_location("a.c"), Ok(loc(r"C:\work\a.c")));
    t.set_base(Some(loc(r"D:\other")));
    t.set_base(None);
    assert_eq!(t.entry_location("a.c"), Ok(loc(r"C:\work\a.c")));
}

#[test]
fn entry_location_refuses_empty_and_dot_segments() {
    let mut t = tab(1, "find:1");
    t.set_base(Some(loc(r"C:\work")));
    for name in [
        "",
        ".",
        "..",
        r"src\..\a.c",
        "src/./a.c",
        "src//a.c",
        "src/",
    ] {
        assert!(t.entry_location(name).is_err(), "{name:?}");
    }
}

#[test]
fn session_round_trip_drops_the_runtime_base() {
    let mut state = crate::AppState::new(loc(r"C:\work"));
    state.active_tab_mut().set_base(Some(loc(r"D:\results")));
    let bytes = postcard::to_stdvec(&state.to_session(0)).unwrap();
    let saved = postcard::from_bytes(&bytes).unwrap();
    let restored = crate::AppState::from_session(saved).unwrap();
    assert_eq!(restored, crate::AppState::new(loc(r"C:\work")));
    assert_eq!(restored.active_tab().base_or_location(), &loc(r"C:\work"));
}

#[test]
fn new_tab_has_the_default_view() {
    assert_eq!(tab(1, "/home").view(), ViewState::default());
}

#[test]
fn new_tab_has_no_listing() {
    assert_eq!(tab(1, "/home").listing(), None);
}

#[test]
fn new_tab_has_an_empty_history() {
    let t = tab(1, "/home");
    assert!(t.history().back().is_empty() && t.history().forward().is_empty());
}

#[test]
fn navigate_sets_the_new_location() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    assert_eq!(t.location(), &loc("/b"));
}

#[test]
fn navigate_pushes_the_old_location_on_back() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.navigate(loc("/c"));
    assert_eq!(t.history().back(), &[loc("/a"), loc("/b")]);
}

#[test]
fn navigate_clears_forward() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.back().unwrap();
    t.navigate(loc("/c"));
    assert!(t.history().forward().is_empty());
}

#[test]
fn navigate_drops_the_listing() {
    let mut t = tab(1, "/a");
    t.set_listing(listing(3));
    t.navigate(loc("/b"));
    assert_eq!(t.listing(), None);
}

#[test]
fn navigate_to_a_new_location_gives_the_default_view() {
    let mut t = tab(1, "/a");
    t.set_cursor(5);
    t.navigate(loc("/b"));
    assert_eq!(t.view(), ViewState::default());
}

#[test]
fn navigate_restores_the_view_remembered_for_the_location() {
    let mut t = tab(1, "/a");
    t.set_cursor(5);
    t.set_scroll(2);
    t.navigate(loc("/b"));
    t.navigate(loc("/a"));
    assert_eq!(t.view(), view(5, 2));
}

#[test]
fn back_returns_to_the_previous_location() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.back().unwrap();
    assert_eq!(t.location(), &loc("/a"));
}

#[test]
fn back_moves_the_current_location_to_forward() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.back().unwrap();
    assert_eq!(
        (t.history().back(), t.history().forward()),
        (&[][..], &[loc("/b")][..])
    );
}

#[test]
fn forward_lists_the_next_location_first() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.navigate(loc("/c"));
    t.back().unwrap();
    t.back().unwrap();
    assert_eq!(t.history().forward(), &[loc("/b"), loc("/c")]);
}

#[test]
fn back_restores_the_view_of_the_previous_location() {
    let mut t = tab(1, "/a");
    t.set_cursor(4);
    t.navigate(loc("/b"));
    t.set_cursor(9);
    t.back().unwrap();
    assert_eq!(t.view(), view(4, 0));
}

#[test]
fn back_drops_the_listing() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.set_listing(listing(3));
    t.back().unwrap();
    assert_eq!(t.listing(), None);
}

#[test]
fn back_with_an_empty_history_is_nothing_back() {
    assert_eq!(tab(1, "/a").back(), Err(StateError::NothingBack));
}

#[test]
fn forward_returns_to_the_next_location() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.back().unwrap();
    t.forward().unwrap();
    assert_eq!(t.location(), &loc("/b"));
}

#[test]
fn forward_keeps_the_back_stack() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.navigate(loc("/c"));
    t.back().unwrap();
    t.back().unwrap();
    t.forward().unwrap();
    assert_eq!(
        (t.history().back(), t.history().forward()),
        (&[loc("/a")][..], &[loc("/c")][..])
    );
}

#[test]
fn forward_restores_the_view_of_the_next_location() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.set_scroll(6);
    t.back().unwrap();
    t.forward().unwrap();
    assert_eq!(t.view(), view(0, 6));
}

#[test]
fn forward_drops_the_listing() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.back().unwrap();
    t.set_listing(listing(3));
    t.forward().unwrap();
    assert_eq!(t.listing(), None);
}

#[test]
fn forward_with_nothing_ahead_is_nothing_forward() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    assert_eq!(t.forward(), Err(StateError::NothingForward));
}

#[test]
fn set_listing_stores_the_listing() {
    let mut t = tab(1, "/a");
    t.set_listing(listing(3));
    assert_eq!(t.listing(), Some(&listing(3)));
}

#[test]
fn set_listing_clamps_the_cursor_to_the_last_row() {
    let mut t = tab(1, "/a");
    t.set_cursor(10);
    t.set_listing(listing(4));
    assert_eq!(t.view().cursor, 3);
}

#[test]
fn set_listing_clamps_the_scroll_to_the_last_row() {
    let mut t = tab(1, "/a");
    t.set_scroll(10);
    t.set_listing(listing(4));
    assert_eq!(t.view().scroll, 3);
}

#[test]
fn set_listing_keeps_a_view_inside_the_listing() {
    let mut t = tab(1, "/a");
    t.set_cursor(2);
    t.set_scroll(1);
    t.set_listing(listing(4));
    assert_eq!(t.view(), view(2, 1));
}

#[test]
fn an_empty_listing_clamps_the_view_to_zero() {
    let mut t = tab(1, "/a");
    t.set_cursor(5);
    t.set_scroll(5);
    t.set_listing(listing(0));
    assert_eq!(t.view(), ViewState::default());
}

#[test]
fn set_cursor_clamps_to_the_last_row() {
    let mut t = tab(1, "/a");
    t.set_listing(listing(4));
    t.set_cursor(99);
    assert_eq!(t.view().cursor, 3);
}

#[test]
fn set_scroll_clamps_to_the_last_row() {
    let mut t = tab(1, "/a");
    t.set_listing(listing(4));
    t.set_scroll(99);
    assert_eq!(t.view().scroll, 3);
}

#[test]
fn set_cursor_on_an_empty_listing_is_zero() {
    let mut t = tab(1, "/a");
    t.set_listing(listing(0));
    t.set_cursor(3);
    assert_eq!(t.view().cursor, 0);
}

#[test]
fn set_cursor_without_a_listing_is_not_clamped() {
    let mut t = tab(1, "/a");
    t.set_cursor(99);
    assert_eq!(t.view().cursor, 99);
}

#[test]
fn set_scroll_without_a_listing_is_not_clamped() {
    let mut t = tab(1, "/a");
    t.set_scroll(99);
    assert_eq!(t.view().scroll, 99);
}

#[test]
fn new_tab_has_an_empty_selection() {
    assert_eq!(tab(1, "/a").selection().len(), 0);
}

#[test]
fn set_listing_gives_a_fresh_selection_of_its_length() {
    let mut t = tab(1, "/a");
    t.set_listing(listing(4));
    t.selection_mut().toggle(2);
    t.set_listing(listing(6));
    assert_eq!((t.selection().len(), t.selection().count()), (6, 0));
}

#[test]
fn navigate_clears_the_selection() {
    let mut t = tab(1, "/a");
    t.set_listing(listing(4));
    t.selection_mut().toggle(1);
    t.navigate(loc("/b"));
    assert!(t.selection().is_empty());
}
