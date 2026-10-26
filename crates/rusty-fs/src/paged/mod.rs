mod identity;
mod index;

use std::fs::{File, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime};

use rusty_core::paging::{ByteSource, LineIndex};

pub use index::index_lines;

pub type IndexResult = Result<Option<LineIndex>, String>;

pub const PAGE_SIZE: usize = 64 * 1024;
pub const CACHE_PAGES: usize = 32;
pub const INDEX_PROGRESS_EVERY: u64 = 8 * 1024 * 1024;

const PAGE: u64 = PAGE_SIZE as u64;
const END_CHECK_EVERY: Duration = Duration::from_millis(250);
const TAIL_CHECK: u64 = 64;

struct Page {
    number: u64,
    bytes: Vec<u8>,
}

struct Cache {
    applied: u64,
    modified: Option<SystemTime>,
    pages: Vec<Page>,
    reads: u64,
    end_checked: Option<Instant>,
    read_end: u64,
}

pub struct PagedFile {
    path: PathBuf,
    file: RwLock<Arc<File>>,
    len: AtomicU64,
    changed: AtomicBool,
    rewritten: AtomicBool,
    path_opens: AtomicU64,
    stats: AtomicU64,
    cache: Mutex<Cache>,
}

struct Stat {
    seq: u64,
    len: u64,
    modified: Option<SystemTime>,
    replacement: Option<File>,
}

