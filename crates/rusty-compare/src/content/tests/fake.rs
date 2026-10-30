use std::collections::HashMap;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusty_core::{FileIdentity, ProviderId, ProviderPath};

use crate::content::{CheckOptions, Side};
use crate::model::{
    CancelToken, ContentKey, EntryKind, HashCache, ObjectId, ReadHint, Reason, RelPath, SideEntry,
    SideSource, Stamp,
};

#[derive(Default)]
pub(super) struct Stats {
    opens: AtomicU64,
    bytes: AtomicU64,
    hints: Mutex<Vec<ReadHint>>,
    read_sizes: Mutex<Vec<usize>>,
}

impl Stats {
    fn opens(&self) -> u64 {
        self.opens.load(Ordering::Relaxed)
    }

    fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    pub(super) fn hints(&self) -> Vec<ReadHint> {
        self.hints.lock().unwrap().clone()
    }

    pub(super) fn read_sizes(&self) -> Vec<usize> {
        self.read_sizes.lock().unwrap().clone()
    }
}

struct FakeFile {
    bytes: Arc<Vec<u8>>,
    object_id: Option<ObjectId>,
    fail_at: Option<u64>,
    stamp: Stamp,
}

pub(super) struct FakeSource {
    provider: ProviderId,
    root: ProviderPath,
    files: HashMap<String, FakeFile>,
    stats: Arc<Stats>,
}

impl FakeSource {
    pub(super) fn stats(&self) -> Arc<Stats> {
        Arc::clone(&self.stats)
    }

    pub(super) fn with_root(mut self, root: &str) -> Self {
        self.root = ProviderPath::from_text(root);
        self
    }

    pub(super) fn with_object_id(mut self, name: &str, object_id: ObjectId) -> Self {
        self.files.get_mut(name).unwrap().object_id = Some(object_id);
        self
    }

    pub(super) fn failing_after(mut self, name: &str, offset: u64) -> Self {
        self.files.get_mut(name).unwrap().fail_at = Some(offset);
        self
    }

    pub(super) fn set_bytes(&mut self, name: &str, bytes: Vec<u8>) {
        self.files.get_mut(name).unwrap().bytes = Arc::new(bytes);
    }

    pub(super) fn set_stamp(&mut self, name: &str, stamp: Stamp) {
        self.files.get_mut(name).unwrap().stamp = stamp;
    }
}

impl SideSource for FakeSource {
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

    fn open(&self, rel: &RelPath, hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let file = self
            .files
            .get(&rel.as_path().to_text())
            .ok_or(Reason::NotFound)?;
        self.stats.opens.fetch_add(1, Ordering::Relaxed);
        self.stats.hints.lock().unwrap().push(hint);
        Ok(Box::new(CountedReader {
            bytes: Arc::clone(&file.bytes),
            position: 0,
            fail_at: file.fail_at,
            stats: Arc::clone(&self.stats),
        }))
    }

    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        self.files
            .get(&rel.as_path().to_text())
            .map(|file| file.stamp)
            .ok_or(Reason::NotFound)
    }

    fn object_id(&self, rel: &RelPath) -> Option<ObjectId> {
        self.files
            .get(&rel.as_path().to_text())
            .and_then(|file| file.object_id)
    }

    fn file_id(&self, rel: &RelPath) -> Option<FileIdentity> {
        self.files
            .get(&rel.as_path().to_text())
            .and_then(|file| file.stamp.file_id)
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        ProviderPath::from_text(&format!(
            "{}/{}",
            self.root.to_text(),
            rel.as_path().to_text()
        ))
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

struct CountedReader {
    bytes: Arc<Vec<u8>>,
    position: u64,
    fail_at: Option<u64>,
    stats: Arc<Stats>,
}

impl Read for CountedReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.stats.read_sizes.lock().unwrap().push(out.len());
        if self.fail_at.is_some_and(|fail| self.position >= fail) {
            return Err(io::Error::other("fake read error"));
        }
        let start = usize::try_from(self.position).unwrap_or(usize::MAX);
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let available = self.bytes.len() - start;
        let limit = self.fail_at.map_or(available, |fail| {
            usize::try_from(fail.saturating_sub(self.position)).unwrap_or(usize::MAX)
        });
        let read = available.min(out.len()).min(limit);
        if read == 0 {
            return Ok(0);
        }
        out[..read].copy_from_slice(&self.bytes[start..start + read]);
        self.position += u64::try_from(read).unwrap_or(u64::MAX);
        self.stats
            .bytes
            .fetch_add(u64::try_from(read).unwrap_or(u64::MAX), Ordering::Relaxed);
        Ok(read)
    }
}

