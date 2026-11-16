use navo_core::encoding::Encoding;
use navo_core::paging::{ByteSource, LineIndex, TextLayout, read_full};

use crate::model::{CancelToken, Reason};

use super::normalise::Options;

mod normaliser;
mod sample;

use normaliser::Normaliser;

pub use sample::Sample;

pub const BLOCK: usize = 1024 * 1024;
pub const BUCKETS: u32 = 32;
pub const PROGRESS_EVERY: u64 = 8 * 1024 * 1024;
const DECODE_CHUNK: usize = 64 * 1024;
const MIN_ANCHOR_BYTES: usize = 8;
const FIRST_CANDIDATES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub hash: u64,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub index: LineIndex,
    pub candidates: Vec<Candidate>,
    pub lines: u64,
}

#[expect(
    clippy::too_many_arguments,
    reason = "signature fixed by the L1 contract"
)]
pub fn scan<S: ByteSource + ?Sized>(
    source: &S,
    layout: TextLayout,
    options: &Options,
    sample: impl Into<Sample>,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(u64),
) -> Result<ScanResult, Reason> {
    let len = source.len();
    if cancel.is_cancelled() {
        return Err(Reason::Stopped);
    }
    let mut scanner = Scanner::new(layout, options, sample.into());
    let mut buffer = vec![0_u8; BLOCK];
    let mut at = 0_u64;
    let mut next = PROGRESS_EVERY;
    let mut reported = 0_u64;
    let mut first = true;
    while at < len {
        if cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        let want = usize::try_from((len - at).min(BLOCK as u64)).map_err(|_| Reason::ReadError)?;
        let read = read_full(source, at, &mut buffer[..want]).map_err(|_| Reason::ReadError)?;
        if read < want {
            return Err(Reason::ChangedOnDisk);
        }
        scanner.feed(&buffer[..read], first)?;
        at += u64::try_from(read).map_err(|_| Reason::ReadError)?;
        first = false;
        if at >= next {
            progress(at);
            if cancel.is_cancelled() {
                return Err(Reason::Stopped);
            }
            reported = at;
            while next <= at {
                next += PROGRESS_EVERY;
            }
        }
    }
    if cancel.is_cancelled() {
        return Err(Reason::Stopped);
    }
    if at > reported {
        progress(at);
        if cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
    }
    finish_checked(scanner, source, len, cancel)
}

fn finish_checked<S: ByteSource + ?Sized>(
    scanner: Scanner<'_>,
    source: &S,
    len: u64,
    cancel: &CancelToken,
) -> Result<ScanResult, Reason> {
    let result = scanner.finish()?;
    if cancel.is_cancelled() {
        return Err(Reason::Stopped);
    }
    source.probe_end().map_err(|_| Reason::ReadError)?;
    let changed = source.take_changed();
    let rewritten = source.take_rewritten();
    if source.len() != len || changed || rewritten {
        return Err(Reason::ChangedOnDisk);
    }
    if cancel.is_cancelled() {
        return Err(Reason::Stopped);
    }
    Ok(result)
}

struct Scanner<'a> {
    layout: TextLayout,
    options: &'a Options,
    rate: u64,
    cap: usize,
    index: LineIndex,
    candidates: Vec<Candidate>,
    lines: u64,
    line_open: bool,
    decoder: Decoder,
    normaliser: Normaliser,
}

impl<'a> Scanner<'a> {
    fn new(layout: TextLayout, options: &'a Options, sample: Sample) -> Self {
        Self {
            layout,
            options,
            rate: u64::from(sample.rate.max(1)),
            cap: sample.cap,
            index: LineIndex::with_layout(layout),
            candidates: Vec::new(),
            lines: 0,
            line_open: false,
            decoder: Decoder::new(layout.text.encoding),
            normaliser: Normaliser::new(options, layout.text.bom),
        }
    }

