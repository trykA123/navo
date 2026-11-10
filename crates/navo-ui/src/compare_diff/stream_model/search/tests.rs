//! Tests for the Stream model's search state: progress, counts, matched hunks, clear, restart.

use super::super::tests::{give, model};
use crate::compare_diff::hooks::StreamUpdate;
use crate::compare_diff::stream_model::StreamModel;

fn hits(m: &mut StreamModel, query: u64, hits: Vec<(u64, Vec<u32>)>, scan: (u32, u32, bool)) {
    m.apply(&StreamUpdate::Hits {
        generation: 1,
        query,
        hits,
        searched: scan.0,
        total: scan.1,
        done: scan.2,
    });
}

#[test]
fn progress_counts_and_matched_hunks_follow_the_hits() {
    let mut m = model(&[1, 2, 3]);
    assert_eq!(m.search_progress(), None);
    m.begin_search(1);
    assert_eq!(m.search_progress(), None);
    assert_eq!(m.match_counts(), (0, 0));
    hits(&mut m, 1, vec![(1, vec![0, 2]), (3, vec![])], (2, 3, false));
    assert_eq!(m.search_progress(), Some((2, 3)));
    assert_eq!(m.match_counts(), (2, 2));
    assert_eq!(m.matched_hunks(1), Some(&[0, 2][..]));
    assert_eq!(m.matched_hunks(3), None);
    assert_eq!(m.matched_hunks(2), None);
    assert!(!m.search_done());
    hits(&mut m, 1, vec![(1, vec![0, 2]), (3, vec![])], (3, 3, true));
    assert_eq!(m.search_progress(), None);
    assert!(m.search_done());
}

#[test]
fn clearing_ends_the_narrowing_and_drops_late_hits() {
    let mut m = model(&[1, 2]);
    give(&mut m, 1, 2);
    m.begin_search(4);
    hits(&mut m, 4, vec![(2, vec![])], (2, 2, true));
    assert_eq!(m.visible(), [1]);
    m.clear_search();
    assert!(!m.searching() && !m.search_done());
    assert_eq!(m.visible(), [0, 1]);
    hits(&mut m, 4, vec![(2, vec![])], (2, 2, true));
    assert!(!m.searching());
}

#[test]
fn a_restarted_search_keeps_its_hits_until_the_new_ones_come() {
    let mut m = model(&[1, 2, 3]);
    m.begin_search(1);
    hits(&mut m, 1, vec![(1, vec![]), (2, vec![])], (3, 3, true));
    m.restart_search(2);
    assert_eq!(m.visible(), [0, 1]);
    assert_eq!(m.search_progress(), None);
    hits(&mut m, 1, vec![(3, vec![])], (3, 3, true));
    assert_eq!(m.visible(), [0, 1]);
    hits(&mut m, 2, vec![(3, vec![])], (1, 3, false));
    assert_eq!(m.visible(), [2]);
    hits(&mut m, 2, vec![(1, vec![])], (3, 3, true));
    assert_eq!(m.visible(), [0, 2]);
}

#[test]
fn restarting_without_a_search_does_nothing() {
    let mut m = model(&[1]);
    m.restart_search(5);
    assert!(!m.searching());
    assert_eq!(m.query(), 0);
}

#[test]
fn each_part_adds_its_hits_to_the_earlier_ones() {
    let mut m = model(&[1, 2, 3]);
    m.begin_search(1);
    hits(&mut m, 1, vec![(1, vec![0])], (1, 3, false));
    hits(&mut m, 1, vec![(3, vec![2])], (2, 3, false));
    hits(&mut m, 1, vec![], (3, 3, true));
    assert_eq!(m.match_counts(), (2, 2));
    assert_eq!(m.matched_hunks(1), Some(&[0][..]));
    assert_eq!(m.matched_hunks(3), Some(&[2][..]));
}