#[derive(Default)]
pub(super) struct FakeCache {
    entries: Mutex<Vec<(ContentKey, [u8; 32])>>,
    gets: AtomicU64,
    puts: AtomicU64,
}

impl FakeCache {
    pub(super) fn with(key: ContentKey, hash: [u8; 32]) -> Self {
        let cache = Self::default();
        cache.seed(key, hash);
        cache
    }

    pub(super) fn seed(&self, key: ContentKey, hash: [u8; 32]) {
        self.entries.lock().unwrap().push((key, hash));
    }

    pub(super) fn lookup(&self, key: &ContentKey) -> Option<[u8; 32]> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|(stored, _)| stored == key)
            .map(|(_, hash)| *hash)
    }

    pub(super) fn gets(&self) -> u64 {
        self.gets.load(Ordering::Relaxed)
    }

    pub(super) fn puts(&self) -> u64 {
        self.puts.load(Ordering::Relaxed)
    }
}

impl HashCache for FakeCache {
    fn get(&self, key: &ContentKey) -> Option<[u8; 32]> {
        self.gets.fetch_add(1, Ordering::Relaxed);
        self.lookup(key)
    }

    fn put(&self, key: &ContentKey, hash: [u8; 32]) {
        self.puts.fetch_add(1, Ordering::Relaxed);
        let mut entries = self.entries.lock().unwrap();
        if let Some((_, stored)) = entries.iter_mut().find(|(stored, _)| stored == key) {
            *stored = hash;
        } else {
            entries.push((key.clone(), hash));
        }
    }
}

pub(super) fn source(name: &str, bytes: Vec<u8>) -> FakeSource {
    let stamp = Stamp {
        size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        modified: None,
        file_id: None,
    };
    let mut files = HashMap::new();
    files.insert(
        name.to_owned(),
        FakeFile {
            bytes: Arc::new(bytes),
            object_id: None,
            fail_at: None,
            stamp,
        },
    );
    FakeSource {
        provider: ProviderId::new("local").unwrap(),
        root: ProviderPath::from_text("root"),
        files,
        stats: Arc::new(Stats::default()),
    }
}

fn rel(name: &str) -> RelPath {
    RelPath::new(ProviderPath::from_text(name))
}

pub(super) fn entry(name: &str, size: u64) -> SideEntry {
    SideEntry::new(rel(name), EntryKind::File, size, None)
}

pub(super) fn kind_entry(name: &str, kind: EntryKind) -> SideEntry {
    SideEntry::new(rel(name), kind, 0, None)
}

pub(super) fn side<'a>(entry: &'a SideEntry, source: &'a FakeSource) -> Side<'a> {
    Side { entry, source }
}

pub(super) fn options<'a>(cache: &'a FakeCache, cancel: &'a CancelToken) -> CheckOptions<'a> {
    CheckOptions {
        cache,
        cancel,
        read_hint: ReadHint::Buffered,
        known_same: false,
    }
}

pub(super) fn assert_reads(stats: &Stats, opens: u64, bytes: u64) {
    assert_eq!(stats.opens(), opens, "opens");
    assert_eq!(stats.bytes(), bytes, "bytes");
}
