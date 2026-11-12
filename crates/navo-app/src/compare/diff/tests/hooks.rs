//! Tests for the DiffHooks wiring: pairs open through the backend, and the hooks never touch a
//! provider or the Viewed store on the caller's thread (VR1 finding 1).

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

use navo_compare::model::Reason;
use navo_compare::text::side::WHOLE_LIMIT;
use navo_core::{Location, ProviderPath};
use navo_ui::compare_diff::{
    CardDetail, CardScan, CompareOptions, DiffHooks, DiffSink, DiffUpdate, PairKind, PairRequest,
    StreamUpdate,
};
use navo_ui::settings::Values;

use crate::compare::diff::DiffBackend;
use crate::compare::diff::hooks::SideFn;
use crate::compare::diff::sides::side_of;
use crate::compare::diff::tests::stream::support::channel;
use crate::compare::diff::viewed::{NoViewed, OpenViewed, ViewedMarks};

const WAIT: Duration = Duration::from_secs(20);

fn location(path: &Path) -> Location {
    let text = path.to_str().unwrap_or_default();
    Location::local(ProviderPath::from_text(text)).unwrap()
}

fn write(root: &Path, name: &str, bytes: &[u8]) {
    fs::write(root.join(name), bytes).unwrap();
}

fn sink() -> (DiffSink, Receiver<DiffUpdate>) {
    let (sender, receiver) = mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| {
        let _ = sender.send(update);
    });
    (sink, receiver)
}

fn request(id: u64, left: &Path, right: &Path) -> PairRequest {
    PairRequest {
        id,
        path: "left.txt".to_owned(),
        left: Some(location(left)),
        right: Some(location(right)),
    }
}

#[test]
fn a_pair_from_two_temp_files_opens() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"alpha\nbeta\n");
    write(dir.path(), "right.txt", b"alpha\ngamma\n");
    let (sink, receiver) = sink();
    let _control = DiffBackend::default().open_pair(
        request(
            5,
            &dir.path().join("left.txt"),
            &dir.path().join("right.txt"),
        ),
        &CompareOptions::from_values(&Values::default()),
        sink,
    );
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Opened {
            request,
            kind,
            text,
        } => {
            assert_eq!(request, 5);
            assert_eq!(kind, PairKind::Text);
            assert!(text.is_some());
        }
        other => panic!("expected Opened, got {other:?}"),
    }
}

#[test]
fn a_missing_side_opens_as_one_sided() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"one\ntwo\n");
    let (sink, receiver) = sink();
    let _control = DiffBackend::default().open_pair(
        PairRequest {
            id: 6,
            path: "left.txt".to_owned(),
            left: Some(location(&dir.path().join("left.txt"))),
            right: None,
        },
        &CompareOptions::from_values(&Values::default()),
        sink,
    );
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Opened { kind, text, .. } => {
            assert_eq!(kind, PairKind::OneSided { left: true });
            assert!(text.is_some());
        }
        other => panic!("expected Opened, got {other:?}"),
    }
}

/// Calls seen by a probe: how many started and finished, and the threads that made them.
#[derive(Default)]
struct Probe {
    started: AtomicUsize,
    finished: AtomicUsize,
    threads: Mutex<Vec<ThreadId>>,
}

impl Probe {
    fn enter(&self) {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.threads.lock().unwrap().push(thread::current().id());
    }

    fn assert_off(&self, caller: ThreadId) {
        assert!(self.threads.lock().unwrap().iter().all(|id| *id != caller));
    }
}

/// A side builder that blocks until `release` sends, then builds the real side.
fn blocking_side(probe: &Arc<Probe>) -> (SideFn, Sender<()>) {
    let (release, gate) = mpsc::channel::<()>();
    let gate = Mutex::new(gate);
    let probe = Arc::clone(probe);
    let side: SideFn = Arc::new(move |location, settings| {
        probe.enter();
        let _ = gate.lock().unwrap().recv_timeout(WAIT);
        probe.finished.fetch_add(1, Ordering::SeqCst);
        side_of(location, settings)
    });
    (side, release)
}

/// A Viewed opener that counts its calls and their threads.
fn counting_viewed(probe: &Arc<Probe>) -> OpenViewed {
    let probe = Arc::clone(probe);
    Arc::new(move || -> Arc<dyn ViewedMarks> {
        probe.enter();
        Arc::new(NoViewed)
    })
}

#[test]
fn open_pair_returns_while_the_side_builder_blocks() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"a\n");
    write(dir.path(), "right.txt", b"b\n");
    let probe = Arc::new(Probe::default());
    let (side, release) = blocking_side(&probe);
    let backend = DiffBackend::with_parts(side, counting_viewed(&probe));
    let (sink, receiver) = sink();
    let left = dir.path().join("left.txt");
    let control = backend.open_pair(
        request(8, &left, &dir.path().join("right.txt")),
        &CompareOptions::from_values(&Values::default()),
        sink,
    );
    assert_eq!(probe.finished.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    release.send(()).unwrap();
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Opened { kind, .. } => assert_eq!(kind, PairKind::Text),
        other => panic!("expected Opened, got {other:?}"),
    }
    probe.assert_off(thread::current().id());
    drop(control);
}

