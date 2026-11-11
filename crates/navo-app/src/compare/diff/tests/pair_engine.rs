//! Pair worker engine tests: large and byte updates, content alignment, replay and cancel.

use std::io::{Cursor, Read};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use navo_compare::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_compare::text::Options;
use navo_compare::text::side::WHOLE_LIMIT;
use navo_core::{FileIdentity, ProviderId, ProviderPath};
use navo_ui::compare_diff::{DiffUpdate, PairControl, PairKind, PairRequest};

use super::pair::{WAIT, channel, local_side, location, request, write};
use crate::compare::diff::load::{LoadInput, Side};
use crate::compare::diff::pair::PairDiff;

/// A request whose locations point at the two files the large engine reads.
fn large_request(id: u64, root: &Path) -> PairRequest {
    PairRequest {
        id,
        path: "left.log".to_owned(),
        left: Some(location(&root.join("left.log"))),
        right: Some(location(&root.join("right.log"))),
    }
}

/// Write 4,000 lines with one change, then a loader input whose sides claim to be large.
fn large_input(root: &Path) -> LoadInput {
    let mut left = String::new();
    let mut right = String::new();
    for index in 0..4_000 {
        left.push_str(&format!("line {index}\n"));
        if index == 1_234 {
            right.push_str(&format!("changed {index}\n"));
        } else {
            right.push_str(&format!("line {index}\n"));
        }
    }
    write(root, "left.log", left.as_bytes());
    write(root, "right.log", right.as_bytes());
    LoadInput {
        left: Some(oversize_side(root, "left.log")),
        right: Some(oversize_side(root, "right.log")),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

/// A real local side whose listed size is past the whole-text limit, so the loader says Large.
fn oversize_side(root: &Path, rel: &str) -> Side {
    let (side, _) = local_side(root, rel);
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(rel)),
        EntryKind::File,
        WHOLE_LIMIT + 1,
        None,
    );
    (side, entry)
}

/// Read `Opened` and return its kind.
fn opened(receive: &Receiver<DiffUpdate>, id: u64) -> PairKind {
    match receive.recv_timeout(WAIT).expect("the pair opened") {
        DiffUpdate::Opened { request, kind, .. } => {
            assert_eq!(request, id);
            kind
        }
        other => panic!("expected Opened, got {other:?}"),
    }
}

#[test]
fn a_large_pair_streams_updates_to_done() {
    let dir = tempfile::tempdir().unwrap();
    let input = large_input(dir.path());
    let (sink, receive) = channel();
    let control = PairDiff::start(large_request(7, dir.path()), input, sink);
    assert!(matches!(opened(&receive, 7), PairKind::Large { .. }));
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no finished large update");
        if let DiffUpdate::Large {
            request,
            progress: Some(progress),
            ..
        } = receive.recv_timeout(WAIT).unwrap()
        {
            assert_eq!(request, 7);
            if progress.total > 0 && progress.done >= progress.total {
                break;
            }
        }
    }
    drop(control);
}

#[test]
fn a_binary_pair_streams_byte_updates() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.bin", b"\0left bytes");
    write(dir.path(), "right.bin", b"\0right bytes");
    let input = LoadInput {
        left: Some(local_side(dir.path(), "left.bin")),
        right: Some(local_side(dir.path(), "right.bin")),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    let (sink, receive) = channel();
    let control = PairDiff::start(request(7), input, sink);
    assert_eq!(opened(&receive, 7), PairKind::Binary);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no finished byte update");
        if let DiffUpdate::Bytes {
            request,
            progress: Some(progress),
            ..
        } = receive.recv_timeout(WAIT).unwrap()
        {
            assert_eq!(request, 7);
            if progress.total > 0 && progress.done >= progress.total {
                break;
            }
        }
    }
    drop(control);
}