    fn feed(&mut self, chunk: &[u8], first: bool) -> Result<(), Reason> {
        self.index.feed(chunk);
        let skip = if first {
            self.layout.bom_len().min(chunk.len())
        } else {
            0
        };
        let mut start = skip;
        for newline in self.layout.newlines(chunk) {
            if newline < start {
                continue;
            }
            self.bytes(&chunk[start..newline])?;
            self.end_line(true)?;
            start = newline + self.layout.unit();
        }
        if let Some(rest) = chunk.get(start..) {
            self.bytes(rest)?;
        }
        Ok(())
    }

    fn bytes(&mut self, bytes: &[u8]) -> Result<(), Reason> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.line_open = true;
        let Self {
            decoder,
            normaliser,
            ..
        } = self;
        for part in bytes.chunks(DECODE_CHUNK) {
            decoder.feed(part, false, &mut |character| normaliser.push(character))?;
        }
        Ok(())
    }

    fn end_line(&mut self, terminated: bool) -> Result<(), Reason> {
        {
            let Self {
                decoder,
                normaliser,
                ..
            } = self;
            decoder.feed(&[], true, &mut |character| normaliser.push(character))?;
        }
        let next = Normaliser::new(self.options, false);
        let normaliser = std::mem::replace(&mut self.normaliser, next);
        let (hash, nonblank) = normaliser.finish(terminated);
        if nonblank >= MIN_ANCHOR_BYTES && hash.is_multiple_of(self.rate) {
            let line = u32::try_from(self.lines).map_err(|_| Reason::Unsupported)?;
            self.keep(Candidate { hash, line });
        }
        self.lines += 1;
        self.line_open = false;
        Ok(())
    }

    fn keep(&mut self, candidate: Candidate) {
        while self.candidates.len() >= self.cap && self.rate <= u64::MAX / 2 {
            self.rate *= 2;
            let rate = self.rate;
            self.candidates
                .retain(|kept| kept.hash.is_multiple_of(rate));
        }
        if self.candidates.len() >= self.cap {
            self.candidates.clear();
            self.cap = 0;
            return;
        }
        if !candidate.hash.is_multiple_of(self.rate) {
            return;
        }
        if self.candidates.len() == self.candidates.capacity() {
            let room = self.cap - self.candidates.len();
            let grow = self.candidates.capacity().max(FIRST_CANDIDATES).min(room);
            self.candidates.reserve_exact(grow);
        }
        self.candidates.push(candidate);
    }

    fn finish(mut self) -> Result<ScanResult, Reason> {
        if self.line_open {
            self.end_line(false)?;
        }
        self.index.finish();
        Ok(ScanResult {
            index: self.index,
            candidates: self.candidates,
            lines: self.lines,
        })
    }

    #[cfg(test)]
    fn retained_capacity(&self) -> usize {
        self.decoder.capacity()
    }
}

struct Decoder {
    feed: Box<Feed>,
    scratch: String,
}

type Feed = dyn FnMut(&[u8], &mut String, bool, &mut dyn FnMut(char)) -> Result<(), Reason>;

impl Decoder {
    fn new(encoding: &'static Encoding) -> Self {
        let mut decoder = encoding.new_decoder_without_bom_handling();
        let feed = move |bytes: &[u8],
                         scratch: &mut String,
                         last: bool,
                         out: &mut dyn FnMut(char)|
              -> Result<(), Reason> {
            let needed = decoder
                .max_utf8_buffer_length(bytes.len())
                .ok_or(Reason::ReadError)?;
            scratch.clear();
            if scratch.capacity() < needed {
                scratch.reserve(needed - scratch.capacity());
            }
            let (_, read, _) = decoder.decode_to_string(bytes, scratch, last);
            if read != bytes.len() {
                return Err(Reason::ReadError);
            }
            for character in scratch.chars() {
                out(character);
            }
            if last {
                decoder = encoding.new_decoder_without_bom_handling();
            }
            Ok(())
        };
        Self {
            feed: Box::new(feed),
            scratch: String::new(),
        }
    }

    fn feed(&mut self, bytes: &[u8], last: bool, out: &mut dyn FnMut(char)) -> Result<(), Reason> {
        let Self { feed, scratch } = self;
        (feed)(bytes, scratch, last, out)
    }

    #[cfg(test)]
    fn capacity(&self) -> usize {
        self.scratch.capacity()
    }
}

#[cfg(test)]
mod tests;