#[test]
fn an_unopenable_side_reports_unavailable_from_the_worker() {
    let probe = Arc::new(Probe::default());
    let counter = Arc::clone(&probe);
    let side: SideFn = Arc::new(move |_, _| {
        counter.enter();
        Err(Reason::NotFound)
    });
    let backend = DiffBackend::with_parts(side, counting_viewed(&Arc::new(Probe::default())));
    let (sink, receiver) = sink();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("left.txt");
    let _control = backend.open_pair(
        request(9, &missing, &missing),
        &CompareOptions::from_values(&Values::default()),
        sink,
    );
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Opened {
            request,
            kind,
            text,
        } => {
            assert_eq!(request, 9);
            assert!(matches!(kind, PairKind::Unavailable(_)));
            assert!(text.is_none());
        }
        other => panic!("expected Opened, got {other:?}"),
    }
    probe.assert_off(thread::current().id());
}

#[test]
fn open_stream_opens_the_viewed_store_on_a_worker_only_when_needed() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"a\n");
    let probe = Arc::new(Probe::default());
    let backend = DiffBackend::with_parts(Arc::new(side_of), counting_viewed(&probe));
    let (sink, receiver) = channel();
    let stream = backend.open_stream(&CompareOptions::from_values(&Values::default()), sink);
    assert_eq!(probe.started.load(Ordering::SeqCst), 0);
    let left = dir.path().join("left.txt");
    let gone = dir.path().join("gone.txt");
    stream.set_items(1, vec![(request(3, &left, &gone), true)]);
    let card = loop {
        if let StreamUpdate::Card { card, .. } = receiver.recv_timeout(WAIT).unwrap() {
            break card;
        }
    };
    // The missing right file stays present but unreadable: no Viewed key, no store lookup.
    assert!(matches!(card.kind, PairKind::Unavailable(_)));
    assert!(!card.viewable);
    assert_eq!(probe.started.load(Ordering::SeqCst), 0);
    stream.set_items(2, vec![(request(4, &left, &left), true)]);
    loop {
        if let StreamUpdate::Card { card, .. } = receiver.recv_timeout(WAIT).unwrap() {
            assert!(card.viewable);
            break;
        }
    }
    assert_eq!(probe.started.load(Ordering::SeqCst), 1);
    probe.assert_off(thread::current().id());
}

#[test]
fn open_stream_runs_the_detail_lane_for_a_large_pair() {
    let dir = tempfile::tempdir().unwrap();
    let body = "same line\n".repeat(200);
    write(dir.path(), "left.log", body.as_bytes());
    write(dir.path(), "right.log", body.as_bytes());
    let side: SideFn = Arc::new(|location, settings| {
        let (source, mut entry) = side_of(location, settings)?;
        entry.size = WHOLE_LIMIT + 1;
        Ok((source, entry))
    });
    let backend = DiffBackend::with_parts(side, Arc::new(|| Arc::new(NoViewed)));
    let (sink, receiver) = channel();
    let stream = backend.open_stream(&CompareOptions::from_values(&Values::default()), sink);
    let left = dir.path().join("left.log");
    let right = dir.path().join("right.log");
    stream.focus(vec![7]);
    stream.set_items(1, vec![(request(7, &left, &right), true)]);

    let mut card_seen = false;
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            StreamUpdate::Card { id: 7, card, .. } => {
                assert!(matches!(card.kind, PairKind::Large { .. }));
                card_seen = true;
            }
            StreamUpdate::Detail { id: 7, detail, .. } => {
                assert!(card_seen, "the detail arrived before the card");
                let CardDetail::Large(card) = detail.as_ref() else {
                    panic!("unexpected large detail");
                };
                if card.scan == CardScan::Done {
                    break;
                }
            }
            _ => {}
        }
    }
}

#[test]
fn open_stream_runs_the_detail_lane_for_a_binary_pair() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.bin", b"a\0b");
    write(dir.path(), "right.bin", b"a\0c");
    let (sink, receiver) = channel();
    let stream =
        DiffBackend::default().open_stream(&CompareOptions::from_values(&Values::default()), sink);
    let left = dir.path().join("left.bin");
    let right = dir.path().join("right.bin");
    stream.focus(vec![8]);
    stream.set_items(1, vec![(request(8, &left, &right), true)]);

    let mut card_seen = false;
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            StreamUpdate::Card { id: 8, card, .. } => {
                assert_eq!(card.kind, PairKind::Binary);
                card_seen = true;
            }
            StreamUpdate::Detail { id: 8, detail, .. } => {
                assert!(card_seen, "the detail arrived before the card");
                let CardDetail::Bytes(card) = detail.as_ref() else {
                    panic!("unexpected binary detail");
                };
                if card.scan != CardScan::Running {
                    break;
                }
            }
            _ => {}
        }
    }
}
