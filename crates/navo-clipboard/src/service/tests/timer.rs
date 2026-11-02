use super::secrets::{PLAIN, save_secret};
use super::*;
use crate::service::secret::{CLEAR_ATTEMPTS, CLEAR_RETRY};
use crate::service::{FailKind, SECRET_CLEAR_AFTER};

const SECOND: Duration = Duration::from_secs(1);

/// Returns once the worker handled everything queued before.
fn settle(h: &Harness) {
    answer(h.service.notes()).unwrap();
}

/// Saves and copies a secret; returns the sequence of its private copy.
fn copy_secret(h: &Harness, text: &str) -> Sequence {
    let id = save_secret(h, text);
    answer(h.service.copy_saved(id, How::Copy)).unwrap();
    let board = h.sink.board();
    assert_eq!(board.text.as_deref(), Some(text));
    Sequence(board.sequence)
}

#[test]
fn a_secret_copy_is_private_and_cleared_after_30_seconds() {
    let h = plain();
    let sequence = copy_secret(&h, PLAIN);
    assert!(h.sink.calls().contains(&Call::Private(PLAIN.into())));
    h.advance(SECRET_CLEAR_AFTER - SECOND);
    settle(&h);
    assert!(h.sink.clears().is_empty());
    h.advance(SECOND);
    settle(&h);
    assert_eq!(h.sink.clears(), [(sequence, true)]);
    assert_eq!(h.sink.board().text, None);
}

#[test]
fn a_newer_copy_elsewhere_is_not_cleared() {
    let h = plain();
    let sequence = copy_secret(&h, PLAIN);
    h.sink.copy_elsewhere("newer");
    h.advance(SECRET_CLEAR_AFTER);
    settle(&h);
    assert_eq!(h.sink.clears(), [(sequence, false)]);
    assert_eq!(h.sink.board().text.as_deref(), Some("newer"));
}

#[test]
fn stop_before_30_seconds_clears() {
    let h = plain();
    let sequence = copy_secret(&h, PLAIN);
    h.advance(10 * SECOND);
    settle(&h);
    assert!(h.sink.clears().is_empty());
    assert!(h.service.stop());
    assert_eq!(h.sink.clears(), [(sequence, true)]);
    assert_eq!(h.sink.board().text, None);
}

#[test]
fn dropping_the_service_clears() {
    let h = plain();
    let sequence = copy_secret(&h, PLAIN);
    let Harness { service, sink, .. } = h;
    drop(service);
    assert_eq!(sink.clears(), [(sequence, true)]);
}

#[test]
fn a_newer_secret_copy_clears_the_old_one_and_restarts_the_timer() {
    let h = plain();
    let first = copy_secret(&h, PLAIN);
    h.advance(20 * SECOND);
    let second = copy_secret(&h, "canary-second");
    assert_eq!(h.sink.clears(), [(first, true)]);
    assert_eq!(h.sink.board().text.as_deref(), Some("canary-second"));
    h.advance(15 * SECOND);
    settle(&h);
    assert_eq!(h.sink.clears().len(), 1);
    h.advance(15 * SECOND);
    settle(&h);
    assert_eq!(h.sink.clears(), [(first, true), (second, true)]);
}

#[test]
fn a_newer_secret_copy_leaves_a_newer_clipboard_alone() {
    let h = plain();
    let first = copy_secret(&h, PLAIN);
    h.sink.copy_elsewhere("newer");
    copy_secret(&h, "canary-second");
    assert_eq!(h.sink.clears(), [(first, false)]);
}

#[test]
fn a_plain_copy_leaves_the_pending_clear_alone() {
    let h = plain();
    let sequence = copy_secret(&h, PLAIN);
    let id = first_id(&h, "plain text");
    h.service.copy(id, How::Copy);
    settle(&h);
    h.advance(SECRET_CLEAR_AFTER);
    settle(&h);
    assert_eq!(h.sink.clears(), [(sequence, false)]);
    assert_eq!(h.sink.board().text.as_deref(), Some("plain text"));
}

