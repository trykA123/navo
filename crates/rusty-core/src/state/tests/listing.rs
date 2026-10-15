use super::{entry, listing};
use crate::Listing;

#[test]
fn entries_keep_their_order() {
    let entries = vec![entry("b", true), entry("a", false)];
    assert_eq!(Listing::new(entries.clone()).entries(), entries.as_slice());
}

#[test]
fn len_counts_the_entries() {
    assert_eq!(listing(3).len(), 3);
}

#[test]
fn empty_listing_is_empty() {
    assert!(listing(0).is_empty());
}

#[test]
fn non_empty_listing_is_not_empty() {
    assert!(!listing(1).is_empty());
}

#[test]
fn get_returns_the_entry_at_the_row() {
    let list = Listing::new(vec![entry("a", false), entry("b", true)]);
    assert_eq!(list.get(1), Some(&entry("b", true)));
}

#[test]
fn get_past_the_end_is_none() {
    assert_eq!(listing(2).get(2), None);
}
