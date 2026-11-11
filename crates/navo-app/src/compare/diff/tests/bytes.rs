//! Tests for the byte worker: region deltas, progress, lifecycle and cancel.

use std::fs;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use navo_compare::bytes::Align;
use navo_compare::model::{
    Budget, CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource,
    Stamp,
};
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};
use navo_ui::compare_diff::{DiffSink, DiffUpdate, PairRequest};

use crate::compare::diff::bytes::{ByteDiff, ByteInput};
use crate::compare::diff::load::Side;
use crate::compare::local::LocalSide;

mod card;

const WAIT: Duration = Duration::from_secs(5);

fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap_or_default())).unwrap()
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

fn input(root: &Path, left: &str, right: &str, align: Align) -> ByteInput {
    ByteInput {
        left: local_side(root, left),
        right: local_side(root, right),
        align,
        run: 1,
        budget: Budget::default(),
    }
}

fn request() -> PairRequest {
    PairRequest {
        id: 11,
        path: "pair.bin".into(),
        left: None,
        right: None,
    }
}

fn sink() -> (DiffSink, Receiver<DiffUpdate>) {
    let (send, receive) = mpsc::channel();
    (
        Arc::new(move |update| {
            let _sent = send.send(update);
        }),
        receive,
    )
}

/// Collect region deltas until the final progress reports `done == total`.
fn collect(receive: &Receiver<DiffUpdate>) -> (Vec<navo_ui::compare_diff::ByteRegionView>, u64) {
    let deadline = Instant::now() + WAIT;
    let mut regions = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "no final byte progress");
        match receive.recv_timeout(WAIT) {
            Ok(DiffUpdate::Bytes {
                regions: delta,
                progress,
                ..
            }) => {
                regions.extend(delta);
                if let Some(progress) = progress
                    && progress.total > 0
                    && progress.done >= progress.total
                {
                    return (regions, progress.total);
                }
            }
            Ok(DiffUpdate::Failed { text, .. }) => panic!("byte diff failed: {text}"),
            Ok(_) => {}
            Err(error) => panic!(
                "byte worker ended early with {} regions: {error}",
                regions.len()
            ),
        }
    }
}

#[test]
fn equal_sizes_report_the_first_changed_offset() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.bin"), b"abcdefghij").unwrap();
    fs::write(dir.path().join("right.bin"), b"abcXefghij").unwrap();
    let (sink, receive) = sink();
    let control = ByteDiff::start(
        request(),
        input(dir.path(), "left.bin", "right.bin", Align::Offset),
        sink,
    );
    let (regions, total) = collect(&receive);
    assert_eq!(total, 10);
    assert!(
        regions
            .iter()
            .any(|region| region.left.contains(&3) && region.changed > 0),
        "the changed byte is in a region"
    );
    drop(control);
}

#[test]
fn different_sizes_align_by_content() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.bin"), b"0123456789abcdefghij").unwrap();
    fs::write(
        dir.path().join("right.bin"),
        b"0123456789abcdefghij-inserted-tail",
    )
    .unwrap();
    let (sink, receive) = sink();
    let control = ByteDiff::start(
        request(),
        input(dir.path(), "left.bin", "right.bin", Align::Content),
        sink,
    );
    let (regions, total) = collect(&receive);
    assert!(total > 20, "both sides are counted");
    assert!(!regions.is_empty(), "the inserted tail is a region");
    drop(control);
}

#[test]
fn a_finished_diff_joins_both_workers_after_drop() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.bin"), b"abcdefghij").unwrap();
    fs::write(dir.path().join("right.bin"), b"abcXefghij").unwrap();
    let (sink, receive) = sink();
    let control = ByteDiff::start(
        request(),
        input(dir.path(), "left.bin", "right.bin", Align::Offset),
        sink,
    );
    let (_, _) = collect(&receive);
    let joined = control.join_flag();
    drop(control);
    wait_for(&joined, "the workers joined after completion");
}

#[test]
fn a_cancel_stops_a_running_diff_and_joins() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release = Arc::new(Mutex::new(release_rx));
    let side = Arc::new(GateSide {
        size: 2_000_000,
        started: Mutex::new(Some(started_tx)),
        release: Arc::clone(&release),
        provider: ProviderId::local(),
    });
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("gate.bin")),
        EntryKind::File,
        side.size,
        None,
    );
    let (sink, _receive) = sink();
    let control = ByteDiff::start(
        request(),
        ByteInput {
            left: (side, entry.clone()),
            right: (
                Arc::new(GateSide {
                    size: entry.size,
                    started: Mutex::new(None),
                    release: Arc::new(Mutex::new(mpsc::channel().1)),
                    provider: ProviderId::local(),
                }),
                entry,
            ),
            align: Align::Offset,
            run: 1,
            budget: Budget::default(),
        },
        sink,
    );
    started_rx
        .recv_timeout(WAIT)
        .expect("the engine started a read");
    let joined = control.join_flag();
    let started = Instant::now();
    drop(control);
    assert!(started.elapsed() < Duration::from_secs(1));
    release_tx.send(()).expect("release the blocked read");
    wait_for(&joined, "cancel stops the diff and joins");
}

/// Wait for a flag without sleeping; fails after five seconds.
fn wait_for(flag: &AtomicBool, what: &str) {
    let deadline = Instant::now() + WAIT;
    while !flag.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::yield_now();
    }
}

/// A side whose first read waits for the test to release it.
struct GateSide {
    size: u64,
    started: Mutex<Option<Sender<()>>>,
    release: Arc<Mutex<Receiver<()>>>,
    provider: ProviderId,
}

impl SideSource for GateSide {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Err(Reason::Unsupported)
    }

    fn open(&self, _rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let started = self
            .started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        Ok(Box::new(GateReader {
            left: self.size,
            started,
            release: Arc::clone(&self.release),
            blocked: false,
        }))
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: self.size,
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
        true
    }
}

/// A reader that signals its first read and blocks until an external release.
struct GateReader {
    left: u64,
    started: Option<Sender<()>>,
    release: Arc<Mutex<Receiver<()>>>,
    blocked: bool,
}

impl Read for GateReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(started) = self.started.take() {
            let _sent = started.send(());
        }
        if !self.blocked {
            self.blocked = true;
            let _released = self
                .release
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv();
        }
        let limit = usize::try_from(self.left).unwrap_or(usize::MAX);
        let count = buf.len().min(64 * 1024).min(limit);
        buf[..count].fill(0);
        self.left = self
            .left
            .saturating_sub(u64::try_from(count).unwrap_or(u64::MAX));
        Ok(count)
    }
}
