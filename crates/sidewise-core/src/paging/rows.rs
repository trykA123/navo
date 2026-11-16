use super::{ByteSource, TextLayout, read_full, span};

pub const MAX_ROW: u64 = 4096;
pub const LOOK_BACK: u64 = 64 * 1024;

const ROW_BUF: usize = 4100;
const LEAD_BUF: usize = 3;

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
    let layout = src.layout();
    let unit = layout.unit();
    let mut buf = [0u8; ROW_BUF];
    let count = read_full(src, start, &mut buf)?;
    let read = &buf[..count];
    if let Some(at) = newline_in(&layout, read) {
        let shown = if at >= unit && layout.is_cr(&read[at - unit..at]) {
            at - unit
        } else {
            at
        };
        return Ok(Row {
            start,
            end: start + shown as u64,
            next: start + (at + unit) as u64,
        });
    }
    let cut = if count <= span(0, MAX_ROW) {
        count
    } else {
        layout.cut_back(read, span(0, MAX_ROW))
    };
    let end = start + cut as u64;
    Ok(Row {
        start,
        end,
        next: end,
    })
}

fn newline_in(layout: &TextLayout, read: &[u8]) -> Option<usize> {
    let row = span(0, MAX_ROW);
    let unit = layout.unit();
    layout
        .first_newline(&read[..read.len().min(row + unit)])
        .or_else(|| {
            let cr = read.get(row..row + unit)?;
            let lf = read.get(row + unit..row + 2 * unit)?;
            (layout.is_cr(cr) && layout.is_lf(lf)).then_some(row + unit)
        })
}

pub fn row_start_at<S: ByteSource + ?Sized>(src: &S, offset: u64) -> Result<u64, String> {
    let len = src.len();
    if len == 0 {
        return Ok(0);
    }
    if offset >= len {
        return previous_row(src, len);
    }
    let offset = src.layout().align(offset);
    let (anchor, _) = line_anchor(src, offset)?;
    step_to(src, anchor, offset)
}

fn step_to<S: ByteSource + ?Sized>(src: &S, anchor: u64, offset: u64) -> Result<u64, String> {
    let mut start = anchor;
    loop {
        let row = row_at(src, start)?;
        if row.next > offset || row.next >= src.len() || row.next <= start {
            return Ok(start);
        }
        start = row.next;
    }
}

fn line_anchor<S: ByteSource + ?Sized>(src: &S, offset: u64) -> Result<(u64, bool), String> {
    let layout = src.layout();
    let offset = layout.align(offset);
    let low = layout.align((offset + 1).saturating_sub(LOOK_BACK));
    let mut buf = vec![0u8; span(low, offset)];
    let count = read_full(src, low, &mut buf)?;
    Ok(match layout.last_newline(&buf[..count]) {
        Some(at) => (low + (at + layout.unit()) as u64, true),
        None => (low, low == 0),
    })
}

fn row_ending_at<S: ByteSource + ?Sized>(src: &S, start: u64) -> Result<Option<u64>, String> {
    let mut candidate = start.saturating_sub(MAX_ROW);
    let mut lead = [0u8; LEAD_BUF];
    let count = read_full(src, candidate, &mut lead)?;
    candidate += src.layout().skip_continuation(&lead[..count]) as u64;
    let fits = candidate < start && row_at(src, candidate)?.next == start;
    Ok(fits.then_some(candidate))
}

pub fn previous_row<S: ByteSource + ?Sized>(src: &S, start: u64) -> Result<u64, String> {
    if start == 0 {
        return Ok(0);
    }
    let len = src.len();
    let layout = src.layout();
    let start = if start < len {
        layout.align(start)
    } else {
        len
    };
    let before = start.saturating_sub(layout.unit() as u64);
    let (anchor, exact) = line_anchor(src, before)?;
    if !exact && let Some(found) = row_ending_at(src, start)? {
        return Ok(found);
    }
    step_to(src, anchor, before)
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
