use std::sync::mpsc::channel;

use super::*;
use crate::search::{Mode, search};
use crate::service::FailKind;

#[test]
fn captured_text_appears_with_a_higher_revision() {
    let h = plain();
    let before = h.service.snapshot().revision;
    h.service.captured(text("alpha", 1));
    let snapshot = wait_for(&h.service, |s| has_text(s, "alpha"));
    assert!(snapshot.revision > before);
    assert!(snapshot.saved.is_empty());
}

#[test]
fn a_repeat_adds_no_row_and_counts_one_use() {
    let h = plain();
    h.service.captured(text("alpha", 1));
    h.service.captured(text("alpha", 2));
    let snapshot = wait_for(&h.service, |s| {
        s.history.first().is_some_and(|row| row.entry.uses == 2)
    });
    assert_eq!(snapshot.history.len(), 1);
    h.service.captured(text("marker", 3));
    let snapshot = wait_for(&h.service, |s| has_text(s, "marker"));
    assert_eq!(snapshot.history.len(), 2);
    assert_eq!(snapshot.history[1].entry.uses, 2);
}

#[test]
fn recording_off_drops_captures() {
    let h = plain();
    h.service.set_recording(false);
    h.service.captured(text("hidden", 1));
    h.service.set_recording(true);
    h.service.captured(text("shown", 2));
    let snapshot = wait_for(&h.service, |s| has_text(s, "shown"));
    assert_eq!(bodies(&snapshot), ["shown"]);
    assert_eq!(h.service.dropped(), 0);
}

#[test]
fn skipped_content_is_dropped() {
    let limits = Limits {
        max_text_bytes: 8,
        ..Limits::default()
    };
    let h = harness(FakeSink::default(), limits);
    h.service.captured(text("   ", 1));
    h.service.captured(text("far too long for the cap", 2));
    h.service.captured(text("fits", 3));
    let snapshot = wait_for(&h.service, |s| has_text(s, "fits"));
    assert_eq!(bodies(&snapshot), ["fits"]);
    assert_eq!(h.service.errors(), 0);
}

#[test]
fn copy_writes_text_once_and_paths_with_the_cut_flag() {
    let h = plain();
    h.service.captured(text("alpha", 1));
    h.service
        .captured(files(&["a.txt", "b.txt"], Transfer::Cut, 2));
    let snapshot = wait_for(&h.service, |s| s.history.len() == 2);
    let (paths_id, text_id) = (snapshot.history[0].id, snapshot.history[1].id);
    h.service.copy(text_id, How::Copy);
    h.service.copy(paths_id, How::Paste);
    h.service.captured(text("marker", 3));
    wait_for(&h.service, |s| has_text(s, "marker"));
    let expected = vec![
        Call::Text("alpha".into()),
        Call::Files(vec!["a.txt".into(), "b.txt".into()], true),
    ];
    assert_eq!(h.sink.calls(), expected);
    assert_eq!(h.service.errors(), 0);
}

#[test]
fn an_answered_copy_replies_only_after_the_sink_wrote() {
    let (sink, entered_rx, release_tx) = blocking_sink();
    let h = harness(sink, Limits::default());
    let id = first_id(&h, "alpha");
    let reply = h.service.copy_answered(id, How::Paste);
    entered_rx.recv_timeout(WAIT).unwrap();
    assert!(reply.try_recv().is_err());
    release_tx.send(()).unwrap();
    assert_eq!(answer(reply), Ok(()));
    assert_eq!(h.sink.calls(), [Call::Text("alpha".into())]);
}

#[test]
fn an_answered_copy_reports_a_missing_row_and_a_failing_sink() {
    let sink = FakeSink {
        fail: true,
        ..FakeSink::default()
    };
    let h = harness(sink, Limits::default());
    assert_eq!(
        answer(h.service.copy_answered(999, How::Copy)),
        Err(FailKind::NotFound)
    );
    let id = first_id(&h, "alpha");
    assert_eq!(
        answer(h.service.copy_answered(id, How::Copy)),
        Err(FailKind::Sink)
    );
    assert_eq!(h.service.errors(), 2);
}

#[test]
fn delete_and_clear_history_change_the_snapshot() {
    let h = plain();
    for (n, body) in ["one", "two", "three"].into_iter().enumerate() {
        h.service.captured(text(body, i64::try_from(n).unwrap()));
    }
    let snapshot = wait_for(&h.service, |s| s.history.len() == 3);
    h.service.delete(snapshot.history[1].id);
    let after_delete = wait_for(&h.service, |s| s.history.len() == 2);
    assert_eq!(bodies(&after_delete), ["three", "one"]);
    assert!(after_delete.revision > snapshot.revision);
    assert_eq!(answer(h.service.clear_history()), Ok(()));
    let cleared = wait_for(&h.service, |s| s.history.is_empty());
    assert!(cleared.revision > after_delete.revision);
}

#[test]
fn wipe_all_leaves_an_empty_snapshot_and_a_usable_service() {
    let h = plain();
    h.service.captured(text("old", 1));
    wait_for(&h.service, |s| has_text(s, "old"));
    assert_eq!(answer(h.service.wipe_all()), Ok(()));
    wait_for(&h.service, |s| s.history.is_empty());
    h.service.captured(text("new", 2));
    let snapshot = wait_for(&h.service, |s| has_text(s, "new"));
    assert_eq!(bodies(&snapshot), ["new"]);
    assert_eq!(h.service.errors(), 0);
    assert!(h.dir.path().join("clipboard.db").exists());
}

#[test]
fn subscribers_hear_increasing_revisions() {
    let h = plain();
    let (tx, rx) = channel();
    h.service.subscribe(Box::new(move |revision| {
        let _ = tx.send(revision);
    }));
    h.service.captured(text("one", 1));
    h.service.captured(text("two", 2));
    let last = wait_for(&h.service, |s| has_text(s, "two")).revision;
    let heard: Vec<u64> = rx.try_iter().collect();
    assert!(heard.len() >= 2, "{heard:?}");
    assert!(heard.windows(2).all(|pair| pair[0] < pair[1]), "{heard:?}");
    assert_eq!(heard.last(), Some(&last));
}

#[test]
fn rows_feed_search() {
    let h = plain();
    h.service.captured(text("needle in hay", 1));
    h.service.captured(text("other", 2));
    let snapshot = wait_for(&h.service, |s| s.history.len() == 2);
    let found = search(snapshot.rows(), Mode::All, "needle", None);
    assert_eq!(found.len(), 1);
}

#[test]
fn snapshot_debug_shows_counts_only() {
    let h = plain();
    h.service.captured(text("canary-body", 1));
    let snapshot = wait_for(&h.service, |s| s.history.len() == 1);
    let shown = format!("{snapshot:?}");
    assert!(!shown.contains("canary"), "{shown}");
    assert!(shown.contains("history: 1"), "{shown}");
}

#[test]
fn wipe_and_clear_on_a_stopped_service_answer_busy() {
    let h = plain();
    assert!(h.service.stop());
    assert_eq!(answer(h.service.wipe_all()), Err(FailKind::Busy));
    assert_eq!(answer(h.service.clear_history()), Err(FailKind::Busy));
}

#[test]
fn the_service_is_alive_until_its_worker_ends() {
    let h = plain();
    assert!(h.service.is_alive());
    assert!(h.service.stop());
    assert!(!h.service.is_alive());
}
