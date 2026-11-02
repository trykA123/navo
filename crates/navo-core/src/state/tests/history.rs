use super::{loc, tab};
use crate::{History, Location, Tab, ViewState};

fn numbered(i: usize) -> Location {
    loc(&format!("/d{i}"))
}

fn walk(count: usize) -> Tab {
    let mut t = tab(1, "/d0");
    for i in 1..=count {
        t.navigate(numbered(i));
    }
    t
}

#[test]
fn limit_is_one_hundred() {
    assert_eq!(History::LIMIT, 100);
}

#[test]
fn back_keeps_at_most_the_limit() {
    assert_eq!(walk(150).history().back().len(), History::LIMIT);
}

#[test]
fn back_drops_the_oldest_locations_first() {
    let expected: Vec<Location> = (50..150).map(numbered).collect();
    assert_eq!(walk(150).history().back(), expected.as_slice());
}

#[test]
fn back_keeps_a_location_until_it_passes_the_limit() {
    assert_eq!(walk(100).history().back().first(), Some(&numbered(0)));
}

#[test]
fn a_view_is_kept_while_its_location_is_in_back() {
    let mut t = tab(1, "/d0");
    t.set_cursor(5);
    for i in 1..=100 {
        t.navigate(numbered(i));
    }
    t.navigate(numbered(0));
    assert_eq!(t.view().cursor, 5);
}

#[test]
fn a_view_is_forgotten_when_its_location_leaves_back() {
    let mut t = tab(1, "/d0");
    t.set_cursor(5);
    for i in 1..=101 {
        t.navigate(numbered(i));
    }
    t.navigate(numbered(0));
    assert_eq!(t.view(), ViewState::default());
}

#[test]
fn a_view_is_kept_while_its_location_is_in_forward() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.set_cursor(7);
    t.back().unwrap();
    t.forward().unwrap();
    assert_eq!(t.view().cursor, 7);
}

#[test]
fn a_view_is_forgotten_when_forward_is_cleared() {
    let mut t = tab(1, "/a");
    t.navigate(loc("/b"));
    t.set_cursor(7);
    t.back().unwrap();
    t.navigate(loc("/c"));
    t.navigate(loc("/b"));
    assert_eq!(t.view(), ViewState::default());
}
