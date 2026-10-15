mod index;

use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use rusty_core::paging::{ByteSource, LineIndex};

pub use index::index_lines;

pub type IndexResult = Result<Option<LineIndex>, String>;

pub const PAGE_SIZE: usize = 64 * 1024;
pub const CACHE_PAGES: usize = 32;
pub const INDEX_PROGRESS_EVERY: u64 = 8 * 1024 * 1024;

const PAGE: u64 = PAGE_SIZE as u64;
const END_CHECK_EVERY: Duration = Duration::from_millis(250);

struct Page {
    number: u64,
    bytes: Vec<u8>,
}

struct Cache {
    modified: Option<SystemTime>,
    pages: Vec<Page>,
    reads: u64,
    end_checked: Option<Instant>,
}

pub struct PagedFile {
    file: File,
    len: AtomicU64,
    changed: AtomicBool,
    cache: Mutex<Cache>,
}

struct Stat {
    len: u64,
    modified: Option<SystemTime>,
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
            modified: metadata.modified().ok(),
            pages: Vec::with_capacity(CACHE_PAGES),
            reads: 0,
            end_checked: None,
        };
        Ok(Self {
            file,
            len: AtomicU64::new(metadata.len()),
            changed: AtomicBool::new(false),
            cache: Mutex::new(cache),
        })
    }

    fn cache(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn disk_reads(&self) -> u64 {
        self.cache().reads
    }

    #[must_use]
    pub fn cached_pages(&self) -> usize {
        self.cache().pages.len()
    }

    fn stat(&self) -> Option<Stat> {
        let metadata = self.file.metadata().ok()?;
        Some(Stat {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    fn apply(&self, cache: &mut Cache, stat: Option<Stat>) {
        let Some(stat) = stat else {
            return;
        };
        if stat.len != self.len.load(Ordering::Acquire) || stat.modified != cache.modified {
            self.len.store(stat.len, Ordering::Release);
            cache.modified = stat.modified;
            cache.pages.clear();
            self.changed.store(true, Ordering::Release);
        }
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
            let stat = self.stat();
            self.apply(&mut self.cache(), stat);
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
        let stat = self.stat();
        let mut bytes = vec![0u8; PAGE_SIZE];
        let read = read_file(&self.file, number * PAGE, &mut bytes)?;
        bytes.truncate(read);
        let count = copy_out(&bytes, within, buf);
        let mut cache = self.cache();
        self.apply(&mut cache, stat);
        cache.reads += 1;
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
    fn len(&self) -> u64 {
        self.len.load(Ordering::Acquire)
    }

    fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
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
