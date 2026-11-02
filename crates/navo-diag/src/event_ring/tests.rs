use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::{EventRing, RingStats};

fn texts(ring: &EventRing) -> Vec<String> {
    ring.snapshot().iter().map(|e| e.to_string()).collect()
}

fn ring_with(budget: usize, events: &[&str]) -> EventRing {
    let ring = EventRing::new(budget);
    for event in events {
        ring.push(event);
    }
    ring
}

#[test]
fn event_ring_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EventRing>();
}

#[test]
fn default_budget_is_two_mebibytes() {
    assert_eq!(EventRing::DEFAULT_BUDGET, 2_097_152);
}

#[test]
fn new_ring_has_empty_stats() {
    assert_eq!(EventRing::new(64).stats(), RingStats::default());
}

#[test]
fn new_ring_has_empty_snapshot() {
    assert!(EventRing::new(64).snapshot().is_empty());
}

#[test]
fn snapshot_returns_events_oldest_first() {
    let ring = ring_with(64, &["one", "two", "three"]);
    assert_eq!(texts(&ring), ["one", "two", "three"]);
}

#[test]
fn stats_count_events_and_utf8_bytes() {
    let ring = ring_with(64, &["ab", "é€", ""]);
    let stats = ring.stats();
    assert_eq!((stats.events, stats.bytes), (3, 7));
}

#[test]
fn snapshot_is_unaffected_by_later_pushes() {
    let ring = ring_with(64, &["first"]);
    let before = ring.snapshot();
    ring.push("second");
    assert_eq!(before.len(), 1);
}

#[test]
fn push_past_budget_drops_oldest() {
    let ring = ring_with(10, &["aaaa", "bbbb", "cccc"]);
    assert_eq!(texts(&ring), ["bbbb", "cccc"]);
}

#[test]
fn push_past_budget_counts_each_drop() {
    let ring = ring_with(10, &["aaaa", "bbbb", "cccc"]);
    let expected = RingStats {
        events: 2,
        bytes: 8,
        dropped: 1,
        rejected: 0,
    };
    assert_eq!(ring.stats(), expected);
}

#[test]
fn large_push_drops_several_oldest() {
    let ring = ring_with(10, &["aaa", "bbb", "ccc", "dddddddd"]);
    assert_eq!(texts(&ring), ["dddddddd"]);
}

#[test]
fn large_push_counts_several_drops() {
    let ring = ring_with(10, &["aaa", "bbb", "ccc", "dddddddd"]);
    let expected = RingStats {
        events: 1,
        bytes: 8,
        dropped: 3,
        rejected: 0,
    };
    assert_eq!(ring.stats(), expected);
}

#[test]
fn filling_budget_exactly_drops_nothing() {
    let ring = ring_with(10, &["aaaaa", "bbbbb"]);
    let expected = RingStats {
        events: 2,
        bytes: 10,
        dropped: 0,
        rejected: 0,
    };
    assert_eq!(ring.stats(), expected);
}

#[test]
fn event_exactly_budget_is_stored() {
    let ring = ring_with(10, &["0123456789"]);
    assert_eq!(texts(&ring), ["0123456789"]);
}

#[test]
fn event_exactly_budget_evicts_all_others() {
    let ring = ring_with(10, &["aa", "bb", "0123456789"]);
    let expected = RingStats {
        events: 1,
        bytes: 10,
        dropped: 2,
        rejected: 0,
    };
    assert_eq!(ring.stats(), expected);
}

#[test]
fn event_over_budget_is_not_stored() {
    let ring = ring_with(10, &["keep", "0123456789x"]);
    assert_eq!(texts(&ring), ["keep"]);
}

#[test]
fn event_over_budget_counts_rejected() {
    let ring = ring_with(10, &["keep", "0123456789x"]);
    let expected = RingStats {
        events: 1,
        bytes: 4,
        dropped: 0,
        rejected: 1,
    };
    assert_eq!(ring.stats(), expected);
}

#[test]
fn zero_budget_rejects_every_nonempty_event() {
    let ring = ring_with(0, &["a", "bb", "ccc"]);
    let expected = RingStats {
        events: 0,
        bytes: 0,
        dropped: 0,
        rejected: 3,
    };
    assert_eq!(ring.stats(), expected);
}

#[test]
fn clear_empties_the_ring() {
    let ring = ring_with(64, &["one", "two"]);
    ring.clear();
    assert!(ring.snapshot().is_empty());
}

