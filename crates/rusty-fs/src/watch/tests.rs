use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use super::batch::Batch;
use super::*;

fn name(text: &str) -> ProviderPath {
    ProviderPath::from_text(text)
}

fn created(text: &str) -> WatchChange {
    WatchChange::Created(name(text))
}

fn ms(count: u64) -> Duration {
    Duration::from_millis(count)
}

#[test]
fn a_batch_waits_for_the_quiet_time_after_the_last_change() {
    let start = Instant::now();
    let mut batch = Batch::default();
    batch.push(created("a"), start);
    batch.push(created("b"), start + ms(60));
    assert_eq!(batch.take_due(start + ms(150)), None);
    assert_eq!(
        batch.take_due(start + ms(160)),
        Some(WatchEvent::Changed(vec![created("a"), created("b")]))
    );
}

#[test]
fn a_steady_stream_is_cut_at_the_maximum_delay() {
    let start = Instant::now();
    let mut batch = Batch::default();
    for step in 0..=50 {
        batch.push(created(&format!("f{step}")), start + ms(step * 10));
    }
    assert_eq!(batch.deadline(), Some(start + WATCH_MAX_DELAY));
    let Some(WatchEvent::Changed(changes)) = batch.take_due(start + ms(500)) else {
        panic!("the batch is due at the maximum delay");
    };
    assert_eq!(changes.len(), 51);
    assert_eq!(batch.deadline(), None);
}

#[test]
fn an_overflow_replaces_the_changes_of_its_batch() {
    let start = Instant::now();
    let mut batch = Batch::default();
    batch.push(created("a"), start);
    batch.overflow(start);
    batch.push(created("b"), start);
    assert_eq!(batch.take_due(start + ms(100)), Some(WatchEvent::Overflow));
}

#[test]
fn an_empty_batch_has_no_deadline_and_nothing_to_take() {
    let mut batch = Batch::default();
    assert_eq!(batch.deadline(), None);
    assert_eq!(batch.take(), None);
}

#[test]
fn a_moved_pair_with_one_cookie_is_a_rename() {
    let mut translate = Translate::default();
    let mut out = Vec::new();
    translate.feed(RawKind::MovedFrom(7), name("old"), &mut out);
    translate.feed(RawKind::MovedTo(7), name("new"), &mut out);
    translate.flush(&mut out);
    assert_eq!(
        out,
        [WatchChange::Renamed {
            from: name("old"),
            to: name("new")
        }]
    );
}

#[test]
fn a_move_out_of_the_folder_is_a_removal() {
    let mut translate = Translate::default();
    let mut out = Vec::new();
    translate.feed(RawKind::MovedFrom(7), name("gone"), &mut out);
    translate.feed(RawKind::Created, name("other"), &mut out);
    assert_eq!(out, [WatchChange::Removed(name("gone")), created("other")]);
}

#[test]
fn a_move_into_the_folder_is_a_creation() {
    let mut translate = Translate::default();
    let mut out = Vec::new();
    translate.feed(RawKind::MovedFrom(1), name("left"), &mut out);
    translate.feed(RawKind::MovedTo(2), name("came"), &mut out);
    assert_eq!(out, [WatchChange::Removed(name("left")), created("came")]);
}

fn manual_watch() -> (Watch, RawSender, mpsc::Receiver<WatchEvent>, Arc<()>) {
    let (sender, events) = mpsc::channel();
    let marker = Arc::new(());
    let held = Arc::clone(&marker);
    let sink: WatchSink = Box::new(move |event| {
        let _ = &held;
        let _ = sender.send(event);
    });
    let mut raw = None;
    let watch = Watch::start(&Location::new("/").unwrap(), sink, |sender| {
        raw = Some(sender);
        Ok(())
    })
    .unwrap();
    (watch, raw.unwrap(), events, marker)
}

#[test]
fn quick_changes_arrive_as_one_batch() {
    let (_watch, raw, events, _) = manual_watch();
    for index in 0..5 {
        raw.change(created(&format!("f{index}")));
    }
    let Ok(WatchEvent::Changed(changes)) = events.recv_timeout(Duration::from_secs(5)) else {
        panic!("one batch arrives");
    };
    assert_eq!(changes.len(), 5);
}

#[test]
fn a_failure_flushes_the_pending_batch_first() {
    let (_watch, raw, events, _) = manual_watch();
    raw.change(created("a"));
    let error = FsError {
        kind: FsErrorKind::NetworkLost,
        location: None,
        os_code: None,
    };
    raw.failed(error.clone());
    let wait = Duration::from_secs(5);
    assert_eq!(
        events.recv_timeout(wait),
        Ok(WatchEvent::Changed(vec![created("a")]))
    );
    assert_eq!(events.recv_timeout(wait), Ok(WatchEvent::Failed(error)));
}

#[test]
fn dropping_the_watch_stops_the_thread_and_drops_the_sink() {
    let (watch, raw, events, marker) = manual_watch();
    raw.change(created("late"));
    drop(watch);
    assert_eq!(Arc::strong_count(&marker), 1);
    assert!(!raw.change(created("after")));
    while let Ok(event) = events.recv() {
        assert!(matches!(event, WatchEvent::Changed(_)));
    }
}

fn modified(text: &str) -> WatchChange {
    WatchChange::Modified(name(text))
}

#[test]
fn a_thousand_modifies_of_one_file_are_one_change() {
    let start = Instant::now();
    let mut batch = Batch::default();
    for _ in 0..1_000 {
        batch.push(modified("download.part"), start);
    }
    assert_eq!(
        batch.take(),
        Some(WatchEvent::Changed(vec![modified("download.part")]))
    );
}

#[test]
fn a_created_file_that_is_then_written_stays_created() {
    let start = Instant::now();
    let mut batch = Batch::default();
    batch.push(created("new.txt"), start);
    batch.push(modified("new.txt"), start);
    assert_eq!(
        batch.take(),
        Some(WatchEvent::Changed(vec![created("new.txt")]))
    );
}

#[test]
fn the_latest_kind_of_a_name_wins() {
    let start = Instant::now();
    let mut batch = Batch::default();
    batch.push(created("short.txt"), start);
    batch.push(WatchChange::Removed(name("short.txt")), start);
    batch.push(modified("other.txt"), start);
    assert_eq!(
        batch.take(),
        Some(WatchEvent::Changed(vec![
            WatchChange::Removed(name("short.txt")),
            modified("other.txt"),
        ]))
    );
}

#[test]
fn a_write_after_a_rename_keeps_the_rename() {
    let start = Instant::now();
    let mut batch = Batch::default();
    let rename = WatchChange::Renamed {
        from: name("a"),
        to: name("b"),
    };
    batch.push(rename.clone(), start);
    batch.push(modified("b"), start);
    assert_eq!(batch.take(), Some(WatchEvent::Changed(vec![rename])));
}

#[test]
fn a_watch_is_live_only_once_its_backend_says_so() {
    let (watch, raw, _events, _) = manual_watch();
    assert!(!watch.is_live());
    raw.mark_live();
    assert!(watch.is_live());
}
