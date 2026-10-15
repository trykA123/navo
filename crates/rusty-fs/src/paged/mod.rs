mod index;

use std::fs::File;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use rusty_core::paging::{ByteSource, LineIndex};

pub use index::index_lines;

pub type IndexResult = Result<Option<LineIndex>, String>;

pub const PAGE_SIZE: usize = 64 * 1024;
pub const CACHE_PAGES: usize = 32;
pub const INDEX_PROGRESS_EVERY: u64 = 8 * 1024 * 1024;

const PAGE: u64 = PAGE_SIZE as u64;

struct Page {
    number: u64,
    bytes: Vec<u8>,
}

struct Cache {
    len: u64,
    modified: Option<SystemTime>,
    pages: Vec<Page>,
    reads: u64,
    changed: bool,
}

pub struct PagedFile {
    file: File,
    cache: Mutex<Cache>,
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
            len: metadata.len(),
            modified: metadata.modified().ok(),
            pages: Vec::with_capacity(CACHE_PAGES),
            reads: 0,
            changed: false,
        };
        Ok(Self {
            file,
            cache: Mutex::new(cache),
        })
    }

    fn cache(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn take_changed(&self) -> bool {
        std::mem::take(&mut self.cache().changed)
    }

    #[must_use]
    pub fn disk_reads(&self) -> u64 {
        self.cache().reads
    }

    #[must_use]
    pub fn cached_pages(&self) -> usize {
        self.cache().pages.len()
    }

    fn check_change(&self, cache: &mut Cache) {
        let Ok(metadata) = self.file.metadata() else {
            return;
        };
        let modified = metadata.modified().ok();
        if metadata.len() != cache.len || modified != cache.modified {
            cache.len = metadata.len();
            cache.modified = modified;
            cache.pages.clear();
            cache.changed = true;
        }
    }

    fn load(&self, cache: &mut Cache, number: u64) -> Result<(), String> {
        self.check_change(cache);
        let mut bytes = vec![0u8; PAGE_SIZE];
        let count = read_file(&self.file, number * PAGE, &mut bytes)?;
        bytes.truncate(count);
        cache.reads += 1;
        if cache.pages.len() >= CACHE_PAGES {
            cache.pages.pop();
        }
        cache.pages.insert(0, Page { number, bytes });
        Ok(())
    }

    fn copy_from_page(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let mut cache = self.cache();
        if offset >= cache.len {
            return Ok(0);
        }
        let number = offset / PAGE;
        let within = usize::try_from(offset % PAGE).unwrap_or(0);
        let hit = cache
            .pages
            .iter()
            .position(|page| page.number == number && page.bytes.len() > within);
        match hit {
            Some(at) => {
                let page = cache.pages.remove(at);
                cache.pages.insert(0, page);
            }
            None => {
                cache.pages.retain(|page| page.number != number);
                self.load(&mut cache, number)?;
            }
        }
        let page = &cache.pages[0].bytes;
        let available = page.get(within..).unwrap_or_default();
        let count = available.len().min(buf.len());
        buf[..count].copy_from_slice(&available[..count]);
        Ok(count)
    }
}

impl ByteSource for PagedFile {
    fn len(&self) -> u64 {
        self.cache().len
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
