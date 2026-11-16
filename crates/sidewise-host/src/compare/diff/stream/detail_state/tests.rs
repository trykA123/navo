//! Tests for the Stream detail lane: choice, cancellation, retries and card budget.

use std::sync::Arc;

use navo_ui::compare_diff::{
    Card, CardDetail, CardScan, LargeCard, PairKind, PairRequest, ViewedKey,
};

use super::detail_bytes;
use crate::compare::diff::stream::memory::CARD_BYTES;
use crate::compare::diff::stream::state::State;

const MIB: u64 = 1024 * 1024;

/// A settled request with no locations; only its id matters to the detail lane.
fn request(id: u64) -> PairRequest {
    PairRequest {
        id,
        path: format!("file-{id}"),
        left: None,
        right: None,
    }
}

/// A card of `kind` with no rows, so only the fixed card bytes count.
fn card(kind: PairKind) -> Card {
    Card {
        kind,
        rows: Vec::new(),
        hunks: Vec::new(),
        added: 0,
        removed: 0,
        viewed_key: ViewedKey([0; 64]),
        viewed: false,
        viewable: true,
    }
}

/// A finished large detail with no hunks; `scan` says the job ended.
fn large_detail() -> Arc<CardDetail> {
    Arc::new(CardDetail::Large(LargeCard {
        hunks: Vec::new(),
        found: 0,
        progress: None,
        scan: CardScan::Done,
    }))
}

/// A state holding one finished card per `(id, kind)`, focused in slice order.
fn state_with(kinds: &[(u64, PairKind)]) -> State {
    let mut state = State::new(MIB);
    state.set_items(
        1,
        kinds.iter().map(|(id, _)| (request(*id), true)).collect(),
    );
    state.focus(kinds.iter().map(|(id, _)| *id).collect());
    for (id, kind) in kinds {
        assert!(state.finish(*id, 1, Arc::new(card(*kind))).is_some());
    }
    state
}

#[test]
fn take_detail_picks_the_heavy_card_nearest_the_middle_of_the_focus() {
    let mut state = state_with(&[
        (0, PairKind::Text),
        (
            1,
            PairKind::Large {
                left: 10,
                right: 10,
            },
        ),
        (2, PairKind::Binary),
        (3, PairKind::Image),
        (4, PairKind::OneSided { left: true }),
    ]);
    let job = state.take_detail().unwrap();
    assert_eq!(job.id, 2);
    assert_eq!(job.kind, PairKind::Binary);
    assert_eq!(job.generation, 1);
    assert_eq!(job.request.id, 2);
}

#[test]
fn text_cards_get_no_detail_job() {
    let mut state = state_with(&[
        (0, PairKind::Text),
        (1, PairKind::OneSided { left: false }),
        (2, PairKind::Unavailable("gone")),
    ]);
    assert!(state.take_detail().is_none());
}

#[test]
fn one_job_runs_at_a_time() {
    let mut state = state_with(&[(0, PairKind::Binary), (1, PairKind::Image)]);
    let job = state.take_detail().unwrap();
    assert!(state.take_detail().is_none());
    state.end_detail(&job);
    assert!(state.take_detail().is_some());
}

#[test]
fn leaving_the_focus_cancels_the_running_job() {
    let mut state = state_with(&[(0, PairKind::Binary), (1, PairKind::Image)]);
    let job = state.take_detail().unwrap();
    state.focus(vec![1]);
    assert!(job.cancel.is_cancelled());
    assert!(state.store_detail(&job, large_detail(), true).is_none());
    state.end_detail(&job);
    assert!(state.take_detail().is_some());
}

#[test]
fn a_new_generation_cancels_and_drops_details() {
    let mut state = state_with(&[(0, PairKind::Binary), (1, PairKind::Image)]);
    let first = state.take_detail().unwrap();
    assert!(state.store_detail(&first, large_detail(), true).is_some());
    state.end_detail(&first);
    let second = state.take_detail().unwrap();
    assert_eq!(state.focused_details().len(), 1);

    state.set_items(2, vec![(request(0), true), (request(1), true)]);
    assert!(second.cancel.is_cancelled());
    assert!(state.focused_details().is_empty());
    assert!(state.take_detail().is_none());

    state.end_detail(&second);
    assert!(
        state
            .finish(1, 2, Arc::new(card(PairKind::Image)))
            .is_some()
    );
    state.focus(vec![1]);
    assert!(state.take_detail().is_some());
}

#[test]
fn invalidate_forgets_the_detail_and_allows_a_new_job() {
    let kind = PairKind::Large { left: 5, right: 5 };
    let mut state = state_with(&[(0, kind)]);
    let job = state.take_detail().unwrap();
    assert!(state.store_detail(&job, large_detail(), true).is_some());
    state.end_detail(&job);
    assert!(state.take_detail().is_none());

    state.invalidate(0);
    assert!(state.focused_details().is_empty());
    assert!(state.finish(0, 1, Arc::new(card(kind))).is_some());
    assert!(state.take_detail().is_some());
}

#[test]
fn detail_bytes_count_against_the_card_budget_and_evict_the_oldest() {
    let mut state = State::new(900);
    state.set_items(1, vec![(request(0), true), (request(1), true)]);
    state.focus(vec![0, 1]);
    assert!(
        state
            .finish(0, 1, Arc::new(card(PairKind::Binary)))
            .is_some()
    );
    assert!(
        state
            .finish(1, 1, Arc::new(card(PairKind::Image)))
            .is_some()
    );

    let first = state.take_detail().unwrap();
    let small = Arc::new(CardDetail::Failed("abcd".to_owned()));
    assert_eq!(detail_bytes(&small), CARD_BYTES + 4);
    let stored = state
        .store_detail(&first, Arc::clone(&small), true)
        .unwrap();
    assert!(stored.evicted.is_empty());
    state.end_detail(&first);

    let second = state.take_detail().unwrap();
    assert_eq!(second.id, 1);
    let large = Arc::new(CardDetail::Failed("x".repeat(200)));
    let stored = state.store_detail(&second, large, true).unwrap();
    assert_eq!(stored.evicted, vec![0]);
    assert_eq!(state.held_bytes, CARD_BYTES);
    let shown = state.focused_details();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].0, 1);
}

#[test]
fn a_job_that_ends_without_a_last_snapshot_is_not_retried() {
    let mut state = state_with(&[(0, PairKind::Large { left: 1, right: 1 })]);
    let job = state.take_detail().unwrap();
    assert!(state.store_detail(&job, large_detail(), false).is_some());
    state.end_detail(&job);
    assert!(state.take_detail().is_none());
    assert_eq!(state.focused_details().len(), 1);
}

#[test]
fn a_stale_job_cannot_store() {
    let mut state = state_with(&[(0, PairKind::Binary), (1, PairKind::Image)]);
    let stale = state.take_detail().unwrap();
    state.end_detail(&stale);
    let fresh = state.take_detail().unwrap();
    assert!(state.store_detail(&stale, large_detail(), true).is_none());

    state.focus(Vec::new());
    assert!(fresh.cancel.is_cancelled());
    assert!(state.store_detail(&fresh, large_detail(), true).is_none());
    state.end_detail(&fresh);

    state.focus(vec![fresh.id]);
    let changed = state.take_detail().unwrap();
    state.set_items(2, vec![(request(0), true), (request(1), true)]);
    assert!(state.store_detail(&changed, large_detail(), true).is_none());
}
