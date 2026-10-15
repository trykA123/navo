use std::sync::atomic::{AtomicBool, Ordering};

use aho_corasick::AhoCorasick;
use memchr::memmem;

use super::{ByteSource, read_full, span};

pub const SEARCH_CHUNK: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Needle {
    Text { text: String, match_case: bool },
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub needle: Needle,
    pub from: u64,
    pub backward: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    At(u64),
    NotFound,
    Cancelled,
}

pub fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for word in input.split_whitespace() {
        if !word.len().is_multiple_of(2) || !word.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("Not hex bytes: {word}"));
        }
        for pair in word.as_bytes().chunks(2) {
            let digits = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
            bytes.push(u8::from_str_radix(digits, 16).map_err(|error| error.to_string())?);
        }
    }
    if bytes.is_empty() {
        return Err("Type hex bytes, like 4D 5A.".to_owned());
    }
    Ok(bytes)
}

enum Matcher {
    Exact(Vec<u8>),
    Fold(AhoCorasick),
}

impl Matcher {
    fn new(needle: &Needle) -> Result<Option<(Self, usize)>, String> {
        let (bytes, fold) = match needle {
            Needle::Text { text, match_case } => (text.as_bytes(), !match_case),
            Needle::Bytes(bytes) => (bytes.as_slice(), false),
        };
        if bytes.is_empty() {
            return Ok(None);
        }
        let matcher = if fold {
            let built = AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build([bytes])
                .map_err(|error| error.to_string())?;
            Self::Fold(built)
        } else {
            Self::Exact(bytes.to_vec())
        };
        Ok(Some((matcher, bytes.len())))
    }

    fn first(&self, hay: &[u8]) -> Option<usize> {
        match self {
            Self::Exact(needle) => memmem::find(hay, needle),
            Self::Fold(automaton) => automaton.find(hay).map(|found| found.start()),
        }
    }

    fn last(&self, hay: &[u8]) -> Option<usize> {
        match self {
            Self::Exact(needle) => memmem::rfind(hay, needle),
            Self::Fold(automaton) => automaton
                .find_overlapping_iter(hay)
                .map(|found| found.start())
                .last(),
        }
    }
}

pub fn find<S: ByteSource + ?Sized>(
    src: &S,
    search: &Search,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<Found, String> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(Found::Cancelled);
    }
    let Some((matcher, width)) = Matcher::new(&search.needle)? else {
        return Ok(Found::NotFound);
    };
    let scan = Scan {
        matcher: &matcher,
        extra: width as u64 - 1,
        from: search.from.min(src.len()),
        cancel,
    };
    if search.backward {
        scan.backward(src, progress)
    } else {
        scan.forward(src, progress)
    }
}

struct Scan<'a> {
    matcher: &'a Matcher,
    extra: u64,
    from: u64,
    cancel: &'a AtomicBool,
}

impl Scan<'_> {
    fn forward<S: ByteSource + ?Sized>(
        &self,
        src: &S,
        progress: &mut dyn FnMut(u64),
    ) -> Result<Found, String> {
        let len = src.len();
        let mut buf = Vec::new();
        let mut at = self.from;
        while at < len {
            if self.cancel.load(Ordering::Relaxed) {
                return Ok(Found::Cancelled);
            }
            let stop = (at + SEARCH_CHUNK as u64 + self.extra).min(len);
            let hay = fill(src, at, stop, &mut buf)?;
            if let Some(found) = self.matcher.first(hay) {
                return Ok(Found::At(at + found as u64));
            }
            at = (at + SEARCH_CHUNK as u64).min(len);
            progress(at - self.from);
        }
        Ok(Found::NotFound)
    }

    fn backward<S: ByteSource + ?Sized>(
        &self,
        src: &S,
        progress: &mut dyn FnMut(u64),
    ) -> Result<Found, String> {
        let len = src.len();
        let mut buf = Vec::new();
        let mut end = self.from;
        while end > 0 {
            if self.cancel.load(Ordering::Relaxed) {
                return Ok(Found::Cancelled);
            }
            let low = end.saturating_sub(SEARCH_CHUNK as u64);
            let stop = (end + self.extra).min(len);
            let hay = fill(src, low, stop, &mut buf)?;
            let before = span(low, end);
            let found = self.matcher.last(hay).filter(|start| *start < before);
            if let Some(found) = found {
                return Ok(Found::At(low + found as u64));
            }
            end = low;
            progress(self.from - end);
        }
        Ok(Found::NotFound)
    }
}

fn fill<'b, S: ByteSource + ?Sized>(
    src: &S,
    from: u64,
    to: u64,
    buf: &'b mut Vec<u8>,
) -> Result<&'b [u8], String> {
    buf.resize(span(from, to), 0);
    let count = read_full(src, from, buf)?;
    Ok(&buf[..count])
}
