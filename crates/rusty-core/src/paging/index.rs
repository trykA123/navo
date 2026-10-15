use super::{ByteSource, read_full, span};

const SCAN_BUF: usize = 64 * 1024;

pub const MARK_EVERY: u64 = 1024;
pub const MARK_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    offset: u64,
    line: u64,
}

const FIRST: Mark = Mark { offset: 0, line: 0 };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<Mark>,
    points: Vec<Mark>,
    scanned: u64,
    newlines: u64,
    done: bool,
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LineIndex {
    #[must_use]
    pub fn new() -> Self {
        Self {
            starts: vec![FIRST],
            points: vec![FIRST],
            scanned: 0,
            newlines: 0,
            done: false,
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) {
        let base = self.scanned;
        for at in memchr::memchr_iter(b'\n', chunk) {
            let newline = base + at as u64;
            self.mark_within(newline + 1);
            self.newlines += 1;
            let next = newline + 1;
            let last_start = self.starts.last().map_or(0, |mark| mark.offset);
            if self.newlines.is_multiple_of(MARK_EVERY) || next - last_start >= MARK_BYTES {
                let mark = Mark {
                    offset: next,
                    line: self.newlines,
                };
                self.starts.push(mark);
                self.points.push(mark);
            }
        }
        self.scanned += chunk.len() as u64;
        self.mark_within(self.scanned);
    }

    fn mark_within(&mut self, end: u64) {
        while let Some(last) = self.points.last().copied() {
            let offset = last.offset + MARK_BYTES;
            if offset >= end {
                break;
            }
            self.points.push(Mark {
                offset,
                line: self.newlines,
            });
        }
    }

    pub fn finish(&mut self) {
        self.done = true;
    }

    #[must_use]
    pub fn scanned(&self) -> u64 {
        self.scanned
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.done
    }

    #[must_use]
    pub fn newlines(&self) -> u64 {
        self.newlines
    }

    #[must_use]
    pub fn marks(&self) -> usize {
        self.starts.len()
    }

    pub fn line_count<S: ByteSource + ?Sized>(&self, src: &S) -> Result<u64, String> {
        if self.scanned == 0 {
            return Ok(0);
        }
        let mut last = [0u8; 1];
        read_full(src, self.scanned - 1, &mut last)?;
        Ok(self.newlines + u64::from(last[0] != b'\n'))
    }

    pub fn line_number<S: ByteSource + ?Sized>(
        &self,
        src: &S,
        offset: u64,
    ) -> Result<Option<u64>, String> {
        let offset = if self.done {
            offset.min(self.scanned.saturating_sub(1))
        } else if offset >= self.scanned {
            return Ok(None);
        } else {
            offset
        };
        let at = self.points.partition_point(|mark| mark.offset <= offset);
        let mark = self
            .points
            .get(at.wrapping_sub(1))
            .copied()
            .unwrap_or(FIRST);
        let counted = count_newlines(src, mark.offset, offset)?;
        Ok(Some(mark.line + counted + 1))
    }

    pub fn line_start<S: ByteSource + ?Sized>(
        &self,
        src: &S,
        line: u64,
    ) -> Result<Option<u64>, String> {
        let wanted = line.saturating_sub(1);
        let at = self.starts.partition_point(|mark| mark.line <= wanted);
        let from = self
            .starts
            .get(at.wrapping_sub(1))
            .copied()
            .unwrap_or(FIRST);
        let skip = wanted - from.line;
        let Some(start) = skip_newlines(src, from.offset, skip, self.scanned)? else {
            return Ok(None);
        };
        if wanted > 0 && self.done && start >= self.scanned {
            return Ok(None);
        }
        Ok(Some(start))
    }
}

fn count_newlines<S: ByteSource + ?Sized>(src: &S, from: u64, to: u64) -> Result<u64, String> {
    let mut buf = vec![0u8; SCAN_BUF];
    let mut at = from;
    let mut count = 0;
    while at < to {
        let want = span(at, to).min(SCAN_BUF);
        let read = read_full(src, at, &mut buf[..want])?;
        if read == 0 {
            break;
        }
        count += memchr::memchr_iter(b'\n', &buf[..read]).count() as u64;
        at += read as u64;
    }
    Ok(count)
}

fn skip_newlines<S: ByteSource + ?Sized>(
    src: &S,
    from: u64,
    skip: u64,
    limit: u64,
) -> Result<Option<u64>, String> {
    if skip == 0 {
        return Ok(Some(from));
    }
    let mut buf = vec![0u8; SCAN_BUF];
    let mut at = from;
    let mut left = skip;
    while at < limit {
        let want = span(at, limit).min(SCAN_BUF);
        let read = read_full(src, at, &mut buf[..want])?;
        if read == 0 {
            break;
        }
        for found in memchr::memchr_iter(b'\n', &buf[..read]) {
            left -= 1;
            if left == 0 {
                return Ok(Some(at + found as u64 + 1));
            }
        }
        at += read as u64;
    }
    Ok(None)
}