#[test]
fn the_clear_runs_at_the_deadline_without_another_message() {
    let h = plain();
    let sequence = copy_secret(&h, PLAIN);
    h.clock
        .advance(SECRET_CLEAR_AFTER - Duration::from_millis(100));
    h.clock.flows.store(true, Ordering::SeqCst);
    assert!(h.service.wake());
    wait_until(|| !h.sink.clears().is_empty());
    assert_eq!(h.sink.clears(), [(sequence, true)]);
}

#[test]
fn the_worker_keeps_working_while_a_clear_is_pending() {
    let h = plain();
    copy_secret(&h, PLAIN);
    h.service.captured(text("during", 2));
    wait_for(&h.service, |s| has_text(s, "during"));
    assert!(h.sink.clears().is_empty());
}

fn failing_clears(count: u32) -> Harness {
    let sink = FakeSink {
        failing_clears: AtomicU32::new(count),
        ..FakeSink::default()
    };
    harness(sink, Limits::default())
}

#[test]
fn a_clear_that_fails_twice_is_retried_until_the_secret_is_gone() {
    let h = failing_clears(2);
    let sequence = copy_secret(&h, PLAIN);
    h.advance(SECRET_CLEAR_AFTER);
    settle(&h);
    assert_eq!(h.sink.failed_clears(), 1);
    h.advance(CLEAR_RETRY - Duration::from_millis(1));
    settle(&h);
    assert_eq!(h.sink.failed_clears(), 1);
    h.advance(Duration::from_millis(1));
    settle(&h);
    assert_eq!(h.sink.failed_clears(), 2);
    h.advance(CLEAR_RETRY);
    settle(&h);
    assert_eq!(h.sink.clears(), [(sequence, true)]);
    assert_eq!(h.sink.board().text, None);
    assert_eq!(h.service.last_error(), None);
}

#[test]
fn a_clear_that_always_fails_gives_up_after_the_bounded_attempts_and_stop_tries_again() {
    let h = failing_clears(u32::MAX);
    copy_secret(&h, PLAIN);
    let errors = h.service.errors();
    h.advance(SECRET_CLEAR_AFTER);
    settle(&h);
    for _ in 1..CLEAR_ATTEMPTS {
        assert_eq!(h.service.errors(), errors);
        h.advance(CLEAR_RETRY);
        settle(&h);
    }
    assert_eq!(h.sink.failed_clears(), CLEAR_ATTEMPTS as usize);
    assert_eq!(h.service.errors(), errors + 1);
    assert_eq!(h.service.last_error(), Some(FailKind::Sink));
    for _ in 0..5 {
        h.advance(CLEAR_RETRY);
        settle(&h);
    }
    assert_eq!(h.sink.failed_clears(), CLEAR_ATTEMPTS as usize);
    assert!(h.service.stop());
    assert_eq!(h.sink.failed_clears(), CLEAR_ATTEMPTS as usize + 1);
}

#[test]
fn stop_clears_a_secret_whose_clear_failed_at_the_deadline() {
    let h = failing_clears(1);
    let sequence = copy_secret(&h, PLAIN);
    h.advance(SECRET_CLEAR_AFTER);
    settle(&h);
    assert_eq!(h.sink.failed_clears(), 1);
    assert!(h.service.stop());
    assert_eq!(h.sink.clears(), [(sequence, true)]);
    assert_eq!(h.sink.board().text, None);
}

#[test]
fn a_newer_secret_copy_after_a_failed_clear_is_cleared_in_turn() {
    let h = failing_clears(1);
    copy_secret(&h, PLAIN);
    let second = copy_secret(&h, "canary-second");
    assert_eq!(h.sink.failed_clears(), 1);
    h.advance(SECRET_CLEAR_AFTER);
    settle(&h);
    assert_eq!(h.sink.clears(), [(second, true)]);
}

#[test]
fn a_newer_copy_elsewhere_retires_the_clear_at_once() {
    let h = plain();
    let sequence = copy_secret(&h, PLAIN);
    h.sink.copy_elsewhere("newer");
    h.advance(SECRET_CLEAR_AFTER);
    settle(&h);
    for _ in 0..3 {
        h.advance(CLEAR_RETRY);
        settle(&h);
    }
    assert!(h.service.stop());
    assert_eq!(h.sink.clears(), [(sequence, false)]);
    assert_eq!(h.sink.board().text.as_deref(), Some("newer"));
}
