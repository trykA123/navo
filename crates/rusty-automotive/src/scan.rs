use std::io::BufRead;
use std::ops::Range;
use std::sync::atomic::Ordering;

use crate::image::{Counts, Image, Problem, Start};
use crate::record::{Cursor, Event, decode};
use crate::{Format, IndexError, Watch};

const PIECE_SPAN: u64 = 4096;
pub(crate) const MAX_SPAN: u64 = PIECE_SPAN + 256;
const CHECK_EVERY: u64 = 4096;
const KEPT_PROBLEMS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Piece {
    pub start: u64,
    pub end: u64,
    pub offset: u64,
    pub file_end: u64,
    pub line: u64,
    pub base: u64,
}

struct Scan {
    cursor: Cursor,
    offset: u64,
    line: u64,
    counts: Counts,
    start: Option<Start>,
    has_end: bool,
    problems: Vec<Problem>,
    problem_total: u64,
    checksum_errors: u64,
    piece: Option<Piece>,
    pieces: Vec<Piece>,
}

pub fn index(
    format: Format,
    mut source: impl BufRead,
    watch: Watch<'_>,
) -> Result<Image, IndexError> {
    let mut scan = Scan::new(format);
    let (mut line, mut bytes) = (Vec::new(), Vec::new());
    loop {
        line.clear();
        let read = source
            .read_until(b'\n', &mut line)
            .map_err(|error| IndexError::Read(error.to_string()))?;
        if read == 0 {
            break;
        }
        scan.take(&line, &mut bytes);
        if scan.line.is_multiple_of(CHECK_EVERY) {
            if watch.cancel.load(Ordering::Relaxed) {
                return Err(IndexError::Cancelled);
            }
            (watch.progress)(scan.offset);
        }
    }
    (watch.progress)(scan.offset);
    Ok(scan.finish())
}

impl Scan {
    fn new(format: Format) -> Self {
        Self {
            cursor: Cursor { format, base: 0 },
            offset: 0,
            line: 0,
            counts: Counts::default(),
            start: None,
            has_end: false,
            problems: Vec::new(),
            problem_total: 0,
            checksum_errors: 0,
            piece: None,
            pieces: Vec::new(),
        }
    }

    fn note(&mut self, problem: Problem) {
        self.problem_total += 1;
        if self.problems.len() < KEPT_PROBLEMS {
            self.problems.push(problem);
        }
    }

    fn take(&mut self, raw: &[u8], bytes: &mut Vec<u8>) {
        let at = self.offset;
        self.offset += raw.len() as u64;
        self.line += 1;
        let line = self.line;
        let text = raw.trim_ascii();
        if text.is_empty() {
            return;
        }
        let record = match decode(self.cursor.format, text, bytes) {
            Ok(record) => record,
            Err(reason) => return self.note(Problem::Malformed { line, reason }),
        };
        if let Some((stored, computed)) = record.wrong_sum {
            self.checksum_errors += 1;
            self.note(Problem::Checksum {
                line,
                stored,
                computed,
            });
        }
        if self.has_end {
            return self.note(Problem::AfterEnd { line });
        }
        self.counts.add(record.kind);
        let base = self.cursor.base;
        match self.cursor.locate(&record) {
            Ok(Event::Data(address)) => {
                self.data(address..address + record.data.len() as u64, at, base)
            }
            Ok(Event::Start(start)) => self.start = Some(start),
            Ok(Event::End(start)) => {
                self.has_end = true;
                self.start = start.or(self.start);
            }
            Ok(Event::Other) => {}
            Err(reason) => self.note(Problem::Malformed { line, reason }),
        }
    }

    fn data(&mut self, span: Range<u64>, at: u64, base: u64) {
        if span.is_empty() {
            return;
        }
        if let Some(piece) = &mut self.piece
            && piece.end == span.start
            && piece.end - piece.start < PIECE_SPAN
        {
            piece.end = span.end;
            piece.file_end = self.offset;
            return;
        }
        let next = Piece {
            start: span.start,
            end: span.end,
            offset: at,
            file_end: self.offset,
            line: self.line,
            base,
        };
        self.pieces.extend(self.piece.replace(next));
    }

    fn finish(mut self) -> Image {
        self.pieces.extend(self.piece.take());
        self.pieces.sort_by_key(|piece| piece.start);
        let mut ranges: Vec<Range<u64>> = Vec::new();
        let mut overlaps = Vec::new();
        for piece in &self.pieces {
            match ranges.last_mut() {
                Some(last) if piece.start < last.end => {
                    overlaps.push(Problem::Overlap {
                        line: piece.line,
                        range: piece.start..piece.end.min(last.end),
                    });
                    last.end = last.end.max(piece.end);
                }
                Some(last) if piece.start == last.end => last.end = piece.end,
                _ => ranges.push(piece.start..piece.end),
            }
        }
        for overlap in overlaps {
            self.note(overlap);
        }
        Image {
            format: self.cursor.format,
            ranges,
            counts: self.counts,
            start: self.start,
            has_end: self.has_end,
            problems: self.problems,
            problem_total: self.problem_total,
            checksum_errors: self.checksum_errors,
            lines: self.line,
            pieces: self.pieces,
        }
    }
}
