//! Display-line reads for the large diff view (V35), on their own worker.

use std::ops::Range;
use std::sync::Arc;

use navo_compare::model::{CancelToken, Reason};
use navo_compare::text::large::LargeSide;
use navo_core::paging::{ByteSource, Laid, LineIndex, TextLayout, read_full};

const SCAN_CHUNK: usize = 256 * 1024;
const READ_CHUNK: usize = 256 * 1024;
const MAX_ROWS: u64 = 2_000;
/// Each side gets half of the 4 MiB window budget, so one side never starves the other.
const SIDE_LIMIT: usize = 2 * 1024 * 1024;
/// Bytes read past the limit, so a cut can see the rest of the character it lands in.
const LOOK_AHEAD: usize = 4;

/// The lines read for one window; `rows` is what both sides returned, maybe less than asked.
/// A side past its end holds fewer lines. A `*_cut` side's last line is longer than the side
/// limit and ends early. `totals` holds both sides' line counts.
pub(crate) struct Window {
    pub(crate) rows: Range<u64>,
    pub(crate) left: Vec<String>,
    pub(crate) right: Vec<String>,
    pub(crate) left_cut: bool,
    pub(crate) right_cut: bool,
    /// The file byte offset of each line in `left` and `right`.
    pub(crate) left_starts: Vec<u64>,
    pub(crate) right_starts: Vec<u64>,
    pub(crate) totals: (u64, u64),
}

/// One side's lines, whether the byte budget cut them short, and whether that cut fell
/// inside the only line (`partial`).
#[derive(Default)]
struct SideLines {
    lines: Vec<String>,
    /// The file byte offset each of `lines` starts at.
    starts: Vec<u64>,
    cut: bool,
    partial: bool,
}

/// Both sides of a large pair, with an index built on the first line request.
pub(crate) struct Reader {
    left: Side,
    right: Side,
}

impl Reader {
    /// A reader over both sources; nothing is indexed here.
    pub(crate) fn new(left: LargeSide, right: LargeSide) -> Self {
        Self {
            left: Side::new(left.source, left.layout),
            right: Side::new(right.source, right.layout),
        }
    }

    /// Read the requested lines, capped at 2,000 rows and 2 MiB of text per side. A side cut
    /// by its budget shortens the window for both, and `rows` reports the real range.
    pub(crate) fn read(
        &mut self,
        rows: Range<u64>,
        cancel: &CancelToken,
    ) -> Result<Window, Reason> {
        let rows = cap_rows(rows);
        let left = self.left.read(&rows, cancel, SIDE_LIMIT)?;
        let right = self.right.read(&rows, cancel, SIDE_LIMIT)?;
        let asked = usize::try_from(rows.end.saturating_sub(rows.start)).unwrap_or(usize::MAX);
        let kept = [&left, &right]
            .into_iter()
            .filter(|side| side.cut)
            .map(|side| side.lines.len())
            .fold(asked, usize::min);
        let end = if kept < asked {
            rows.start
                .saturating_add(u64::try_from(kept).unwrap_or(u64::MAX))
        } else {
            rows.end
        };
        let (left_cut, right_cut) = (left.partial && kept > 0, right.partial && kept > 0);
        let (mut left, mut right) = ((left.lines, left.starts), (right.lines, right.starts));
        for side in [&mut left, &mut right] {
            side.0.truncate(kept);
            side.1.truncate(kept);
        }
        let totals = (self.left.total(cancel)?, self.right.total(cancel)?);
        Ok(Window {
            rows: rows.start..end,
            left: left.0,
            right: right.0,
            left_cut,
            right_cut,
            left_starts: left.1,
            right_starts: right.1,
            totals,
        })
    }
}

/// Clip a requested window to the most rows one view may ask for.
fn cap_rows(rows: Range<u64>) -> Range<u64> {
    let end = rows.start.saturating_add(MAX_ROWS).min(rows.end);
    rows.start..end
}

/// A shared source as a sized `ByteSource`, so `Laid` can give it the detected layout.
struct Shared(Arc<dyn ByteSource + Send + Sync>);

impl ByteSource for Shared {
    fn len(&self) -> u64 {
        self.0.len()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.read_at(offset, buf)
    }

    fn revalidate(&self) -> std::io::Result<bool> {
        self.0.revalidate()
    }
}

/// One side's source (scanned with its detected layout), layout and lazily built line index.
struct Side {
    source: Laid<Shared>,
    layout: TextLayout,
    index: Option<LineIndex>,
}

impl Side {
    /// A side that indexes itself when the view first asks for lines.
    fn new(source: Arc<dyn ByteSource + Send + Sync>, layout: TextLayout) -> Self {
        Self {
            source: Laid::new(Shared(source), layout),
            layout,
            index: None,
        }
    }

    /// Read `rows` as display lines within `limit` bytes. A cut keeps whole lines only, or
    /// the cut first line (`partial`) when even that one is longer than the limit. Rows past
    /// the end of an unchanged side read as no lines.
    fn read(
        &mut self,
        rows: &Range<u64>,
        cancel: &CancelToken,
        limit: usize,
    ) -> Result<SideLines, Reason> {
        if rows.is_empty() {
            return Ok(SideLines::default());
        }
        let Some((start, end)) = self.bounds(rows, cancel)? else {
            return Ok(SideLines::default());
        };
        let ahead = limit.saturating_add(LOOK_AHEAD);
        let mut bytes = read_range(&self.source, start..end, cancel, ahead)?;
        let kept = u64::try_from(bytes.len().min(limit)).unwrap_or(u64::MAX);
        let cut = end.saturating_sub(start) > kept;
        let partial = cut && self.cut(&mut bytes, limit);
        let (lines, starts) = decode_lines(&bytes, self.layout, start);
        Ok(SideLines {
            lines,
            starts,
            cut,
            partial,
        })
    }

