//! Tests for pair control: Opened, spans, refresh under load and non-blocking drop.

use std::fs;
use std::io::{Cursor, Read};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use navo_compare::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_compare::text::Options;
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};
use navo_ui::compare_diff::{DiffSink, DiffUpdate, PairControl, PairKind, PairRequest};

use crate::compare::diff::load::{LoadInput, Side};
use crate::compare::diff::pair::PairDiff;
use crate::compare::local::LocalSide;

const WAIT: Duration = Duration::from_secs(20);

fn location(path: &Path) -> Location {
    let text = path.to_str().unwrap_or_default();
    Location::local(ProviderPath::from_text(text)).unwrap()
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    fs::write(root.join(rel), bytes).unwrap();
}

fn local_side(root: &Path, rel: &str) -> Side {
    let size = fs::metadata(root.join(rel)).unwrap().len();
    let side = Arc::new(LocalSide::new(location(root)).unwrap());
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(rel)),
        EntryKind::File,
        size,
        None,
    );
    (side, entry)
}

fn local_input(root: &Path) -> LoadInput {
    LoadInput {
        left: Some(local_side(root, "left.txt")),
        right: Some(local_side(root, "right.txt")),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

fn request(id: u64) -> PairRequest {
    PairRequest {
        id,
        path: "left.txt".to_owned(),
        left: None,
        right: None,
    }
}

fn channel() -> (DiffSink, Receiver<DiffUpdate>) {
    let (sender, receiver) = mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| {
        let _ = sender.send(update);
    });
    (sink, receiver)
}

#[test]
fn opened_comes_first_then_the_requested_spans() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"alpha\nbeta\n");
    write(dir.path(), "right.txt", b"alpha\nbetA\n");
    let (sink, receiver) = channel();
    let control = PairDiff::start(request(7), local_input(dir.path()), sink);
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Opened {
            request: id,
            kind,
            text,
        } => {
            assert_eq!(id, 7);
            assert_eq!(kind, PairKind::Text);
            assert!(text.is_some());
        }
        other => panic!("expected Opened, got {other:?}"),
    }
    control.spans(0..10);
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Spans { request, spans } => {
            assert_eq!(request, 7);
            assert!(!spans.is_empty());
        }
        other => panic!("expected Spans, got {other:?}"),
    }
}

#[test]
fn a_refresh_while_loading_sends_one_result_with_the_new_id() {
    let source = Arc::new(SlowSource::new());
    let (sink, receiver) = channel();
    let control = PairDiff::start(request(7), slow_input(&source), sink);
    source.wait_opened();
    control.refresh();
    source.release();
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Opened { request, .. } => assert_eq!(request, 8),
        other => panic!("expected the refreshed Opened, got {other:?}"),
    }
    control.spans(0..1);
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Spans { request, .. } => assert_eq!(request, 8),
        other => panic!("expected Spans on the new id, got {other:?}"),
    }
}

#[test]
fn dropping_during_a_slow_load_returns_at_once() {
    let source = Arc::new(SlowSource::new());
    let (sink, _receiver) = channel();
    let control = PairDiff::start(request(7), slow_input(&source), sink);
    source.wait_opened();
    let started = Instant::now();
    drop(control);
    assert!(started.elapsed() < Duration::from_millis(500));
    source.release();
}

fn slow_input(source: &Arc<SlowSource>) -> LoadInput {
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("a.txt")),
        EntryKind::File,
        6,
        None,
    );
    let slow = Arc::clone(source);
    let side: Arc<dyn SideSource> = slow;
    LoadInput {
        left: Some((Arc::clone(&side), entry.clone())),
        right: Some((side, entry)),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

/// A source whose first open waits until the test releases it.
struct SlowSource {
    gate: Arc<(Mutex<Gate>, Condvar)>,
    provider: ProviderId,
}

#[derive(Default)]
struct Gate {
    opened: bool,
    released: bool,
}

impl SlowSource {
    fn new() -> Self {
        Self {
            gate: Arc::new((Mutex::new(Gate::default()), Condvar::new())),
            provider: ProviderId::local(),
        }
    }

    fn wait_opened(&self) {
        let (lock, ready) = &*self.gate;
        let mut gate = lock.lock().unwrap_or_else(PoisonError::into_inner);
        while !gate.opened {
            gate = ready.wait(gate).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn release(&self) {
        let (lock, ready) = &*self.gate;
        let mut gate = lock.lock().unwrap_or_else(PoisonError::into_inner);
        gate.released = true;
        ready.notify_all();
    }
}

impl SideSource for SlowSource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Ok(())
    }

    fn open(&self, _rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let (lock, ready) = &*self.gate;
        let mut gate = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if !gate.released {
            gate.opened = true;
            ready.notify_all();
            while !gate.released {
                gate = ready.wait(gate).unwrap_or_else(PoisonError::into_inner);
            }
        }
        drop(gate);
        Ok(Box::new(Cursor::new(b"hello\n".to_vec())))
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: 6,
            modified: None,
            file_id: None,
        })
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        rel.as_path().clone()
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}
