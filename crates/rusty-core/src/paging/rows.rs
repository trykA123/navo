use super::{ByteSource, read_full, span};

pub const MAX_ROW: u64 = 4096;
pub const LOOK_BACK: u64 = 64 * 1024;

const ROW_BUF: usize = 4098;
const CHAR_STEPS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub start: u64,
    pub end: u64,
    pub next: u64,
}

pub fn row_at<S: ByteSource + ?Sized>(src: &S, start: u64) -> Result<Row, String> {
    let len = src.len();
    if start >= len {
        return Ok(Row {
            start: len,
            end: len,
            next: len,
        });
    }
    let mut buf = [0u8; ROW_BUF];
    let count = read_full(src, start, &mut buf)?;
    let read = &buf[..count];
    if let Some(at) = newline_in(read) {
        let shown = if at > 0 && read[at - 1] == b'\r' {
            at - 1
        } else {
            at
        };
        return Ok(Row {
            start,
            end: start + shown as u64,
            next: start + at as u64 + 1,
        });
    }
    let cut = if count < ROW_BUF {
        count
    } else {
        char_cut(read)
    };
    let end = start + cut as u64;
    Ok(Row {
        start,
        end,
        next: end,
    })
}

fn newline_in(read: &[u8]) -> Option<usize> {
    let row = span(0, MAX_ROW);
    memchr::memchr(b'\n', &read[..read.len().min(row + 1)])
        .or_else(|| (read.get(row..row + 2) == Some(b"\r\n")).then_some(row + 1))
}

fn char_cut(read: &[u8]) -> usize {
    let mut cut = span(0, MAX_ROW);
    for _ in 0..CHAR_STEPS {
        if read[cut] & 0xC0 != 0x80 {
            break;
        }
        cut -= 1;
    }
    cut
}

pub fn row_start_at<S: ByteSource + ?Sized>(src: &S, offset: u64) -> Result<u64, String> {
    let len = src.len();
    if len == 0 {
        return Ok(0);
    }
    let offset = offset.min(len - 1);
    let mut start = line_anchor(src, offset)?;
    loop {
        let row = row_at(src, start)?;
        if row.next > offset || row.next >= src.len() || row.next <= start {
            return Ok(start);
        }
        start = row.next;
    }
}

fn line_anchor<S: ByteSource + ?Sized>(src: &S, offset: u64) -> Result<u64, String> {
    let low = (offset + 1).saturating_sub(LOOK_BACK);
    let mut buf = vec![0u8; span(low, offset)];
    let count = read_full(src, low, &mut buf)?;
    Ok(match memchr::memrchr(b'\n', &buf[..count]) {
        Some(at) => low + at as u64 + 1,
        None => low,
    })
}

pub fn previous_row<S: ByteSource + ?Sized>(src: &S, start: u64) -> Result<u64, String> {
    if start == 0 {
        return Ok(0);
    }
    row_start_at(src, start - 1)
}

pub fn rows_down<S: ByteSource + ?Sized>(src: &S, start: u64, count: u64) -> Result<u64, String> {
    let len = src.len();
    if start >= len {
        return row_start_at(src, len);
    }
    let mut at = start;
    for _ in 0..count {
        let row = row_at(src, at)?;
        if row.next >= len {
            break;
        }
        at = row.next;
    }
    Ok(at)
}

pub fn rows_up<S: ByteSource + ?Sized>(src: &S, start: u64, count: u64) -> Result<u64, String> {
    let mut at = start;
    for _ in 0..count {
        if at == 0 {
            break;
        }
        at = previous_row(src, at)?;
    }
    Ok(at)
}