    /// Shorten cut bytes to whole lines within `limit`, or, with no newline there, to the
    /// last whole character. True when the only line was cut.
    fn cut(&self, bytes: &mut Vec<u8>, limit: usize) -> bool {
        let kept = bytes.len().min(limit);
        if let Some(last) = self.layout.last_newline(&bytes[..kept]) {
            bytes.truncate(last.saturating_add(self.layout.unit()));
            return false;
        }
        let at = if kept < bytes.len() {
            self.layout.cut_back(bytes, kept)
        } else {
            kept
        };
        bytes.truncate(at);
        true
    }

    /// Find the byte range of `rows`; a range past the end stops at the source end. `None`
    /// when the side ends before `rows`; fails with `ChangedOnDisk` when it changed instead.
    fn bounds(
        &mut self,
        rows: &Range<u64>,
        cancel: &CancelToken,
    ) -> Result<Option<(u64, u64)>, Reason> {
        self.build(cancel)?;
        let index = self.index.as_ref().ok_or(Reason::ReadError)?;
        let first = rows.start.saturating_add(1);
        let last = rows.end.saturating_add(1);
        let found = index
            .line_start(&self.source, first)
            .map_err(|_| Reason::ReadError)?;
        let Some(start) = found else {
            return unchanged(&self.source, index).map(|()| None);
        };
        let end = index
            .line_start(&self.source, last)
            .map_err(|_| Reason::ReadError)?
            .unwrap_or_else(|| self.source.len());
        Ok(Some((start, end.max(start))))
    }

    /// The side's line count, from its index (built here when no window read it yet).
    fn total(&mut self, cancel: &CancelToken) -> Result<u64, Reason> {
        self.build(cancel)?;
        let index = self.index.as_ref().ok_or(Reason::ReadError)?;
        index
            .line_count(&self.source)
            .map_err(|_| Reason::ReadError)
    }

    /// Build the side's line index once, scanning the source in 256 KiB blocks.
    fn build(&mut self, cancel: &CancelToken) -> Result<(), Reason> {
        if self.index.is_none() {
            self.index = Some(build_index(&self.source, self.layout, cancel)?);
        }
        Ok(())
    }
}

/// Check that a side still matches its index: same size and a fresh stamp check. A change
/// is `ChangedOnDisk`; a failed check is a `ReadError`, so an unknown state never passes.
fn unchanged(source: &dyn ByteSource, index: &LineIndex) -> Result<(), Reason> {
    if source.len() != index.scanned() {
        return Err(Reason::ChangedOnDisk);
    }
    match source.revalidate() {
        Ok(true) => Ok(()),
        Ok(false) => Err(Reason::ChangedOnDisk),
        Err(_) => Err(Reason::ReadError),
    }
}

/// Scan a source into a line index, stopping at a cancel.
fn build_index(
    source: &dyn ByteSource,
    layout: TextLayout,
    cancel: &CancelToken,
) -> Result<LineIndex, Reason> {
    let mut index = LineIndex::with_layout(layout);
    let mut buffer = vec![0u8; SCAN_CHUNK];
    let mut at = 0u64;
    while at < source.len() {
        if cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        let want = usize::try_from(source.len().saturating_sub(at))
            .unwrap_or(usize::MAX)
            .min(SCAN_CHUNK);
        let read = read_full(source, at, &mut buffer[..want]).map_err(|_| Reason::ReadError)?;
        if read == 0 {
            break;
        }
        index.feed(&buffer[..read]);
        at = at.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
    }
    index.finish();
    Ok(index)
}

/// Read `start..end` into a byte vector, stopping at `limit` bytes.
fn read_range(
    source: &dyn ByteSource,
    range: Range<u64>,
    cancel: &CancelToken,
    limit: usize,
) -> Result<Vec<u8>, Reason> {
    let span = usize::try_from(range.end.saturating_sub(range.start)).unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(span.min(limit).min(READ_CHUNK));
    let mut at = range.start;
    while at < range.end && bytes.len() < limit {
        if cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        let left = limit.saturating_sub(bytes.len());
        let want = usize::try_from(range.end - at)
            .unwrap_or(usize::MAX)
            .min(READ_CHUNK)
            .min(left);
        let mut block = vec![0u8; want];
        let read = read_full(source, at, &mut block).map_err(|_| Reason::ReadError)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&block[..read]);
        at = at.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
    }
    Ok(bytes)
}

/// Split raw bytes read from file offset `start` into display lines and the file offset of each,
/// dropping the BOM at the file start.
fn decode_lines(bytes: &[u8], layout: TextLayout, start: u64) -> (Vec<String>, Vec<u64>) {
    let skip = if start == 0 {
        layout.bom_len().min(bytes.len())
    } else {
        0
    };
    let body = &bytes[skip..];
    let base = start.saturating_add(u64::try_from(skip).unwrap_or(0));
    let at = |from: usize| base.saturating_add(u64::try_from(from).unwrap_or(u64::MAX));
    let unit = layout.unit();
    let (mut lines, mut starts) = (Vec::new(), Vec::new());
    let mut from = 0;
    for newline in layout.newlines(body) {
        lines.push(decode(&body[from..newline], layout));
        starts.push(at(from));
        from = newline.saturating_add(unit);
    }
    if from < body.len() {
        lines.push(decode(&body[from..], layout));
        starts.push(at(from));
    }
    (lines, starts)
}

/// Decode one line's bytes with the side's encoding.
fn decode(bytes: &[u8], layout: TextLayout) -> String {
    layout
        .text
        .encoding
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}
