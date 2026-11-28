use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use aho_corasick::AhoCorasick;
use encoding_rs::UTF_16BE;
use memchr::memmem;
use regex::{Regex, RegexBuilder};

use super::{FindQuery, NameFilter};
use decode::{Decoders, bom_len, snippet};

mod bytes;
mod decode;

#[cfg(test)]
pub(in crate::find) use decode::{chunk_growths, reset_chunk_growths};

const CHUNK: usize = 1024 * 1024;
const PROBE: usize = 8192;
const HIT_LIMIT: usize = 1000;

thread_local! {
    static CHUNK_BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

pub struct Compiled {
    kind: Kind,
    name: Option<NameFilter>,
    decoders: Decoders,
}

enum Kind {
    Empty,
    Literal(Vec<u8>),
    Fold(AhoCorasick),
    Regex(Regex),
    Bytes(Vec<u8>),
}

impl Compiled {
    /// Compiles a find query into a matcher, rejecting invalid token combinations.
    pub fn new(query: &FindQuery) -> Result<Self, String> {
        validate_query(query)?;
        let kind = compile_kind(query)?;
        let name = query.name.as_deref().map(NameFilter::new).transpose()?;
        Ok(Self {
            kind,
            name,
            decoders: Decoders::new(&query.encodings),
        })
    }

    /// The `name:` filter, when the query has one.
    #[must_use]
    pub fn name_filter(&self) -> Option<&NameFilter> {
        self.name.as_ref()
    }

    /// Whether matching this query requires reading file contents.
    #[must_use]
    pub fn needs_scan(&self) -> bool {
        !matches!(self.kind, Kind::Empty)
    }

    fn visit(&self, text: &str, fold: &mut FoldScratch, mut visit: impl FnMut(usize) -> bool) {
        match &self.kind {
            Kind::Empty | Kind::Bytes(_) => {}
            Kind::Literal(needle) => {
                memmem::find_iter(text.as_bytes(), needle).all(visit);
            }
            Kind::Regex(regex) => {
                regex.find_iter(text).all(|found| visit(found.start()));
            }
            Kind::Fold(matcher) => visit_folded(matcher, text, fold, visit),
        }
    }
}

fn validate_query(query: &FindQuery) -> Result<(), String> {
    if query.hex.is_some() {
        if query.regex || query.word {
            return Err("hex: cannot be combined with re: or word:".to_owned());
        }
        if !query.text.is_empty() {
            return Err("hex: cannot be combined with text".to_owned());
        }
    }
    if query.invert && query.text.is_empty() {
        return Err("not: needs text to look for".to_owned());
    }
    Ok(())
}

fn compile_kind(query: &FindQuery) -> Result<Kind, String> {
    if let Some(hex) = &query.hex {
        return Ok(if hex.is_empty() {
            Kind::Empty
        } else {
            Kind::Bytes(hex.clone())
        });
    }
    if query.text.is_empty() {
        return Ok(Kind::Empty);
    }
    if query.word || query.regex {
        let pattern = match (query.word, query.regex) {
            (false, _) => Cow::Borrowed(query.text.as_str()),
            (true, true) => Cow::Owned(format!(r"\b(?:{})\b", query.text)),
            (true, false) => Cow::Owned(format!(r"\b(?:{})\b", regex::escape(&query.text))),
        };
        return RegexBuilder::new(&pattern)
            .case_insensitive(!query.case)
            .build()
            .map(Kind::Regex)
            .map_err(|error| error.to_string());
    }
    if query.case {
        return Ok(Kind::Literal(query.text.as_bytes().to_vec()));
    }
    AhoCorasick::new([query.text.to_lowercase()])
        .map(Kind::Fold)
        .map_err(|error| error.to_string())
}

/// Reusable folding buffers, cleared per line so they never reallocate per line.
#[derive(Default)]
struct FoldScratch {
    folded: String,
    offsets: Vec<usize>,
}

fn visit_folded(
    matcher: &AhoCorasick,
    text: &str,
    scratch: &mut FoldScratch,
    mut visit: impl FnMut(usize) -> bool,
) {
    scratch.folded.clear();
    scratch.offsets.clear();
    for (offset, character) in text.char_indices() {
        for lower in character.to_lowercase() {
            scratch.folded.push(lower);
            scratch.offsets.resize(scratch.folded.len(), offset);
        }
    }
    for found in matcher.find_iter(&scratch.folded) {
        if !visit(scratch.offsets[found.start()]) {
            break;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineHit {
    pub line: u64,
    pub column: u32,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    Binary,
    Cancelled,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileHits {
    pub hits: Vec<LineHit>,
    pub total: u32,
    pub skipped: Option<Skip>,
}

pub fn scan(
    mut reader: impl Read,
    compiled: &Compiled,
    cancel: &AtomicBool,
) -> io::Result<FileHits> {
    if let Kind::Bytes(needle) = &compiled.kind {
        return bytes::scan_bytes(reader, needle, cancel);
    }
    let mut head = vec![0; PROBE];
    let mut filled = 0;
    while filled < head.len() && !cancel.load(Ordering::Relaxed) {
        let count = read_chunk(&mut reader, &mut head[filled..])?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    head.truncate(filled);
    let mut scanner = Scanner::new(&head, compiled, cancel);
    if scanner.cancelled() || scanner.is_binary(&head) {
        return Ok(scanner.found);
    }
    let skip = bom_len(&head);
    scanner.push(&head[skip..]);
    CHUNK_BUFFER.with(|cell| {
        let mut fallback = Vec::new();
        let mut shared = cell.try_borrow_mut().ok();
        let buffer: &mut Vec<u8> = match shared.as_mut() {
            Some(guard) => {
                prepare_chunk(guard);
                guard
            }
            None => {
                fallback.resize(CHUNK, 0);
                &mut fallback
            }
        };
        while !scanner.cancelled() {
            let count = read_chunk(&mut reader, buffer)?;
            if count == 0 {
                scanner.finish();
                scanner.cancelled();
                break;
            }
            scanner.push(&buffer[..count]);
        }
        Ok::<(), io::Error>(())
    })?;
    Ok(scanner.found)
}

/// Grows the reused chunk buffer to `CHUNK`, counting capacity growths in tests.
fn prepare_chunk(buffer: &mut Vec<u8>) {
    if buffer.capacity() < CHUNK {
        #[cfg(test)]
        decode::note_chunk_growth();
    }
    if buffer.len() < CHUNK {
        buffer.resize(CHUNK, 0);
    }
}

pub(super) fn read_chunk(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    loop {
        match reader.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

struct Scanner<'a> {
    compiled: &'a Compiled,
    cancel: &'a AtomicBool,
    wide: Option<&'static encoding_rs::Encoding>,
    decoders: Decoders,
    fold: FoldScratch,
    pending: Vec<u8>,
    line: u64,
    found: FileHits,
}

impl<'a> Scanner<'a> {
    fn new(head: &[u8], compiled: &'a Compiled, cancel: &'a AtomicBool) -> Self {
        let wide = compiled.decoders.wide_from_head(head);
        Self {
            compiled,
            cancel,
            wide,
            decoders: compiled.decoders,
            fold: FoldScratch::default(),
            pending: Vec::new(),
            line: 1,
            found: FileHits::default(),
        }
    }

    fn cancelled(&mut self) -> bool {
        if !self.cancel.load(Ordering::Relaxed) {
            return false;
        }
        self.found.skipped = Some(Skip::Cancelled);
        true
    }

    fn is_binary(&mut self, head: &[u8]) -> bool {
        if self.wide.is_none() && head.contains(&0) {
            self.found.skipped = Some(Skip::Binary);
            return true;
        }
        false
    }

    fn push(&mut self, bytes: &[u8]) {
        let unit = if self.wide.is_some() { 2 } else { 1 };
        let previous = self.pending.len() / unit * unit;
        self.pending.extend_from_slice(bytes);
        let pending = std::mem::take(&mut self.pending);
        let mut start = 0;
        for at in (previous..pending.len() / unit * unit).step_by(unit) {
            let newline = match self.wide {
                Some(encoding) if encoding == UTF_16BE => pending[at..at + 2] == [0, 10],
                Some(_) => pending[at..at + 2] == [10, 0],
                None => pending[at] == b'\n',
            };
            if !newline {
                continue;
            }
            self.record_line(&pending[start..at]);
            self.line = self.line.saturating_add(1);
            start = at + unit;
            if self.cancelled() {
                break;
            }
        }
        self.pending = pending;
        self.pending.drain(..start);
    }

    fn record_line(&mut self, bytes: &[u8]) {
        let mut seen = Vec::new();
        self.decoders.decode(bytes, self.wide, |decoded| {
            self.visit_text(decoded.strip_suffix('\r').unwrap_or(decoded), &mut seen);
        });
    }

    fn visit_text(&mut self, text: &str, seen: &mut Vec<u32>) {
        let mut previous = 0;
        let mut column = 0;
        self.compiled.visit(text, &mut self.fold, |start| {
            if self.cancel.load(Ordering::Relaxed) {
                return false;
            }
            column += text[previous..start].chars().count();
            previous = start;
            let at = u32::try_from(column + 1).unwrap_or(u32::MAX);
            if seen.contains(&at) {
                return true;
            }
            seen.push(at);
            self.found.total = self.found.total.saturating_add(1);
            if self.found.hits.len() < HIT_LIMIT {
                self.found.hits.push(LineHit {
                    line: self.line,
                    column: at,
                    text: snippet(text, column),
                });
            }
            true
        });
    }

    fn finish(&mut self) {
        if !self.pending.is_empty() && !self.cancelled() {
            let pending = std::mem::take(&mut self.pending);
            self.record_line(&pending);
        }
    }
}
