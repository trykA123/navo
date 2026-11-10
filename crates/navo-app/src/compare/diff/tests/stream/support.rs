//! Test sources and helpers for the Stream session.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use navo_compare::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_compare::text::Options;
use navo_core::{FileIdentity, ProviderId, ProviderPath};
use navo_ui::compare_diff::{Card, PairRequest, StreamSink, StreamUpdate};

use crate::compare::diff::load::LoadInput;
use crate::compare::diff::stream::LoadFn;
use crate::compare::diff::stream::memory::card_bytes;
use crate::compare::diff::viewed::{NoViewed, ViewedMarks};

/// One test pair's bytes and whether its source waits for the gate.
#[derive(Clone)]
pub(crate) struct PairData {
    pub left: Vec<u8>,
    pub right: Vec<u8>,
    pub blocked: bool,
}

/// The active loads seen by `MemorySource`, with their peak.
#[derive(Default)]
pub(crate) struct Concurrency {
    active: AtomicUsize,
    maximum: AtomicUsize,
}

impl Concurrency {
    /// Highest number of readers open at one time.
    pub(super) fn maximum(&self) -> usize {
        self.maximum.load(Ordering::SeqCst)
    }
}

/// A gate a blocked source waits on until the test releases it.
#[derive(Default)]
pub(crate) struct Gate {
    opened: Mutex<bool>,
    ready: Condvar,
    released: Mutex<bool>,
}

impl Gate {
    /// Wait until at least one blocked source has opened.
    pub(super) fn wait_opened(&self) {
        let mut opened = self.opened.lock().unwrap_or_else(PoisonError::into_inner);
        while !*opened {
            opened = self
                .ready
                .wait(opened)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Let blocked sources return their bytes.
    pub(super) fn release(&self) {
        *self.released.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.ready.notify_all();
    }

    fn wait(&self) {
        *self.opened.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.ready.notify_all();
        let mut released = self.released.lock().unwrap_or_else(PoisonError::into_inner);
        while !*released {
            released = self
                .ready
                .wait(released)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// A channel sink and its receiver.
pub(crate) fn channel() -> (StreamSink, Receiver<StreamUpdate>) {
    let (sender, receiver) = mpsc::channel();
    let sink: StreamSink = Arc::new(move |update| {
        let _ = sender.send(update);
    });
    (sink, receiver)
}

/// A pair request for a test item.
pub(super) fn request(id: u64) -> PairRequest {
    PairRequest {
        id,
        path: format!("item{id}.txt"),
        left: None,
        right: None,
    }
}

/// Settled Stream items from the test data's ids.
pub(crate) fn items(data: &HashMap<u64, PairData>, settled: bool) -> Vec<(PairRequest, bool)> {
    let mut ids: Vec<u64> = data.keys().copied().collect();
    ids.sort_unstable();
    ids.into_iter().map(|id| (request(id), settled)).collect()
}

/// A loader over in-memory pairs, with optional delay, concurrency count and gate.
pub(crate) fn load_fn(
    data: HashMap<u64, PairData>,
    delay: Duration,
    concurrency: Option<Arc<Concurrency>>,
    gate: Option<Arc<Gate>>,
) -> LoadFn {
    let data = Arc::new(data);
    Arc::new(move |request: &PairRequest| {
        let pair = data.get(&request.id)?;
        let left = source(&pair.left, delay, concurrency.clone(), blocked(pair, &gate));
        let right = source(
            &pair.right,
            delay,
            concurrency.clone(),
            blocked(pair, &gate),
        );
        Some(LoadInput {
            left: Some((left, entry(request.id, "left", pair.left.len()))),
            right: Some((right, entry(request.id, "right", pair.right.len()))),
            options: Options::default(),
            fallback: encoding_rs::UTF_8,
        })
    })
}

/// The Viewed lookup for tests that do not use a store.
pub(crate) fn not_viewed() -> Arc<dyn ViewedMarks> {
    Arc::new(NoViewed)
}

/// A loader with no delay, concurrency counter or gate.
pub(crate) fn simple_load(data: HashMap<u64, PairData>) -> LoadFn {
    load_fn(data, Duration::ZERO, None, None)
}

/// The D26 card-body size the session counts.
pub(super) fn card_size(card: &Card) -> u64 {
    card_bytes(card)
}

/// One pair with fully changed lines, every line `chars` long.
pub(crate) fn changed_pair(id: u64, lines: usize, chars: usize) -> PairData {
    let tail = "a".repeat(chars);
    let left = (0..lines)
        .map(|line| format!("item{id} line{line} {tail}\n"))
        .collect::<String>();
    let right = left.replace(&tail, &"b".repeat(chars));
    PairData {
        left: left.into_bytes(),
        right: right.into_bytes(),
        blocked: false,
    }
}

fn blocked(pair: &PairData, gate: &Option<Arc<Gate>>) -> Option<Arc<Gate>> {
    if pair.blocked { gate.clone() } else { None }
}

fn source(
    bytes: &[u8],
    delay: Duration,
    concurrency: Option<Arc<Concurrency>>,
    gate: Option<Arc<Gate>>,
) -> Arc<dyn SideSource> {
    Arc::new(MemorySource {
        bytes: bytes.to_vec(),
        delay,
        concurrency,
        gate,
        provider: ProviderId::local(),
    })
}

fn entry(id: u64, side: &str, size: usize) -> SideEntry {
    SideEntry::new(
        RelPath::new(ProviderPath::from_text(&format!("item{id}-{side}.txt"))),
        EntryKind::File,
        u64::try_from(size).unwrap_or(u64::MAX),
        None,
    )
}

/// An in-memory source that can wait, sleep and count concurrent opens.
struct MemorySource {
    bytes: Vec<u8>,
    delay: Duration,
    concurrency: Option<Arc<Concurrency>>,
    gate: Option<Arc<Gate>>,
    provider: ProviderId,
}

impl SideSource for MemorySource {
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
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        if let Some(concurrency) = &self.concurrency {
            let active = concurrency.active.fetch_add(1, Ordering::SeqCst) + 1;
            concurrency.maximum.fetch_max(active, Ordering::SeqCst);
        }
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Ok(Box::new(GuardedReader {
            cursor: Cursor::new(self.bytes.clone()),
            concurrency: self.concurrency.clone(),
        }))
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

/// A cursor that records when its reader closes.
struct GuardedReader {
    cursor: Cursor<Vec<u8>>,
    concurrency: Option<Arc<Concurrency>>,
}

impl Read for GuardedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buffer)
    }
}

impl Drop for GuardedReader {
    fn drop(&mut self) {
        if let Some(concurrency) = &self.concurrency {
            concurrency.active.fetch_sub(1, Ordering::SeqCst);
        }
    }
}