#[test]
fn clear_keeps_counters() {
    let ring = ring_with(6, &["aaaa", "bbbb", "0123456"]);
    ring.clear();
    let expected = RingStats {
        events: 0,
        bytes: 0,
        dropped: 1,
        rejected: 1,
    };
    assert_eq!(ring.stats(), expected);
}

const THREADS: usize = 8;
const PUSHES: usize = 1_000;
const SHARED_BUDGET: usize = 4_096;

fn hammer() -> RingStats {
    let ring = Arc::new(EventRing::new(SHARED_BUDGET));
    let workers: Vec<_> = (0..THREADS)
        .map(|t| {
            let ring = Arc::clone(&ring);
            thread::spawn(move || {
                for i in 0..PUSHES {
                    ring.push(&format!("{{\"thread\":{t},\"seq\":{i:04}}}"));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("worker thread panicked");
    }
    ring.stats()
}

#[test]
fn concurrent_pushes_are_all_kept_or_dropped() {
    let stats = hammer();
    let total = u64::try_from(stats.events).expect("fits") + stats.dropped;
    assert_eq!(total, u64::try_from(THREADS * PUSHES).expect("fits"));
}

#[test]
fn concurrent_pushes_stay_within_budget() {
    assert!(hammer().bytes <= SHARED_BUDGET);
}

fn event() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(vec!['a', 'é', '€']), 0..=300).prop_map(|chars| {
        let mut text = String::new();
        for c in chars {
            if text.len() + c.len_utf8() > 300 {
                break;
            }
            text.push(c);
        }
        text
    })
}

fn scenario() -> impl Strategy<Value = (usize, Vec<String>)> {
    (0usize..=4_096, prop::collection::vec(event(), 0..64))
}

#[derive(Default)]
struct Model {
    budget: usize,
    events: VecDeque<String>,
    bytes: usize,
    dropped: u64,
    rejected: u64,
}

impl Model {
    fn push(&mut self, event: &str) {
        if event.len() > self.budget {
            self.rejected += 1;
            return;
        }
        self.events.push_back(event.to_owned());
        self.bytes += event.len();
        while self.bytes > self.budget {
            let oldest = self
                .events
                .pop_front()
                .expect("over budget implies an event");
            self.bytes -= oldest.len();
            self.dropped += 1;
        }
    }

    fn stats(&self) -> RingStats {
        RingStats {
            events: self.events.len(),
            bytes: self.bytes,
            dropped: self.dropped,
            rejected: self.rejected,
        }
    }
}

fn check_each_push(
    budget: usize,
    events: &[String],
    check: impl Fn(&EventRing, &[String], &Model) -> Result<(), TestCaseError>,
) -> Result<(), TestCaseError> {
    let ring = EventRing::new(budget);
    let mut model = Model {
        budget,
        ..Model::default()
    };
    let mut accepted = Vec::new();
    for event in events {
        ring.push(event);
        model.push(event);
        if event.len() <= budget {
            accepted.push(event.clone());
        }
        check(&ring, &accepted, &model)?;
    }
    Ok(())
}

proptest! {
    #[test]
    fn prop_bytes_never_exceed_budget((budget, events) in scenario()) {
        check_each_push(budget, &events, |ring, _, _| {
            prop_assert!(ring.stats().bytes <= budget);
            Ok(())
        })?;
    }

    #[test]
    fn prop_snapshot_is_suffix_of_accepted((budget, events) in scenario()) {
        check_each_push(budget, &events, |ring, accepted, _| {
            let snapshot = texts(ring);
            prop_assert!(accepted.ends_with(&snapshot));
            Ok(())
        })?;
    }

    #[test]
    fn prop_stats_agree_with_snapshot((budget, events) in scenario()) {
        check_each_push(budget, &events, |ring, _, _| {
            let snapshot = ring.snapshot();
            let stats = ring.stats();
            let bytes: usize = snapshot.iter().map(|e| e.len()).sum();
            prop_assert_eq!((stats.events, stats.bytes), (snapshot.len(), bytes));
            Ok(())
        })?;
    }

    #[test]
    fn prop_ring_matches_model((budget, events) in scenario()) {
        check_each_push(budget, &events, |ring, _, model| {
            prop_assert_eq!(ring.stats(), model.stats());
            prop_assert_eq!(texts(ring), Vec::from(model.events.clone()));
            Ok(())
        })?;
    }
}