#[test]
fn binary_pairs_of_different_sizes_align_by_content() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.bin", b"\0abc");
    write(dir.path(), "right.bin", b"\0abc-and-a-longer-tail");
    let input = LoadInput {
        left: Some(local_side(dir.path(), "left.bin")),
        right: Some(local_side(dir.path(), "right.bin")),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    let (sink, receive) = channel();
    let control = PairDiff::start(request(7), input, sink);
    assert_eq!(opened(&receive, 7), PairKind::Binary);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no finished byte update");
        if let DiffUpdate::Bytes {
            progress: Some(progress),
            ..
        } = receive.recv_timeout(WAIT).unwrap()
            && progress.total > 0
            && progress.done >= progress.total
        {
            assert_eq!(
                progress.total, 26,
                "content alignment counts both sides' bytes"
            );
            break;
        }
    }
    drop(control);
}

#[test]
fn a_forced_bytes_view_opens_a_large_pair_as_binary() {
    let dir = tempfile::tempdir().unwrap();
    let input = large_input(dir.path());
    let (sink, receive) = channel();
    let control = PairDiff::start(large_request(7, dir.path()), input, sink);
    assert!(matches!(opened(&receive, 7), PairKind::Large { .. }));
    control.view_as(8, true);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(
            Instant::now() < deadline,
            "no Opened for the forced request"
        );
        if let DiffUpdate::Opened {
            request: 8, kind, ..
        } = receive.recv_timeout(WAIT).unwrap()
        {
            assert_eq!(kind, PairKind::Binary);
            break;
        }
    }
    control.view_as(9, false);
    loop {
        assert!(
            Instant::now() < deadline,
            "no Opened for the released request"
        );
        if let DiffUpdate::Opened {
            request: 9, kind, ..
        } = receive.recv_timeout(WAIT).unwrap()
        {
            assert!(matches!(kind, PairKind::Large { .. }));
            break;
        }
    }
    drop(control);
}

#[test]
fn lines_before_the_engine_starts_are_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let input = large_input(dir.path());
    let (sink, receive) = channel();
    let control = PairDiff::start(large_request(7, dir.path()), input, sink);
    // Start/install ordering is not forced; engine::tests::install_replays_a_waiting_window_and_focus_only_once is deterministic.
    control.lines(0..3);
    assert!(matches!(opened(&receive, 7), PairKind::Large { .. }));
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(
            Instant::now() < deadline,
            "the waiting window was not replayed"
        );
        if let DiffUpdate::LargeLines { request, .. } = receive.recv_timeout(WAIT).unwrap() {
            assert_eq!(request, 7);
            break;
        }
    }
    drop(control);
}

#[test]
fn an_engine_failure_reaches_the_update_stream() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.bin", b"\0left bytes");
    let gate = Arc::new(GateSource::new(b"\0right bytes"));
    gate.fail_open(3);
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("right.bin")),
        EntryKind::File,
        12,
        None,
    );
    let input = LoadInput {
        left: Some(local_side(dir.path(), "left.bin")),
        right: Some((Arc::clone(&gate) as Arc<dyn SideSource>, entry)),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    let (sink, receive) = channel();
    let control = PairDiff::start(request(7), input, sink);
    assert_eq!(opened(&receive, 7), PairKind::Binary);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(
            Instant::now() < deadline,
            "the engine failure was not reported"
        );
        if let DiffUpdate::Failed { request, .. } = receive.recv_timeout(WAIT).unwrap() {
            assert_eq!(request, 7);
            break;
        }
    }
    drop(control);
}

