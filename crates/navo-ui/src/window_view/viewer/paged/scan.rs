//! Whole-file digests (CRC32, SHA-256, BLAKE3) and a strings list for the hex inspector.
//! Pure: it streams any byte source in chunks; the drawer runs it off the UI thread.

use std::sync::atomic::{AtomicBool, Ordering};

use navo_core::paging::ByteSource;
use sha2::{Digest as _, Sha256};

/// Bytes read per step: 1 MiB keeps memory flat for any file size.
pub(in crate::window_view::viewer) const SCAN_CHUNK: usize = 1024 * 1024;
/// The shortest run of printable bytes that counts as a string.
pub(in crate::window_view::viewer) const MIN_RUN: usize = 4;
/// The most strings the list keeps; later ones are dropped and the list says so.
pub(in crate::window_view::viewer) const MAX_STRINGS: usize = 10_000;
/// The most characters kept of one string; a longer one ends with an ellipsis.
const MAX_TEXT: usize = 200;

/// The three digests as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Digests {
    pub crc32: String,
    pub sha256: String,
    pub blake3: String,
}

/// One run of printable ASCII bytes and the offset of its first byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct FoundString {
    pub offset: u64,
    pub text: String,
}

/// What one pass over the file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Scanned {
    pub digests: Digests,
    pub strings: Vec<FoundString>,
    /// True when the file holds more strings than the list keeps.
    pub capped: bool,
}

/// Collects runs of at least `MIN_RUN` printable ASCII bytes (0x20 to 0x7E) across chunks.
pub(in crate::window_view::viewer) struct StringScan {
    cap: usize,
    found: Vec<FoundString>,
    capped: bool,
    start: u64,
    run: Vec<u8>,
    run_len: usize,
}

impl StringScan {
    pub(in crate::window_view::viewer) fn new(cap: usize) -> Self {
        Self {
            cap,
            found: Vec::new(),
            capped: false,
            start: 0,
            run: Vec::new(),
            run_len: 0,
        }
    }

    /// Feeds `bytes` that start at file offset `at`; chunks must arrive in order.
    pub(in crate::window_view::viewer) fn feed(&mut self, at: u64, bytes: &[u8]) {
        for (step, &byte) in bytes.iter().enumerate() {
            self.push(at + step as u64, byte);
        }
    }

    /// Adds the byte at file offset `at` to the current run, or ends the run.
    fn push(&mut self, at: u64, byte: u8) {
        if !(0x20..=0x7E).contains(&byte) {
            self.end_run();
            return;
        }
        if self.run_len == 0 {
            self.start = at;
        }
        if self.run.len() < MAX_TEXT {
            self.run.push(byte);
        }
        self.run_len += 1;
    }

    /// Ends the current run and keeps it when it is long enough and the list has room.
    fn end_run(&mut self) {
        let is_string = self.run_len >= MIN_RUN;
        if is_string && self.found.len() >= self.cap {
            self.capped = true;
        } else if is_string {
            let mut text: String = self.run.iter().map(|&byte| char::from(byte)).collect();
            if self.run_len > self.run.len() {
                text.push('…');
            }
            self.found.push(FoundString {
                offset: self.start,
                text,
            });
        }
        self.run.clear();
        self.run_len = 0;
    }

    /// Ends the last run and returns the strings and whether the cap dropped some.
    pub(in crate::window_view::viewer) fn finish(mut self) -> (Vec<FoundString>, bool) {
        self.end_run();
        (self.found, self.capped)
    }
}

/// Formats bytes as lowercase hex.
fn lower_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Reads `source` from 0 to its length at the start in `chunk`-byte steps, hashing and
/// collecting strings (at most `cap`). Ok(None) when `cancel` was set; Err when a read failed.
pub(in crate::window_view::viewer) fn scan<S: ByteSource + ?Sized>(
    source: &S,
    chunk: usize,
    cap: usize,
    cancel: &AtomicBool,
) -> Result<Option<Scanned>, String> {
    let end = source.len();
    let mut crc = crc32fast::Hasher::new();
    let mut sha = Sha256::new();
    let mut blake = blake3::Hasher::new();
    let mut strings = StringScan::new(cap);
    let mut buf = vec![0u8; chunk.max(1)];
    let mut at = 0u64;
    while at < end {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let want = usize::try_from(end - at).map_or(buf.len(), |left| left.min(buf.len()));
        let read = source.read_at(at, &mut buf[..want])?;
        if read == 0 {
            break;
        }
        let bytes = &buf[..read];
        crc.update(bytes);
        sha.update(bytes);
        blake.update(bytes);
        strings.feed(at, bytes);
        at += read as u64;
    }
    let (strings, capped) = strings.finish();
    Ok(Some(Scanned {
        digests: Digests {
            crc32: format!("{:08x}", crc.finalize()),
            sha256: lower_hex(&sha.finalize()),
            blake3: blake.finalize().to_hex().to_string(),
        },
        strings,
        capped,
    }))
}
