use super::{ByteSource, read_full, span};

const SCAN_BUF: usize = 64 * 1024;

pub const MARK_EVERY: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    marks: Vec<u64>,
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
            marks: vec![0],
            scanned: 0,
            newlines: 0,
            done: false,
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) {
        for at in memchr::memchr_iter(b'\n', chunk) {
            self.newlines += 1;
            if self.newlines.is_multiple_of(MARK_EVERY) {
                self.marks.push(self.scanned + at as u64 + 1);
            }
        }
        self.scanned += chunk.len() as u64;
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
        self.marks.len()
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
        let mark = self.marks.partition_point(|start| *start <= offset) - 1;
        let from = self.marks[mark];
        let counted = count_newlines(src, from, offset)?;
        Ok(Some(mark as u64 * MARK_EVERY + counted + 1))
    }

    pub fn line_start<S: ByteSource + ?Sized>(
        &self,
        src: &S,
        line: u64,
    ) -> Result<Option<u64>, String> {
        let wanted = line.saturating_sub(1);
        let mark = usize::try_from(wanted / MARK_EVERY).unwrap_or(usize::MAX);
        let Some(&from) = self.marks.get(mark) else {
            return Ok(None);
        };
        let skip = wanted % MARK_EVERY;
        let Some(start) = skip_newlines(src, from, skip, self.scanned)? else {
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
