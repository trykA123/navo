//! Tests for the live engine slot: waiting requests are replayed once and then forgotten.

use std::sync::{Arc, Mutex, PoisonError};

use super::{Live, ProbeCalls, Running};

/// A probe engine and the calls it will record.
fn probe() -> (Running, Arc<Mutex<ProbeCalls>>) {
    let calls = Arc::new(Mutex::new(ProbeCalls::default()));
    (Running::Probe(Arc::clone(&calls)), calls)
}

fn with<T>(calls: &Arc<Mutex<ProbeCalls>>, read: impl FnOnce(&ProbeCalls) -> T) -> T {
    read(&calls.lock().unwrap_or_else(PoisonError::into_inner))
}

#[test]
fn install_replays_a_waiting_window_and_focus_only_once() {
    let live = Live::new();
    live.lines(0..3);
    live.focus(7);
    let (first, calls) = probe();
    live.install(Some(first));
    assert_eq!(with(&calls, |calls| calls.lines.clone()), vec![0..3]);
    assert_eq!(with(&calls, |calls| calls.focus.clone()), vec![7]);
    let (second, next) = probe();
    live.install(Some(second));
    assert!(with(&next, |calls| calls.lines.is_empty()));
    assert!(with(&next, |calls| calls.focus.is_empty()));
}

#[test]
fn clear_forgets_a_waiting_window_and_focus() {
    let live = Live::new();
    live.lines(0..3);
    live.focus(7);
    live.clear();
    let (engine, calls) = probe();
    live.install(Some(engine));
    assert!(with(&calls, |calls| calls.lines.is_empty()));
    assert!(with(&calls, |calls| calls.focus.is_empty()));
}

#[test]
fn acknowledge_reaches_the_installed_engine() {
    let live = Live::new();
    let (engine, calls) = probe();
    live.install(Some(engine));
    live.acknowledge(11);
    assert_eq!(with(&calls, |calls| calls.acked.clone()), vec![11]);
}

#[test]
fn a_closed_slot_refuses_a_late_engine() {
    let live = Live::new();
    live.close();
    let (engine, calls) = probe();
    live.install(Some(engine));
    live.lines(0..3);
    live.acknowledge(5);
    assert!(with(&calls, |calls| calls.lines.is_empty()));
    assert!(with(&calls, |calls| calls.acked.is_empty()));
}