#[test]
fn a_refresh_restarts_the_large_diff_under_the_new_id() {
    let dir = tempfile::tempdir().unwrap();
    let input = large_input(dir.path());
    let (sink, receive) = channel();
    let control = PairDiff::start(large_request(7, dir.path()), input, sink);
    assert!(matches!(opened(&receive, 7), PairKind::Large { .. }));
    control.refresh(8);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "the refreshed pair never opened");
        match receive.recv_timeout(WAIT).unwrap() {
            DiffUpdate::Opened {
                request: 8, kind, ..
            } => {
                assert!(matches!(kind, PairKind::Large { .. }));
                break;
            }
            DiffUpdate::Opened { .. } => {}
            _ => {}
        }
    }
    control.focus(1);
    control.lines(0..3);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no line update on the new id");
        match receive.recv_timeout(WAIT).unwrap() {
            DiffUpdate::LargeLines { request, .. } => {
                assert_eq!(request, 8, "the restarted diff writes its new id");
                break;
            }
            DiffUpdate::Large { request, .. } | DiffUpdate::Bytes { request, .. } => {
                if request < 8 {
                    continue;
                }
                assert_eq!(request, 8, "a stale id reached the screen");
            }
            _ => {}
        }
    }
    drop(control);
}

#[test]
fn dropping_a_binary_pair_cancels_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.bin", b"\0left bytes");
    let gate = Arc::new(GateSource::new(b"\0right bytes"));
    let (started, is_started) = mpsc::channel();
    gate.expect_open(3, started);
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("right.bin")),
        EntryKind::File,
        12,
        None,
    );
    let input = LoadInput {
        left: Some(local_side(dir.path(), "left.bin")),
        right: Some((Arc::clone(&gate) as Arc<dyn SideSource>, entry)),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    let (sink, receive) = channel();
    let control = PairDiff::start(request(7), input, sink);
    assert_eq!(opened(&receive, 7), PairKind::Binary);
    is_started
        .recv_timeout(WAIT)
        .expect("the byte engine opened its side");
    let started_at = Instant::now();
    drop(control);
    assert!(started_at.elapsed() < Duration::from_secs(1));
    gate.release();
    let deadline = Instant::now() + WAIT;
    loop {
        match receive.recv_timeout(WAIT) {
            Ok(DiffUpdate::Bytes { .. }) => panic!("a byte update arrived after the cancel"),
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("a worker still runs after the cancel"),
        }
        assert!(Instant::now() < deadline, "the workers never stopped");
    }
}

/// A binary source whose `nth` open blocks until the test releases it.
struct GateSource {
    bytes: Vec<u8>,
    opens: AtomicUsize,
    fail_after: AtomicUsize,
    gate: Mutex<(usize, Option<mpsc::Sender<()>>)>,
    release: Arc<(Mutex<bool>, Condvar)>,
    provider: ProviderId,
}

impl GateSource {
    fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            opens: AtomicUsize::new(0),
            fail_after: AtomicUsize::new(0),
            gate: Mutex::new((0, None)),
            release: Arc::new((Mutex::new(false), Condvar::new())),
            provider: ProviderId::local(),
        }
    }

    fn expect_open(&self, nth: usize, started: mpsc::Sender<()>) {
        let mut gate = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        *gate = (nth, Some(started));
    }

    fn fail_open(&self, nth: usize) {
        self.fail_after.store(nth, Ordering::SeqCst);
    }

    fn release(&self) {
        let (lock, ready) = &*self.release;
        *lock.lock().unwrap_or_else(PoisonError::into_inner) = true;
        ready.notify_all();
    }
}

impl SideSource for GateSource {
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
        self.opens.fetch_add(1, Ordering::SeqCst);
        let fail_at = self.fail_after.load(Ordering::SeqCst);
        if fail_at > 0 && self.opens.load(Ordering::SeqCst) >= fail_at {
            return Err(Reason::ReadError);
        }
        let (nth, started) = &mut *self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        if *nth > 0 && self.opens.load(Ordering::SeqCst) >= *nth {
            if let Some(started) = started.take() {
                let _ = started.send(());
            }
            let (lock, ready) = &*self.release;
            let mut released = lock.lock().unwrap_or_else(PoisonError::into_inner);
            while !*released {
                released = ready.wait(released).unwrap_or_else(PoisonError::into_inner);
            }
        }
        Ok(Box::new(Cursor::new(self.bytes.clone())))
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: u64::try_from(self.bytes.len()).unwrap_or(u64::MAX),
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
