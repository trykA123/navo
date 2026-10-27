use std::borrow::Cow;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use aho_corasick::AhoCorasick;
use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1252};
use memchr::memmem;
use regex::{Regex, RegexBuilder};

use super::FindQuery;

const CHUNK: usize = 1024 * 1024;
const PROBE: usize = 8192;
const HIT_LIMIT: usize = 1000;
const TEXT_LIMIT: usize = 200;

pub struct Compiled {
    kind: Kind,
}

enum Kind {
    Empty,
    Literal(Vec<u8>),
    Fold(AhoCorasick),
    Regex(Regex),
}

impl Compiled {
    pub fn new(query: &FindQuery) -> Result<Self, String> {
        let kind = if query.text.is_empty() {
            Kind::Empty
        } else if query.regex {
            Kind::Regex(
                RegexBuilder::new(&query.text)
                    .case_insensitive(!query.case)
                    .build()
                    .map_err(|error| error.to_string())?,
            )
        } else if query.case {
            Kind::Literal(query.text.as_bytes().to_vec())
        } else {
            Kind::Fold(
                AhoCorasick::new([query.text.to_lowercase()]).map_err(|error| error.to_string())?,
            )
        };
        Ok(Self { kind })
    }

    fn visit(&self, text: &str, mut visit: impl FnMut(usize) -> bool) {
        match &self.kind {
            Kind::Empty => {}
            Kind::Literal(needle) => {
                memmem::find_iter(text.as_bytes(), needle).all(visit);
            }
            Kind::Regex(regex) => {
                regex.find_iter(text).all(|found| visit(found.start()));
            }
            Kind::Fold(matcher) => visit_folded(matcher, text, visit),
        }
    }
}

fn visit_folded(matcher: &AhoCorasick, text: &str, mut visit: impl FnMut(usize) -> bool) {
    let mut folded = String::new();
    let mut offsets = Vec::new();
    for (offset, character) in text.char_indices() {
        for lower in character.to_lowercase() {
            folded.push(lower);
            offsets.resize(folded.len(), offset);
        }
    }
    for found in matcher.find_iter(&folded) {
        if !visit(offsets[found.start()]) {
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
    scanner.push(&head[scanner.bom_len()..]);
    let mut buffer = vec![0; CHUNK];
    while !scanner.cancelled() {
        let count = read_chunk(&mut reader, &mut buffer)?;
        if count == 0 {
            scanner.finish();
            scanner.cancelled();
            break;
        }
        scanner.push(&buffer[..count]);
    }
    Ok(scanner.found)
}

fn read_chunk(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
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
    pending: Vec<u8>,
    line: u64,
    found: FileHits,
}

impl<'a> Scanner<'a> {
    fn new(head: &[u8], compiled: &'a Compiled, cancel: &'a AtomicBool) -> Self {
        let wide = match head.get(..2) {
            Some([0xff, 0xfe]) => Some(UTF_16LE),
            Some([0xfe, 0xff]) => Some(UTF_16BE),
            _ => None,
        };
        Self {
            compiled,
            cancel,
            wide,
            pending: Vec::new(),
            line: 1,
            found: FileHits::default(),
        }
    }

    fn bom_len(&self) -> usize {
        if self.wide.is_some() { 2 } else { 0 }
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
        let decoded = decode_line(bytes, self.wide);
        let text = decoded.strip_suffix('\r').unwrap_or(&decoded);
        let mut previous = 0;
        let mut column = 0;
        self.compiled.visit(text, |start| {
            if self.cancel.load(Ordering::Relaxed) {
                return false;
            }
            self.found.total = self.found.total.saturating_add(1);
            if self.found.hits.len() < HIT_LIMIT {
                column += text[previous..start].chars().count();
                previous = start;
                self.found.hits.push(LineHit {
                    line: self.line,
                    column: u32::try_from(column + 1).unwrap_or(u32::MAX),
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

fn decode_line<'a>(bytes: &'a [u8], wide: Option<&'static encoding_rs::Encoding>) -> Cow<'a, str> {
    if let Some(encoding) = wide {
        return encoding.decode_without_bom_handling(bytes).0;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => WINDOWS_1252.decode_without_bom_handling(bytes).0,
    }
}

fn snippet(text: &str, column: usize) -> String {
    let length = text.chars().count();
    let start = column
        .saturating_sub(TEXT_LIMIT / 2)
        .min(length.saturating_sub(TEXT_LIMIT));
    text.chars().skip(start).take(TEXT_LIMIT).collect()
}
