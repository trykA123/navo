use std::collections::HashMap;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use rusty_core::{FileIdentity, ProviderId, ProviderPath};

use crate::model::{
    CancelToken, EntryKind, ObjectId, Reason, RelPath, SideEntry, SideSource, Stamp,
};

#[derive(Clone)]
struct SlowListing {
    every: usize,
    delay: Duration,
    started: Arc<AtomicBool>,
}

pub(super) struct SlowSpec {
    pub count: usize,
    pub every: usize,
    pub delay: Duration,
    pub started: Arc<AtomicBool>,
}

pub(super) struct ReadTracking {
    pub delay: Duration,
    pub marker: Arc<AtomicBool>,
    pub total: Arc<AtomicU64>,
    pub after: Arc<AtomicU64>,
}

pub(super) struct FakeSource {
    provider: ProviderId,
    root: ProviderPath,
    entries: HashMap<String, Vec<SideEntry>>,
    files: HashMap<String, Arc<Vec<u8>>>,
    stamps: HashMap<String, Stamp>,
    list_failures: HashMap<String, Reason>,
    slow_listings: HashMap<String, SlowListing>,
    open_delay: Option<Duration>,
    panic_once: Arc<AtomicBool>,
    panic_list_once: Arc<AtomicBool>,
    read_delay: Option<Duration>,
    read_marker: Option<Arc<AtomicBool>>,
    reads_total: Arc<AtomicU64>,
    reads_after: Arc<AtomicU64>,
    case_sensitive: bool,
}

impl FakeSource {
    pub(super) fn new(case_sensitive: bool) -> Self {
        Self {
            provider: ProviderId::new("fake").unwrap(),
            root: ProviderPath::from_text("root"),
            entries: HashMap::new(),
            files: HashMap::new(),
            stamps: HashMap::new(),
            list_failures: HashMap::new(),
            slow_listings: HashMap::new(),
            open_delay: None,
            panic_once: Arc::new(AtomicBool::new(false)),
            panic_list_once: Arc::new(AtomicBool::new(false)),
            read_delay: None,
            read_marker: None,
            reads_total: Arc::new(AtomicU64::new(0)),
            reads_after: Arc::new(AtomicU64::new(0)),
            case_sensitive,
        }
    }

    pub(super) fn add_folder(&mut self, rel: &str) {
        let path = RelPath::new(ProviderPath::from_text(rel));
        self.push_entry(SideEntry::new(path, EntryKind::Folder, 0, None));
        self.entries.entry(rel.to_owned()).or_default();
    }

    pub(super) fn add_file(&mut self, rel: &str, bytes: Vec<u8>) {
        let size = u64::try_from(bytes.len()).unwrap();
        let path = RelPath::new(ProviderPath::from_text(rel));
        self.push_entry(SideEntry::new(path, EntryKind::File, size, None));
        self.files.insert(rel.to_owned(), Arc::new(bytes));
        self.stamps.insert(
            rel.to_owned(),
            Stamp {
                size,
                modified: None,
                file_id: None,
            },
        );
    }

    pub(super) fn add_slow_folder(&mut self, rel: &str, spec: SlowSpec) {
        self.add_folder(rel);
        let entries = (0..spec.count)
            .map(|index| {
                SideEntry::new(
                    RelPath::new(ProviderPath::from_text(&format!("{rel}/s{index:06}.bin"))),
                    EntryKind::File,
                    0,
                    None,
                )
            })
            .collect();
        self.entries.insert(rel.to_owned(), entries);
        self.slow_listings.insert(
            rel.to_owned(),
            SlowListing {
                every: spec.every.max(1),
                delay: spec.delay,
                started: spec.started,
            },
        );
    }

    pub(super) fn fail_listing(&mut self, rel: &str, reason: Reason) {
        self.list_failures.insert(rel.to_owned(), reason);
    }

    pub(super) fn panic_on_next_open(&self) {
        self.panic_once.store(true, Ordering::Release);
    }

    pub(super) fn panic_on_next_list(&self) {
        self.panic_list_once.store(true, Ordering::Release);
    }

    pub(super) fn set_open_delay(&mut self, delay: Duration) {
        self.open_delay = Some(delay);
    }

    pub(super) fn track_reads(&mut self, tracking: ReadTracking) {
        self.read_delay = Some(tracking.delay);
        self.read_marker = Some(tracking.marker);
        self.reads_total = tracking.total;
        self.reads_after = tracking.after;
    }

    fn push_entry(&mut self, entry: SideEntry) {
        let full = entry.rel.as_path().to_text();
        let folder = full.rsplit_once('/').map_or("", |(folder, _)| folder);
        self.entries
            .entry(folder.to_owned())
            .or_default()
            .push(entry);
    }
}

impl SideSource for FakeSource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        if self.panic_list_once.swap(false, Ordering::AcqRel) {
            panic!("fake source list panic");
        }
        let key = folder.as_path().to_text();
        if let Some(reason) = self.list_failures.get(&key) {
            return Err(*reason);
        }
        let entries = self.entries.get(&key).cloned().unwrap_or_default();
        let slow = self.slow_listings.get(&key).cloned();
        for (index, entry) in entries.into_iter().enumerate() {
            if let Some(slow) = &slow {
                slow_tick(slow, index);
            }
            if cancel.is_cancelled() {
                break;
            }
            if sink(entry).is_break() {
                break;
            }
        }
        Ok(())
    }

    fn open(
        &self,
        rel: &RelPath,
        _hint: crate::model::ReadHint,
    ) -> Result<Box<dyn Read + Send>, Reason> {
        if self.panic_once.swap(false, Ordering::AcqRel) {
            panic!("fake source open panic");
        }
        if let Some(delay) = self.open_delay {
            thread::sleep(delay);
        }
        let bytes = self
            .files
            .get(&rel.as_path().to_text())
            .ok_or(Reason::NotFound)?;
        Ok(Box::new(FakeReader {
            bytes: (**bytes).clone(),
            position: 0,
            delay: self.read_delay,
            marker: self.read_marker.clone(),
            total: Arc::clone(&self.reads_total),
            after: Arc::clone(&self.reads_after),
        }))
    }

    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        self.stamps
            .get(&rel.as_path().to_text())
            .copied()
            .ok_or(Reason::NotFound)
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        ProviderPath::from_text(&format!(
            "{}/{}",
            self.root.to_text(),
            rel.as_path().to_text()
        ))
    }

    fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    fn read_only(&self) -> bool {
        false
    }
}

struct FakeReader {
    bytes: Vec<u8>,
    position: usize,
    delay: Option<Duration>,
    marker: Option<Arc<AtomicBool>>,
    total: Arc<AtomicU64>,
    after: Arc<AtomicU64>,
}

fn slow_tick(slow: &SlowListing, index: usize) {
    if index == 0 {
        slow.started.store(true, Ordering::Release);
    }
    if index.is_multiple_of(slow.every) {
        thread::sleep(slow.delay);
    }
}

impl Read for FakeReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self
            .marker
            .as_ref()
            .is_some_and(|marker| marker.load(Ordering::Acquire))
        {
            self.after.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(delay) = self.delay {
            thread::sleep(delay);
        }
        let remaining = self.bytes.len().saturating_sub(self.position);
        let read = remaining.min(out.len());
        if read == 0 {
            return Ok(0);
        }
        out[..read].copy_from_slice(&self.bytes[self.position..self.position + read]);
        self.position += read;
        self.total.fetch_add(1, Ordering::Relaxed);
        Ok(read)
    }
}
