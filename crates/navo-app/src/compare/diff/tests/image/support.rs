//! Shared helpers for the V37 image tests: temp sides, a scripted source with a gate,
//! and channels for pair updates.

use std::fs;
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
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};
use navo_ui::compare_diff::{DiffSink, DiffUpdate, ImageMask, ImageScores, PairKind, PairRequest};

use crate::compare::diff::load::{LoadInput, Side};
use crate::compare::local::LocalSide;
mod pattern;
pub(crate) use pattern::{PatternSource, pattern_input};

pub(crate) const WAIT: Duration = Duration::from_secs(5);
/// The image worker's chunk size; a gated read blocks after exactly one chunk.
pub(crate) const CHUNK: usize = 1024 * 1024;
/// A local location for `path`.
pub(crate) fn location(path: &Path) -> Location {
    let text = path.to_str().unwrap_or_default();
    Location::local(ProviderPath::from_text(text)).unwrap()
}

/// A local file side under `root`, listed as `rel`.
pub(crate) fn local_side(root: &Path, rel: &str) -> Side {
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

/// A pair input from two local files under `root`.
pub(crate) fn local_input(root: &Path, left: &str, right: &str) -> LoadInput {
    LoadInput {
        left: Some(local_side(root, left)),
        right: Some(local_side(root, right)),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

/// A pair request with the given compare row id.
pub(crate) fn request(id: u64) -> PairRequest {
    PairRequest {
        id,
        path: "left.png".to_owned(),
        left: None,
        right: None,
    }
}

/// A sink and the receiver of its updates.
pub(crate) fn channel() -> (DiffSink, Receiver<DiffUpdate>) {
    let (sender, receiver) = mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| {
        let _ = sender.send(update);
    });
    (sink, receiver)
}

/// Wait for `Opened` and return its kind; an image pair carries no text.
pub(crate) fn opened(receiver: &Receiver<DiffUpdate>) -> PairKind {
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Opened { kind, text, .. } => {
            assert!(text.is_none(), "an image pair has no text");
            kind
        }
        other => panic!("expected Opened, got {other:?}"),
    }
}

/// Wait for the image result of request `id`.
pub(crate) fn image(receiver: &Receiver<DiffUpdate>, id: u64) -> (ImageScores, Option<ImageMask>) {
    match receiver.recv_timeout(WAIT).unwrap() {
        DiffUpdate::Image {
            request,
            scores,
            mask,
        } => {
            assert_eq!(request, id);
            (scores, mask)
        }
        other => panic!("expected Image, got {other:?}"),
    }
}

/// A pair whose left source blocks in its full image read and whose right source counts opens.
pub(crate) fn blocked_input(left: &Arc<ScriptSource>, right: &Arc<ScriptSource>) -> LoadInput {
    named_input(left, right, "left.png", "right.png")
}

/// A pair over two scripted sources with the given file names.
pub(crate) fn named_input(
    left: &Arc<ScriptSource>,
    right: &Arc<ScriptSource>,
    left_name: &str,
    right_name: &str,
) -> LoadInput {
    LoadInput {
        left: Some(script_side(left, left_name, left.len())),
        right: Some(script_side(right, right_name, right.len())),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

/// A pair whose entries claim `size` bytes, to test the encoded-size fallback.
pub(crate) fn sized_input(
    left: &Arc<ScriptSource>,
    right: &Arc<ScriptSource>,
    size: u64,
) -> LoadInput {
    LoadInput {
        left: Some(script_side(left, "left.png", size)),
        right: Some(script_side(right, "right.png", size)),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

/// One side over a scripted source, with its listed size.
fn script_side(source: &Arc<ScriptSource>, name: &str, size: u64) -> Side {
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(name)),
        EntryKind::File,
        size,
        None,
    );
    let side: Arc<dyn SideSource> = source.clone();
    (side, entry)
}

/// A source that serves `bytes`, counts opens and can block one reader after `block_at`.
pub(crate) struct ScriptSource {
    bytes: Vec<u8>,
    provider: ProviderId,
    opens: AtomicUsize,
    block_open: usize,
    block_at: usize,
    gate: Arc<Gate>,
}

impl ScriptSource {
    /// A source blocked from open number `block_open` onward, after `block_at` bytes.
    pub(crate) fn new(bytes: Vec<u8>, block_open: usize, block_at: usize) -> Arc<Self> {
        Arc::new(Self {
            bytes,
            provider: ProviderId::local(),
            opens: AtomicUsize::new(0),
            block_open,
            block_at,
            gate: Arc::new(Gate::default()),
        })
    }

    /// The number of `open` calls so far.
    pub(crate) fn opens(&self) -> usize {
        self.opens.load(Ordering::SeqCst)
    }

    /// The number of bytes this side serves.
    pub(crate) fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap_or(u64::MAX)
    }

    /// Block until a gated reader has reached its block point.
    pub(crate) fn wait_blocked(&self) {
        self.gate.wait_blocked();
    }

    /// Let the gated reader finish and return the rest of the bytes.
    pub(crate) fn release(&self) {
        self.gate.release();
    }
}

impl SideSource for ScriptSource {
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
        let count = self.opens.fetch_add(1, Ordering::SeqCst) + 1;
        let inner = Cursor::new(self.bytes.clone());
        if count >= self.block_open {
            return Ok(Box::new(GatedReader {
                inner,
                block_at: self.block_at,
                gate: Arc::clone(&self.gate),
            }));
        }
        Ok(Box::new(inner))
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: self.len(),
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

/// A reader that blocks once, at `block_at`, until the gate is released.
struct GatedReader {
    inner: Cursor<Vec<u8>>,
    block_at: usize,
    gate: Arc<Gate>,
}

impl Read for GatedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let position = usize::try_from(self.inner.position()).unwrap_or(usize::MAX);
        if position >= self.block_at {
            self.gate.block();
        }
        self.inner.read(buffer)
    }
}

/// A one-shot gate shared by a scripted source and its test.
#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    ready: Condvar,
}

#[derive(Default)]
struct GateState {
    blocked: bool,
    released: bool,
}

impl Gate {
    /// Mark the reader blocked and wait until the test releases it.
    fn block(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.released {
            return;
        }
        state.blocked = true;
        self.ready.notify_all();
        while !state.released {
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Wait until a reader has reached the block point.
    fn wait_blocked(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while !state.blocked {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "the reader did not block within 5 s");
            let (next, timeout) = self
                .ready
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner);
            state = next;
            assert!(
                !timeout.timed_out() || state.blocked,
                "the reader did not block within 5 s"
            );
        }
    }

    /// Release the blocked reader.
    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.released = true;
        self.ready.notify_all();
    }
}
