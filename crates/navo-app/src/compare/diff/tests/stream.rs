//! Tests for the Stream session: priority, four workers, budget and generations.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub(crate) mod support;

use navo_ui::compare_diff::{Card, PairKind, StreamControl, StreamUpdate, ViewedKey};

use crate::compare::diff::stream::memory::{CARD_BYTES, card_bytes};
use crate::compare::diff::stream::state::State;
use crate::compare::diff::stream::{LoadFn, StreamSession};
use support::{
    Concurrency, Gate, PairData, card_size, changed_pair, channel, items, load_fn, not_viewed,
    request, simple_load,
};

const WAIT: Duration = Duration::from_secs(20);
const SIXTY_FOUR_MIB: u64 = 64 * 1024 * 1024;

fn small_data(ids: std::ops::Range<u64>) -> HashMap<u64, PairData> {
    ids.map(|id| {
        (
            id,
            PairData {
                left: b"alpha\nbase line\n".to_vec(),
                right: b"alpha\nchanged line\n".to_vec(),
                blocked: false,
            },
        )
    })
    .collect()
}

#[test]
fn focused_items_are_queued_before_the_rest() {
    let data = small_data(0..8);
    let (sink, receiver) = channel();
    let load = load_fn(data.clone(), Duration::from_millis(2), None, None);
    let session = StreamSession::start(sink, SIXTY_FOUR_MIB, load, not_viewed());
    session.focus(vec![6, 2]);
    session.set_items(1, items(&data, true));
    let mut ids = Vec::new();
    while ids.len() < 4 {
        if let StreamUpdate::Card { id, .. } = receiver.recv_timeout(WAIT).unwrap() {
            ids.push(id);
        }
    }
    assert!(
        ids.contains(&6) && ids.contains(&2),
        "focused ids were {ids:?}"
    );
}

fn next_card(receiver: &Receiver<StreamUpdate>) -> Arc<Card> {
    loop {
        if let StreamUpdate::Card { card, .. } = receiver.recv_timeout(WAIT).unwrap() {
            return card;
        }
    }
}

#[test]
fn focus_resends_a_held_card_without_rebuilding() {
    let data = small_data(0..1);
    let (sink, receiver) = channel();
    let builds = Arc::new(AtomicUsize::new(0));
    let base = simple_load(data.clone());
    let counter = Arc::clone(&builds);
    let load: LoadFn = Arc::new(move |request| {
        counter.fetch_add(1, Ordering::SeqCst);
        base(request)
    });
    let session = StreamSession::start(sink, SIXTY_FOUR_MIB, load, not_viewed());
    session.set_items(1, items(&data, true));
    let first = next_card(&receiver);
    session.focus(vec![0]);
    let again = next_card(&receiver);
    assert!(Arc::ptr_eq(&first, &again));
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}

#[test]
fn focus_replaces_the_focused_set() {
    let mut state = State::new(SIXTY_FOUR_MIB);
    let _ = state.focus(vec![1]);
    let _ = state.focus(vec![2]);
    state.set_items(1, vec![(request(1), true), (request(2), true)]);
    assert!(state.in_normal_queue(1));
    assert!(state.in_focus_queue(2));
}

#[test]
fn never_more_than_four_loads_run_at_once() {
    let data = small_data(0..20);
    let concurrency = Arc::new(Concurrency::default());
    let (sink, receiver) = channel();
    let load = load_fn(
        data.clone(),
        Duration::from_millis(5),
        Some(Arc::clone(&concurrency)),
        None,
    );
    let session = StreamSession::start(sink, SIXTY_FOUR_MIB, load, not_viewed());
    session.set_items(1, items(&data, true));
    let mut built = 0;
    while built < 20 {
        if let StreamUpdate::Card { .. } = receiver.recv_timeout(WAIT).unwrap() {
            built += 1;
        }
    }
    assert!(
        concurrency.maximum() <= 4,
        "maximum was {}",
        concurrency.maximum()
    );
}