impl PagedFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("Cannot open it: {error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("Cannot open it: {error}"))?;
        if !metadata.is_file() {
            return Err("Only files open here.".to_owned());
        }
        let cache = Cache {
            applied: 0,
            modified: metadata.modified().ok(),
            pages: Vec::with_capacity(CACHE_PAGES),
            reads: 0,
            end_checked: None,
            read_end: 0,
        };
        Ok(Self {
            path: path.to_owned(),
            file: RwLock::new(Arc::new(file)),
            len: AtomicU64::new(metadata.len()),
            changed: AtomicBool::new(false),
            rewritten: AtomicBool::new(false),
            path_opens: AtomicU64::new(0),
            stats: AtomicU64::new(1),
            cache: Mutex::new(cache),
        })
    }

    fn handle(&self) -> Arc<File> {
        self.file
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn replacement(&self, held: &File, held_metadata: &Metadata) -> Option<(File, Metadata)> {
        self.path_opens.fetch_add(1, Ordering::Relaxed);
        let file = File::open(&self.path).ok()?;
        let metadata = file.metadata().ok()?;
        let same = identity::same_identity(held, &file);
        let is_replaced = identity::is_replaced(same, held_metadata, &metadata);
        (metadata.is_file() && is_replaced).then_some((file, metadata))
    }

    fn cache(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn disk_reads(&self) -> u64 {
        self.cache().reads
    }

    #[must_use]
    pub fn path_opens(&self) -> u64 {
        self.path_opens.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn cached_pages(&self) -> usize {
        self.cache().pages.len()
    }

    fn stat(&self, probes_path: bool) -> Option<Stat> {
        let seq = self.stats.fetch_add(1, Ordering::AcqRel);
        let file = self.handle();
        let held = file.metadata().ok()?;
        let found = probes_path
            .then(|| self.replacement(&file, &held))
            .flatten();
        let (metadata, replacement) = match found {
            Some((file, metadata)) => (metadata, Some(file)),
            None => (held, None),
        };
        Some(Stat {
            seq,
            len: metadata.len(),
            modified: metadata.modified().ok(),
            replacement,
        })
    }

    fn tail_changed(&self, cache: &Cache, old_len: u64, new_len: u64) -> bool {
        if new_len < old_len {
            return true;
        }
        let Some(last) = old_len.checked_sub(1) else {
            return false;
        };
        let number = last / PAGE;
        let from = old_len.saturating_sub(TAIL_CHECK).max(number * PAGE);
        let span = |at: u64| usize::try_from(at - number * PAGE).unwrap_or(usize::MAX);
        let kept = cache
            .pages
            .iter()
            .find(|page| page.number == number)
            .and_then(|page| page.bytes.get(span(from)..span(old_len)));
        let Some(kept) = kept else {
            return false;
        };
        let mut now = vec![0u8; kept.len()];
        read_file(&self.handle(), from, &mut now).is_ok_and(|read| now[..read] != *kept)
    }

    fn apply(&self, cache: &mut Cache, stat: Option<Stat>) -> bool {
        let Some(mut stat) = stat else {
            return true;
        };
        if stat.seq < cache.applied {
            return false;
        }
        cache.applied = stat.seq;
        if stat.replacement.is_some() {
            cache.read_end = 0;
        }
        if stat.len < cache.read_end {
            let mut byte = [0];
            if read_file(&self.handle(), cache.read_end - 1, &mut byte).is_ok_and(|read| read != 0)
            {
                stat.len = cache.read_end;
            } else {
                cache.read_end = 0;
            }
        }
        let old_len = self.len.load(Ordering::Acquire);
        let is_rewrite = match stat.replacement {
            Some(file) => {
                *self.file.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(file);
                cache.modified = None;
                true
            }
            None => false,
        };
        if is_rewrite || stat.len != old_len || stat.modified != cache.modified {
            if is_rewrite || self.tail_changed(cache, old_len, stat.len) {
                self.rewritten.store(true, Ordering::Release);
            }
            self.len.store(stat.len, Ordering::Release);
            cache.modified = stat.modified;
            cache.pages.clear();
            self.changed.store(true, Ordering::Release);
        }
        true
    }

    fn probe_end_with(&self, stat: Option<Stat>) -> Result<(), String> {
        let mut cache = self.cache();
        let old_end = self.len.load(Ordering::Acquire);
        let is_replaced = stat.as_ref().is_some_and(|stat| stat.replacement.is_some());
        if !self.apply(&mut cache, stat) {
            return Ok(());
        }
        cache.end_checked = Some(Instant::now());
        let len = self.len.load(Ordering::Acquire);
        let offset = if is_replaced { len } else { old_end.min(len) };
        let mut bytes = vec![0; PAGE_SIZE];
        let read = read_file(&self.handle(), offset, &mut bytes)?;
        cache.reads += 1;
        if read != 0 {
            let end = offset + read as u64;
            if self.tail_changed(&cache, offset, end) {
                self.rewritten.store(true, Ordering::Release);
            }
            cache.read_end = end;
            self.len.store(len.max(end), Ordering::Release);
            cache.pages.clear();
            self.changed.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn past_end(&self, offset: u64) -> bool {
        if offset < self.len.load(Ordering::Acquire) {
            return false;
        }
        let due = {
            let mut cache = self.cache();
            let due = cache
                .end_checked
                .is_none_or(|checked| checked.elapsed() >= END_CHECK_EVERY);
            if due {
                cache.end_checked = Some(Instant::now());
            }
            due
        };
        if due {
            let stat = self.stat(true);
            let _ = self.apply(&mut self.cache(), stat);
        }
        offset >= self.len.load(Ordering::Acquire)
    }

    fn copy_cached(&self, number: u64, within: usize, buf: &mut [u8]) -> Option<usize> {
        let mut cache = self.cache();
        let at = cache
            .pages
            .iter()
            .position(|page| page.number == number && page.bytes.len() > within)?;
        let page = cache.pages.remove(at);
        let count = copy_out(&page.bytes, within, buf);
        cache.pages.insert(0, page);
        Some(count)
    }

    fn copy_from_page(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if self.past_end(offset) {
            return Ok(0);
        }
        let number = offset / PAGE;
        let within = usize::try_from(offset % PAGE).unwrap_or(0);
        if let Some(count) = self.copy_cached(number, within, buf) {
            return Ok(count);
        }
        let stat = self.stat(false);
        let mut bytes = vec![0u8; PAGE_SIZE];
        let read = read_file(&self.handle(), number * PAGE, &mut bytes)?;
        bytes.truncate(read);
        let count = copy_out(&bytes, within, buf);
        let mut cache = self.cache();
        cache.reads += 1;
        if !self.apply(&mut cache, stat) {
            return Ok(count);
        }
        cache.pages.retain(|page| page.number != number);
        if cache.pages.len() >= CACHE_PAGES {
            cache.pages.pop();
        }
        cache.pages.insert(0, Page { number, bytes });
        Ok(count)
    }
}

fn copy_out(page: &[u8], within: usize, buf: &mut [u8]) -> usize {
    let available = page.get(within..).unwrap_or_default();
    let count = available.len().min(buf.len());
    buf[..count].copy_from_slice(&available[..count]);
    count
}

impl ByteSource for PagedFile {
    fn probe_end(&self) -> Result<(), String> {
        self.probe_end_with(self.stat(true))
    }

    fn len(&self) -> u64 {
        self.len.load(Ordering::Acquire)
    }

    fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }

    fn take_rewritten(&self) -> bool {
        self.rewritten.swap(false, Ordering::AcqRel)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let mut filled = 0;
        while filled < buf.len() {
            let count = self.copy_from_page(offset + filled as u64, &mut buf[filled..])?;
            if count == 0 {
                break;
            }
            filled += count;
        }
        Ok(filled)
    }
}

fn read_file(file: &File, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
    let mut filled = 0;
    while filled < buf.len() {
        let count = read_once(file, offset + filled as u64, &mut buf[filled..])
            .map_err(|error| format!("Cannot read it: {error}"))?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    Ok(filled)
}

#[cfg(unix)]
fn read_once(file: &File, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn read_once(file: &File, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_index;
