//! Tests for how the Stream model takes updates: generations, item lists, cards, search, Esc.

use std::sync::Arc;

use super::super::tests::{card, file, give, model};
use crate::browser::RowStatus;
use crate::compare_diff::hooks::{Card, StreamUpdate};
use crate::compare_diff::stream_model::{EscStep, Stop, StreamModel, stream_items};

#[test]
fn new_items_keep_the_cursor_on_the_same_id() {
    let mut m = model(&[1, 2, 3]);
    give(&mut m, 3, 1);
    m.cursor = Stop {
        item: 2,
        hunk: Some(0),
    };
    let rows = [
        file(0, "a0.txt", RowStatus::Different),
        file(1, "f1.txt", RowStatus::Different),
        file(3, "f3.txt", RowStatus::Different),
    ];
    m.set_items(2, stream_items(&rows, ""));
    assert_eq!(
        m.cursor,
        Stop {
            item: 2,
            hunk: Some(0)
        }
    );
    assert!(m.cards.contains_key(&3));
}

#[test]
fn a_stale_generation_is_ignored() {
    let mut m = model(&[1]);
    m.apply(&StreamUpdate::Card {
        generation: 0,
        id: 1,
        card: card(1, 1),
    });
    assert!(m.cards.is_empty());
    give(&mut m, 1, 1);
    m.apply(&StreamUpdate::Evicted {
        generation: 0,
        id: 1,
    });
    assert_eq!(m.cards.len(), 1);
    m.apply(&StreamUpdate::Evicted {
        generation: 1,
        id: 1,
    });
    assert!(m.cards.is_empty());
}

#[test]
fn search_narrows_and_esc_unwinds_in_order() {
    let mut m = model(&[1, 2, 3]);
    give(&mut m, 2, 3);
    m.set_outline(true);
    m.begin_search(7);
    m.apply(&StreamUpdate::Hits {
        generation: 1,
        query: 6,
        hits: vec![(1, vec![])],
        searched: 1,
        total: 3,
        done: false,
    });
    assert!(m.visible().is_empty());
    m.apply(&StreamUpdate::Hits {
        generation: 1,
        query: 7,
        hits: vec![(2, vec![1, 2])],
        searched: 3,
        total: 3,
        done: true,
    });
    assert_eq!(m.visible(), [1]);
    assert_eq!(m.stops().len(), 2);
    assert_eq!(m.esc(), EscStep::ClearSearch);
    assert_eq!(m.visible().len(), 3);
    assert_eq!(m.esc(), EscStep::CloseOutline);
    assert_eq!(m.esc(), EscStep::Leave);
}

#[test]
fn hits_in_parts_add_up_and_esc_keeps_the_cursor() {
    let mut m = model(&[1, 2, 3]);
    give(&mut m, 1, 1);
    give(&mut m, 3, 2);
    m.begin_search(1);
    for (id, hunks) in [(1, vec![0]), (3, vec![1])] {
        m.apply(&StreamUpdate::Hits {
            generation: 1,
            query: 1,
            hits: vec![(id, hunks)],
            searched: 1,
            total: 3,
            done: false,
        });
    }
    assert_eq!(m.visible(), [0, 2]);
    assert!(m.next_change(1));
    assert_eq!(
        m.cursor,
        Stop {
            item: 2,
            hunk: Some(1)
        }
    );
    assert_eq!(m.esc(), EscStep::ClearSearch);
    assert!(!m.searching());
    assert_eq!(m.cursor.item, 2);
}

#[test]
fn hits_without_a_search_are_dropped() {
    let mut m = model(&[1]);
    m.apply(&StreamUpdate::Hits {
        generation: 1,
        query: 0,
        hits: vec![(1, vec![])],
        searched: 1,
        total: 1,
        done: true,
    });
    assert!(!m.searching());
}

/// Sends a one-hunk card for item 1 with content key byte `key` and stored mark `viewed`.
fn keyed(m: &mut StreamModel, key: u8, viewed: bool) {
    let mut built: Card = (*card(1, key)).clone();
    built.viewed = viewed;
    m.apply(&StreamUpdate::Card {
        generation: m.generation,
        id: 1,
        card: Arc::new(built),
    });
}

#[test]
fn a_rebuilt_card_keeps_the_mark_and_new_content_takes_the_stored_one() {
    let mut m = model(&[1]);
    keyed(&mut m, 5, true);
    assert!(m.is_viewed(1));
    m.toggle_viewed();
    // Same content again, built before the store saw the change: the local mark wins.
    keyed(&mut m, 5, true);
    assert!(!m.is_viewed(1));
    // New content: its stored mark.
    keyed(&mut m, 6, true);
    assert!(m.is_viewed(1));
    keyed(&mut m, 7, false);
    assert!(!m.is_viewed(1));
}

#[test]
fn viewed_from_the_app_and_progress_apply() {
    let mut m = model(&[1, 2]);
    m.apply(&StreamUpdate::Viewed {
        id: 2,
        viewed: true,
    });
    assert_eq!(m.counter(), (1, 2));
    m.apply(&StreamUpdate::Progress {
        generation: 0,
        built: 9,
        total: 9,
    });
    assert_eq!(m.progress(), (0, 0));
    m.apply(&StreamUpdate::Progress {
        generation: 1,
        built: 1,
        total: 2,
    });
    assert_eq!(m.progress(), (1, 2));
}

#[test]
fn an_item_that_leaves_moves_the_cursor_to_its_neighbour() {
    let mut m = model(&[1, 2, 3]);
    m.cursor = Stop {
        item: 1,
        hunk: None,
    };
    let rows = [
        file(1, "f1.txt", RowStatus::Different),
        file(3, "f3.txt", RowStatus::Different),
    ];
    m.set_items(2, stream_items(&rows, ""));
    assert_eq!(m.items()[m.cursor().item].id, 3);
    m.set_items(3, stream_items(&[], ""));
    assert!(m.stops().is_empty());
}
