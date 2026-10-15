use rusty_core::paging::{
    ByteSource, HEX_WIDTH, HexRow, LineIndex, hex_row, hex_row_start, hex_rows, read_full, row_at,
    row_start_at, rows_down, rows_up,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagedKind {
    Hex,
    Text,
}

pub(super) struct HexLine {
    pub start: u64,
    pub row: HexRow,
}

pub(super) struct TextLine {
    pub start: u64,
    pub end: u64,
    pub next: u64,
    pub text: String,
    pub line: Option<u64>,
}

pub(super) struct Screen<L> {
    pub lines: Vec<L>,
}

fn byte_len(start: u64, end: u64) -> usize {
    usize::try_from(end.saturating_sub(start)).unwrap_or(usize::MAX)
}

pub(super) fn hex_lines<S: ByteSource + ?Sized>(
    src: &S,
    top: u64,
    count: usize,
) -> Result<Screen<HexLine>, String> {
    let wide = src.len() > u64::from(u32::MAX);
    let start = hex_row_start(top);
    let mut buf = vec![0u8; count * byte_len(0, HEX_WIDTH)];
    let read = read_full(src, start, &mut buf)?;
    let lines = buf[..read]
        .chunks(byte_len(0, HEX_WIDTH))
        .zip(0u64..)
        .map(|(bytes, row)| {
            let at = start + row * HEX_WIDTH;
            HexLine {
                start: at,
                row: hex_row(at, bytes, wide),
            }
        })
        .collect();
    Ok(Screen { lines })
}

fn starts_line<S: ByteSource + ?Sized>(src: &S, offset: u64) -> Result<bool, String> {
    if offset == 0 {
        return Ok(true);
    }
    let mut byte = [0u8; 1];
    let read = read_full(src, offset - 1, &mut byte)?;
    Ok(read == 1 && byte[0] == b'\n')
}

fn counted(index: &LineIndex, offset: u64) -> bool {
    index.is_done() || offset < index.scanned()
}

pub(super) fn text_lines<S: ByteSource + ?Sized>(
    src: &S,
    top: u64,
    count: usize,
    index: Option<&LineIndex>,
) -> Result<Screen<TextLine>, String> {
    let len = src.len();
    let mut lines = Vec::with_capacity(count);
    let mut number = match index {
        Some(index) => index.line_number(src, top)?,
        None => None,
    };
    let mut is_line_start = starts_line(src, top)?;
    let mut at = top;
    while lines.len() < count && at < len {
        let row = row_at(src, at)?;
        let mut bytes = vec![0u8; byte_len(row.start, row.end)];
        let read = read_full(src, row.start, &mut bytes)?;
        let line = number
            .filter(|_| is_line_start)
            .filter(|_| index.is_some_and(|index| counted(index, row.start)));
        lines.push(TextLine {
            start: row.start,
            end: row.end,
            next: row.next,
            text: String::from_utf8_lossy(&bytes[..read]).into_owned(),
            line,
        });
        is_line_start = row.next > row.end;
        if is_line_start {
            number = number.map(|number| number + 1);
        }
        at = row.next;
    }
    Ok(Screen { lines })
}

pub(super) fn row_start<S: ByteSource + ?Sized>(
    src: &S,
    kind: PagedKind,
    offset: u64,
) -> Result<u64, String> {
    match kind {
        PagedKind::Hex => Ok(hex_row_start(offset.min(src.len().saturating_sub(1)))),
        PagedKind::Text => row_start_at(src, offset),
    }
}

pub(super) fn end_top<S: ByteSource + ?Sized>(
    src: &S,
    kind: PagedKind,
    rows: u64,
) -> Result<u64, String> {
    match kind {
        PagedKind::Hex => Ok(hex_rows(src.len()).saturating_sub(rows) * HEX_WIDTH),
        PagedKind::Text => {
            let last = row_start_at(src, src.len())?;
            rows_up(src, last, rows.saturating_sub(1))
        }
    }
}

pub(super) fn step<S: ByteSource + ?Sized>(
    src: &S,
    kind: PagedKind,
    top: u64,
    rows: i64,
) -> Result<u64, String> {
    let count = rows.unsigned_abs();
    match (kind, rows < 0) {
        (PagedKind::Hex, true) => Ok(top.saturating_sub(count * HEX_WIDTH)),
        (PagedKind::Hex, false) => Ok(top.saturating_add(count * HEX_WIDTH)),
        (PagedKind::Text, true) => rows_up(src, top, count),
        (PagedKind::Text, false) => rows_down(src, top, count),
    }
}