#[test]
fn held_bodies_stay_under_the_budget_and_refocus_rebuilds() {
    const BUDGET: u64 = 1024 * 1024;
    let data: HashMap<u64, PairData> = (0..200).map(|id| (id, changed_pair(id, 30, 80))).collect();
    let (sink, receiver) = channel();
    let load = load_fn(data.clone(), Duration::ZERO, None, None);
    let session = StreamSession::start(sink, BUDGET, load, not_viewed());
    session.set_items(1, items(&data, true));
    let mut held: HashMap<u64, u64> = HashMap::new();
    let mut evicted = Vec::new();
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            StreamUpdate::Card { id, card, .. } => {
                held.insert(id, card_size(&card));
                assert!(session.state_held() <= BUDGET);
            }
            StreamUpdate::Evicted { id, .. } => {
                held.remove(&id);
                evicted.push(id);
            }
            StreamUpdate::Progress { built: 200, .. } => break,
            _ => {}
        }
    }
    assert!(!evicted.is_empty());
    let victim = evicted[0];
    session.focus(vec![victim]);
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            StreamUpdate::Card { id, .. } if id == victim => break,
            _ => {}
        }
    }
}

#[test]
fn a_new_generation_drops_blocked_work_from_the_old_one() {
    let gate = Arc::new(Gate::default());
    let mut old = small_data(100..112);
    for pair in old.values_mut() {
        pair.blocked = true;
    }
    let new = small_data(200..203);
    let mut all = old.clone();
    all.extend(new.clone());
    let (sink, receiver) = channel();
    let load = load_fn(all, Duration::ZERO, None, Some(Arc::clone(&gate)));
    let session = StreamSession::start(sink, SIXTY_FOUR_MIB, load, not_viewed());
    session.set_items(1, items(&old, true));
    gate.wait_opened();
    session.set_items(2, items(&new, true));
    gate.release();
    let mut cards = 0;
    while cards < 3 {
        if let StreamUpdate::Card { generation, id, .. } = receiver.recv_timeout(WAIT).unwrap() {
            assert_eq!(
                generation, 2,
                "old card {id} arrived after the new generation"
            );
            cards += 1;
        }
    }
}

#[test]
fn items_are_queued_in_item_order_and_focus_moves_one_ahead_once() {
    let mut state = State::new(SIXTY_FOUR_MIB);
    state.set_items(1, (0..40).map(|id| (request(id), true)).collect());
    let _ = state.focus(vec![30, 30, 7]);
    let _ = state.focus(vec![30]);
    let mut expected = vec![30, 7];
    expected.extend((0..40).filter(|id| *id != 30 && *id != 7));
    assert_eq!(state.drain_order(), expected);
}

#[test]
fn a_summary_card_still_counts_against_the_budget() {
    let card = Card {
        kind: PairKind::Binary,
        rows: Vec::new(),
        hunks: Vec::new(),
        added: 0,
        removed: 0,
        viewed_key: ViewedKey([0; 64]),
        viewed: false,
        viewable: true,
    };
    assert_eq!(card_bytes(&card), CARD_BYTES);
}

#[test]
fn many_summary_cards_are_evicted_under_the_budget() {
    let budget = CARD_BYTES * 10;
    let data: HashMap<u64, PairData> = (0..60)
        .map(|id| {
            let pair = PairData {
                left: vec![0, 1, 2],
                right: vec![0, 1, 3],
                blocked: false,
            };
            (id, pair)
        })
        .collect();
    let (sink, receiver) = channel();
    let session = StreamSession::start(sink, budget, simple_load(data.clone()), not_viewed());
    session.set_items(1, items(&data, true));
    let mut evicted = 0;
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            StreamUpdate::Card { .. } => assert!(session.state_held() <= budget),
            StreamUpdate::Evicted { .. } => evicted += 1,
            StreamUpdate::Progress { built: 60, .. } => break,
            _ => {}
        }
    }
    assert!(evicted >= 50, "only {evicted} evicted");
}

#[test]
fn items_leaving_the_list_free_their_cards_in_the_same_generation() {
    let data: HashMap<u64, PairData> = (0..6).map(|id| (id, changed_pair(id, 30, 80))).collect();
    let (sink, receiver) = channel();
    let load = load_fn(data.clone(), Duration::ZERO, None, None);
    let session = StreamSession::start(sink, SIXTY_FOUR_MIB, load, not_viewed());
    session.set_items(1, items(&data, true));
    while !matches!(
        receiver.recv_timeout(WAIT).unwrap(),
        StreamUpdate::Progress { built: 6, .. }
    ) {}
    let full = session.state_held();
    let kept: HashMap<u64, PairData> = data.into_iter().filter(|(id, _)| *id < 2).collect();
    session.set_items(1, items(&kept, true));
    let left = session.state_held();
    assert!(left > 0 && left < full / 2, "held {left} of {full}");
}
