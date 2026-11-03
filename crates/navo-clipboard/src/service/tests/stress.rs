use super::*;
use crate::service::FailKind;

#[test]
fn stop_returns_in_time_with_work_queued_and_drains_it() {
    let sink = FakeSink {
        delay: Duration::from_millis(40),
        ..FakeSink::default()
    };
    let h = harness(sink, Limits::default());
    h.service.captured(text("alpha", 1));
    let id = wait_for(&h.service, |s| has_text(s, "alpha")).history[0].id;
    for _ in 0..5 {
        h.service.copy(id, How::Copy);
    }
    h.service.captured(text("queued", 2));
    let started = Instant::now();
    assert!(h.service.stop());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(h.sink.calls().len(), 5);
    assert!(has_text(&h.service.snapshot(), "queued"));
    assert!(!h.service.captured(text("late", 3)));
    assert!(h.service.stop());
}

#[test]
fn a_full_channel_drops_without_blocking() {
    let (sink, entered_rx, release_tx) = blocking_sink();
    let h = harness(sink, Limits::default());
    let id = first_id(&h, "alpha");
    h.service.copy(id, How::Copy);
    entered_rx.recv_timeout(WAIT).unwrap();

    let started = Instant::now();
    let accepted = (0..74)
        .filter(|n| h.service.captured(text(&format!("row {n}"), 10 + n)))
        .count();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(accepted, 64);
    assert_eq!(h.service.dropped(), 10);

    drop(release_tx);
    wait_for(&h.service, |s| has_text(s, "row 63"));
}

#[test]
fn an_unknown_row_and_a_failing_sink_are_counted_not_fatal() {
    let sink = FakeSink {
        fail: true,
        ..FakeSink::default()
    };
    let h = harness(sink, Limits::default());
    assert_eq!(h.service.last_error(), None);
    h.service.copy(999, How::Copy);
    h.service.captured(text("alpha", 1));
    let id = wait_for(&h.service, |s| has_text(s, "alpha")).history[0].id;
    assert_eq!(h.service.errors(), 1);
    assert_eq!(h.service.last_error(), Some(FailKind::NotFound));
    h.service.copy(id, How::Paste);
    h.service.captured(text("beta", 2));
    wait_for(&h.service, |s| has_text(s, "beta"));
    assert_eq!(h.service.errors(), 2);
    assert_eq!(h.service.last_error(), Some(FailKind::Sink));
    assert!(h.sink.calls().is_empty());
}

const SHORT: Duration = Duration::from_millis(100);

#[test]
fn a_capture_queued_before_recording_stops_is_dropped() {
    let (sink, entered_rx, release_tx) = blocking_sink();
    let h = harness(sink, Limits::default());
    let id = first_id(&h, "alpha");
    h.service.copy(id, How::Copy);
    entered_rx.recv_timeout(WAIT).unwrap();
    assert!(h.service.captured(text("queued", 2)));
    h.service.set_recording(false);
    drop(release_tx);
    assert!(h.service.stop());
    assert_eq!(bodies(&h.service.snapshot()), ["alpha"]);
}

#[test]
fn stop_gives_up_on_a_blocked_sink_and_the_worker_then_discards_the_rest() {
    let (sink, entered_rx, release_tx) = blocking_sink();
    let h = harness_stopping(sink, Limits::default(), SHORT);
    let id = first_id(&h, "alpha");
    h.service.copy(id, How::Copy);
    entered_rx.recv_timeout(WAIT).unwrap();
    assert!(h.service.captured(text("queued", 2)));
    assert!(h.service.copy(id, How::Copy));

    assert!(!h.service.stop());
    assert!(!h.service.stop());
    drop(release_tx);
    let deadline = Instant::now() + WAIT;
    while !h.service.stop() {
        assert!(Instant::now() < deadline, "worker never ended");
    }
    assert_eq!(bodies(&h.service.snapshot()), ["alpha"]);
    assert_eq!(h.sink.calls().len(), 1);
}

#[test]
fn dropping_the_service_does_not_wait_for_a_blocked_sink() {
    let (sink, entered_rx, release_tx) = blocking_sink();
    let Harness { dir, service, .. } = harness_stopping(sink, Limits::default(), SHORT);
    service.captured(text("alpha", 1));
    let id = wait_for(&service, |s| has_text(s, "alpha")).history[0].id;
    service.copy(id, How::Copy);
    entered_rx.recv_timeout(WAIT).unwrap();
    let started = Instant::now();
    drop(service);
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(release_tx);
    drop(dir);
}

#[test]
fn a_panicking_listener_is_counted_and_the_worker_goes_on() {
    let h = plain();
    h.service.subscribe(Box::new(|_| panic!("listener failed")));
    h.service.captured(text("one", 1));
    wait_for(&h.service, |s| has_text(s, "one"));
    wait_until(|| h.service.last_error() == Some(FailKind::Thread));
    h.service.captured(text("two", 2));
    wait_for(&h.service, |s| has_text(s, "two"));
    wait_until(|| h.service.errors() == 2);
}

/// The worker records a listener panic after it publishes the snapshot, so poll with a deadline.
fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(
            Instant::now() < deadline,
            "the worker never recorded the panic"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_listener_may_subscribe_while_it_is_called() {
    let h = plain();
    let shared = h.service.shared.clone();
    let (tx, rx) = channel();
    h.service.subscribe(Box::new(move |_| {
        shared.subscribe(Box::new(|_| {}));
        let _ = tx.send(());
    }));
    h.service.captured(text("one", 1));
    rx.recv_timeout(WAIT).unwrap();
    h.service.captured(text("two", 2));
    wait_for(&h.service, |s| has_text(s, "two"));
}

#[test]
fn deleting_a_missing_row_does_not_notify() {
    let h = plain();
    let (tx, rx) = channel();
    h.service.subscribe(Box::new(move |revision| {
        let _ = tx.send(revision);
    }));
    h.service.captured(text("one", 1));
    wait_for(&h.service, |s| has_text(s, "one"));
    assert_eq!(answer(h.service.delete(9999)), Ok(()));
    h.service.captured(text("two", 2));
    wait_for(&h.service, |s| has_text(s, "two"));
    assert_eq!(rx.try_iter().count(), 2);
}
