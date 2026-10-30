use std::fs::File;
use std::path::Path;

use memmap2::Mmap;
use rusty_core::paging::{ByteSource, LineIndex, read_full};
use rusty_fs::PagedFile;
use xxhash_rust::xxh3::xxh3_64;

pub const BLOCK: usize = 1024 * 1024;
pub const BUCKETS: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate {
    pub hash: u64,
    pub line: u32,
}

pub struct Scanned {
    pub candidates: Vec<Candidate>,
    pub index: LineIndex,
    pub lines: u64,
    pub tracked_bytes: usize,
}

pub struct FileSource {
    file: File,
    len: u64,
}

impl FileSource {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let len = file
            .metadata()
            .map_err(|error| format!("{}: {error}", path.display()))?
            .len();
        Ok(Self { file, len })
    }

    pub fn file(&self) -> &File {
        &self.file
    }
}

impl ByteSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        read_file_at(&self.file, offset, buf)
    }
}

pub fn read_file_at(file: &File, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
    let mut filled = 0;
    while filled < buf.len() {
        let read = read_once(file, offset + filled as u64, &mut buf[filled..])
            .map_err(|error| format!("read at {offset}: {error}"))?;
        if read == 0 {
            break;
        }
        filled += read;
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

#[expect(
    unsafe_code,
    reason = "memmap2 mapping is unsafe by its API; the benchmark owns these generated files and never changes them while mapped"
)]
pub fn map_file(file: &File) -> Result<Mmap, String> {
    // SAFETY: the corpus files are private to this process and are not modified or truncated while a map lives.
    unsafe { Mmap::map(file) }.map_err(|error| format!("cannot map: {error}"))
}

pub fn scan_path(path: &Path, mode: IoMode) -> Result<Scanned, String> {
    match mode {
        IoMode::Paged => scan_source(&PagedFile::open(path)?, BUCKETS),
        IoMode::Positional => scan_source(&FileSource::open(path)?, BUCKETS),
        IoMode::Mapped => {
            let source = FileSource::open(path)?;
            let map = map_file(source.file())?;
            scan_bytes(&map, BUCKETS)
        }
    }
}

#[derive(Clone, Copy)]
pub struct ScanOptions {
    pub first_line: u32,
    pub buckets: u32,
    pub candidate_cap: usize,
}

pub fn scan_source<S: ByteSource + ?Sized>(source: &S, buckets: u32) -> Result<Scanned, String> {
    let options = ScanOptions {
        first_line: 0,
        buckets,
        candidate_cap: usize::MAX,
    };
    scan_range(source, 0, source.len(), options)
}

pub fn scan_range<S: ByteSource + ?Sized>(
    source: &S,
    from: u64,
    to: u64,
    options: ScanOptions,
) -> Result<Scanned, String> {
    let mut scanner = Scanner::new(options.buckets, options.first_line, options.candidate_cap);
    let mut buffer = vec![0_u8; BLOCK];
    let mut at = from;
    while at < to {
        let want = usize::try_from((to - at).min(BLOCK as u64)).map_err(|_| "block size")?;
        let read = read_full(source, at, &mut buffer[..want])?;
        if read == 0 {
            break;
        }
        scanner.feed(&buffer[..read]);
        at += read as u64;
    }
    scanner.finish()
}

pub fn scan_bytes(bytes: &[u8], buckets: u32) -> Result<Scanned, String> {
    let mut scanner = Scanner::new(buckets, 0, usize::MAX);
    for chunk in bytes.chunks(BLOCK) {
        scanner.feed(chunk);
    }
    scanner.finish()
}

struct Scanner {
    candidates: Vec<Candidate>,
    index: LineIndex,
    lines: u64,
    carry: Vec<u8>,
    buckets: u32,
    cap: usize,
    overflow: bool,
}

impl Scanner {
    fn new(buckets: u32, first_line: u32, cap: usize) -> Self {
        Self {
            candidates: Vec::new(),
            index: LineIndex::new(),
            lines: first_line.into(),
            carry: Vec::new(),
            buckets,
            cap,
            overflow: false,
        }
    }

    fn feed(&mut self, chunk: &[u8]) {
        self.index.feed(chunk);
        let mut start = 0;
        while let Some(found) = chunk[start..].iter().position(|byte| *byte == b'\n') {
            let end = start + found;
            self.complete_line(&chunk[start..end]);
            start = end + 1;
        }
        if let Some(rest) = chunk.get(start..) {
            self.carry.extend_from_slice(rest);
        }
    }

    fn complete_line(&mut self, tail: &[u8]) {
        if self.carry.is_empty() {
            self.record(tail);
        } else {
            self.carry.extend_from_slice(tail);
            let line = std::mem::take(&mut self.carry);
            self.record(&line);
            self.carry = line;
            self.carry.clear();
        }
    }

    fn record(&mut self, line: &[u8]) {
        let key = normalise(line);
        if key
            .iter()
            .filter(|byte| !byte.is_ascii_whitespace())
            .count()
            >= 8
        {
            self.record_hash(key);
        }
        self.lines += 1;
    }

    fn record_hash(&mut self, key: &[u8]) {
        let hash = xxh3_64(key);
        if !hash.is_multiple_of(u64::from(self.buckets)) {
            return;
        }
        if self.candidates.len() >= self.cap {
            self.overflow = true;
            return;
        }
        if let Ok(line) = u32::try_from(self.lines) {
            self.candidates.push(Candidate { hash, line });
        }
    }

    fn finish(mut self) -> Result<Scanned, String> {
        if !self.carry.is_empty() {
            let line = std::mem::take(&mut self.carry);
            self.record(&line);
        }
        self.index.finish();
        if self.overflow {
            return Err("anchor candidate cap reached".to_owned());
        }
        let tracked = self.candidates.capacity() * std::mem::size_of::<Candidate>()
            + self.index.marks() * 32
            + BLOCK;
        Ok(Scanned {
            candidates: self.candidates,
            index: self.index,
            lines: self.lines,
            tracked_bytes: tracked,
        })
    }
}

fn normalise(mut line: &[u8]) -> &[u8] {
    while let Some((first, rest)) = line.split_first() {
        if first.is_ascii_whitespace() {
            line = rest;
        } else {
            break;
        }
    }
    while let Some((last, rest)) = line.split_last() {
        if last.is_ascii_whitespace() {
            line = rest;
        } else {
            break;
        }
    }
    line
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoMode {
    Paged,
    Positional,
    Mapped,
}

impl IoMode {
    pub const ALL: [Self; 3] = [Self::Paged, Self::Positional, Self::Mapped];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Paged => "PagedFile",
            Self::Positional => "positional",
            Self::Mapped => "mmap",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_counts_lines_and_candidates() {
        let text = b"alpha line one\nbeta line two\n\nshort line\n";
        let scanned = scan_bytes(text, 1).unwrap();
        assert_eq!(scanned.lines, 4);
        assert_eq!(scanned.candidates.len(), 3);
        assert!(
            scanned
                .candidates
                .iter()
                .all(|candidate| candidate.line < 4)
        );
    }

    #[test]
    fn scan_handles_a_line_across_blocks() {
        let mut text = vec![b'x'; BLOCK];
        text.extend_from_slice(b"\ntail line after the block\n");
        let scanned = scan_bytes(&text, 1).unwrap();
        assert_eq!(scanned.lines, 2);
        assert_eq!(scanned.candidates.len(), 2);
    }

    #[test]
    fn normalise_trims_whitespace() {
        assert_eq!(normalise(b"  key value \r\n"), b"key value");
    }
}
