use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use blake3::Hasher;
use rusty_core::{ProviderId, ProviderPath};

use super::fake::{FakeCache, FakeSource, entry, options, side, source};
use crate::content::read::content_key;
use crate::content::{
    CONTENT_BLOCK, ContentResult, LARGE_CONTENT_THRESHOLD, PARALLEL_CONTENT_BLOCK, Side, check,
};
use crate::model::{
    CancelToken, ContentClass, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp, Status,
};

const PAIR_SIZE: u64 = 200 * 1024 * 1024;
const DIFF_AT: u64 = 150 * 1024 * 1024 + 123;
const FILL: u8 = 0x41;
const DIFF_FILL: u8 = 0x42;

fn sized(name: &str, bytes: Vec<u8>, size: u64) -> FakeSource {
    let mut source = source(name, bytes);
    source.set_stamp(
        name,
        Stamp {
            size,
            modified: None,
            file_id: None,
        },
    );
    source
}

fn generated(path: &Path) {
    let file = File::create(path).expect("pair file created");
    let mut writer = BufWriter::new(file);
    let block = vec![FILL; CONTENT_BLOCK];
    let mut left = PAIR_SIZE;
    while left > 0 {
        let take = usize::try_from(left.min(CONTENT_BLOCK as u64)).expect("block fits");
        writer.write_all(&block[..take]).expect("pair file written");
        left -= take as u64;
    }
    writer.flush().expect("pair file flushed");
}

fn single_hash(path: &Path) -> [u8; 32] {
    let mut file = File::open(path).expect("pair file opened");
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; CONTENT_BLOCK];
    loop {
        let read = file.read(&mut buffer).expect("pair file read");
        if read == 0 {
            return *hasher.finalize().as_bytes();
        }
        hasher.update(&buffer[..read]);
    }
}

struct FileSource {
    provider: ProviderId,
    root: ProviderPath,
    path: PathBuf,
    name: String,
    stamp: Stamp,
}

impl FileSource {
    fn new(root: &str, name: &str, path: &Path) -> Self {
        Self {
            provider: ProviderId::new("local").expect("provider id"),
            root: ProviderPath::from_text(root),
            path: path.to_owned(),
            name: name.to_owned(),
            stamp: Stamp {
                size: PAIR_SIZE,
                modified: None,
                file_id: None,
            },
        }
    }
}

impl SideSource for FileSource {
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
        File::open(&self.path)
            .map(|file| Box::new(file) as Box<dyn Read + Send>)
            .map_err(|_| Reason::ReadError)
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Ok(self.stamp)
    }

    fn object_id(&self, _rel: &RelPath) -> Option<crate::model::ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<rusty_core::FileIdentity> {
        None
    }

    fn cache_path(&self, _rel: &RelPath) -> ProviderPath {
        ProviderPath::from_text(&format!("{}/{}", self.root.to_text(), self.name))
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        true
    }
}

fn threshold_read_sizes(size: u64) -> Vec<usize> {
    let entry = entry("a.bin", size);
    let left = sized("a.bin", vec![0xAA; CONTENT_BLOCK], size);
    let right = sized("a.bin", vec![0xAA; CONTENT_BLOCK], size);
    let stats = left.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let result = check(
        side(&entry, &left),
        side(&entry, &right),
        &options(&cache, &cancel),
        &mut |_| {},
    );
    assert_eq!(result.status, Status::Unavailable(Reason::ChangedOnDisk));
    stats.read_sizes()
}

#[test]
fn exactly_the_threshold_stays_on_the_small_buffer() {
    let below = threshold_read_sizes(LARGE_CONTENT_THRESHOLD);
    assert!(below.contains(&CONTENT_BLOCK));
    assert!(below.iter().all(|size| *size <= CONTENT_BLOCK));

    let above = threshold_read_sizes(LARGE_CONTENT_THRESHOLD + 1);
    assert!(above.contains(&PARALLEL_CONTENT_BLOCK));
    assert!(above.iter().all(|size| *size <= PARALLEL_CONTENT_BLOCK));
}

#[test]
fn cancel_stops_after_one_large_block() {
    let size = LARGE_CONTENT_THRESHOLD + 1;
    let entry = entry("a.bin", size);
    let bytes = vec![0x11; PARALLEL_CONTENT_BLOCK];
    let left = sized("a.bin", bytes.clone(), size);
    let right = sized("a.bin", bytes, size);
    let stats = left.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let stop = cancel.clone();
    let mut events = Vec::new();
    let result = check(
        side(&entry, &left),
        side(&entry, &right),
        &options(&cache, &cancel),
        &mut |event| {
            events.push(event);
            stop.cancel();
        },
    );
    assert_eq!(result.status, Status::NotChecked(Reason::Stopped));
    assert_eq!(events.len(), 1);
    assert_eq!(
        stats.read_sizes().iter().sum::<usize>(),
        PARALLEL_CONTENT_BLOCK
    );
}

struct Pair {
    _dir: tempfile::TempDir,
    left_path: PathBuf,
    right_path: PathBuf,
    left_entry: SideEntry,
    right_entry: SideEntry,
    left_source: FileSource,
    right_source: FileSource,
    cancel: CancelToken,
}

impl Pair {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("temp dir created");
        let left_path = dir.path().join("left.bin");
        let right_path = dir.path().join("right.bin");
        generated(&left_path);
        std::fs::copy(&left_path, &right_path).expect("pair file copied");
        Self {
            left_entry: entry("a.bin", PAIR_SIZE),
            right_entry: entry("a.bin", PAIR_SIZE),
            left_source: FileSource::new("left", "a.bin", &left_path),
            right_source: FileSource::new("right", "a.bin", &right_path),
            cancel: CancelToken::new(),
            _dir: dir,
            left_path,
            right_path,
        }
    }

    fn check(&self, cache: &FakeCache) -> ContentResult {
        check(
            Side {
                entry: &self.left_entry,
                source: &self.left_source,
            },
            Side {
                entry: &self.right_entry,
                source: &self.right_source,
            },
            &options(cache, &self.cancel),
            &mut |_| {},
        )
    }

    fn modify_right(&self) {
        let mut file = OpenOptions::new()
            .write(true)
            .open(&self.right_path)
            .expect("pair file opened for edit");
        file.seek(SeekFrom::Start(DIFF_AT)).expect("pair file seek");
        file.write_all(&[DIFF_FILL]).expect("pair file edited");
    }
}

#[test]
fn large_pairs_match_the_single_thread_hashes() {
    let pair = Pair::new();
    let cache = FakeCache::default();
    let equal = pair.check(&cache);
    let expected = single_hash(&pair.left_path);
    assert_eq!(equal.status, Status::Same);
    assert_eq!(equal.content, ContentClass::Text);
    assert_eq!(
        cache.lookup(&content_key(&pair.left_source, &pair.left_entry)),
        Some(expected)
    );
    assert_eq!(
        cache.lookup(&content_key(&pair.right_source, &pair.right_entry)),
        Some(expected)
    );

    pair.modify_right();
    let unequal = pair.check(&FakeCache::default());
    assert_eq!(
        unequal.status,
        Status::Different {
            first_difference: Some(DIFF_AT)
        }
    );
    assert_eq!(unequal.content, ContentClass::Unknown);
}
