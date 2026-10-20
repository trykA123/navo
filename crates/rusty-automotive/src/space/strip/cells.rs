use std::ops::Range;

use crate::space::{Kind, Region};

use super::Scale;

const KINDS: [Kind; Kind::COUNT] = [
    Kind::Code,
    Kind::Rodata,
    Kind::Data,
    Kind::Bss,
    Kind::Other,
    Kind::Fill,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub x0: u32,
    pub x1: u32,
    pub kind: Kind,
    pub blocks: Range<usize>,
    pub bytes: u64,
}

struct Open {
    cell: Cell,
    tally: [u64; Kind::COUNT],
    narrow: bool,
}

struct Piece {
    at: usize,
    kind: Kind,
    bytes: u64,
    columns: Range<u32>,
    narrow: bool,
}

#[must_use]
pub fn cells(region: &Region, scale: &Scale, width_px: u32) -> Vec<Cell> {
    let spans = region.blocks.iter().map(|block| Span {
        at: 0,
        range: block.start..block.start.saturating_add(block.size),
        kind: block.kind,
    });
    merged(
        spans.enumerate().map(|(at, span)| Span { at, ..span }),
        scale,
        width_px,
    )
}

pub(super) struct Span {
    pub at: usize,
    pub range: Range<u64>,
    pub kind: Kind,
}

pub(super) fn merged(spans: impl Iterator<Item = Span>, scale: &Scale, width_px: u32) -> Vec<Cell> {
    if width_px == 0 {
        return Vec::new();
    }
    let domain = scale.domain();
    let mut out: Vec<Open> = Vec::new();
    for span in spans {
        if span.range.start >= domain.end {
            break;
        }
        let Some(clipped) = clip(&span.range, &domain) else {
            continue;
        };
        let (columns, narrow) = columns(scale, &clipped, width_px);
        let piece = Piece {
            at: span.at,
            kind: span.kind,
            bytes: clipped.end - clipped.start,
            columns,
            narrow,
        };
        place(&mut out, piece, width_px);
    }
    out.into_iter().map(Open::finish).collect()
}

fn clip(range: &Range<u64>, domain: &Range<u64>) -> Option<Range<u64>> {
    let start = range.start.max(domain.start);
    let end = range.end.min(domain.end);
    (end > start).then_some(start..end)
}

fn columns(scale: &Scale, span: &Range<u64>, width: u32) -> (Range<u32>, bool) {
    let edge = |offset| scale.x(offset) * f64::from(width);
    let (from, to) = (edge(span.start), edge(span.end));
    if to - from < 1.0 {
        let column = (from.floor() as u32).min(width - 1);
        return (column..column + 1, true);
    }
    let start = (from.round() as u32).min(width - 1);
    (start..(to.round() as u32).clamp(start + 1, width), false)
}

fn place(out: &mut Vec<Open>, piece: Piece, width: u32) {
    let narrow = piece.narrow;
    let cursor = out.last().map_or(0, |last| last.cell.x1);
    if let Some(last) = out.last_mut() {
        let joins = narrow && last.narrow && piece.columns.start < last.cell.x1;
        if joins || cursor >= width {
            last.absorb(&piece);
            return;
        }
    }
    let x0 = piece.columns.start.max(cursor);
    let x1 = if narrow {
        x0 + 1
    } else {
        piece.columns.end.max(x0 + 1)
    };
    out.push(Open::new(&piece, x0..x1, narrow));
}

impl Open {
    fn new(piece: &Piece, columns: Range<u32>, narrow: bool) -> Self {
        let mut open = Self {
            cell: Cell {
                x0: columns.start,
                x1: columns.end,
                kind: piece.kind,
                blocks: piece.at..piece.at,
                bytes: 0,
            },
            tally: [0; Kind::COUNT],
            narrow,
        };
        open.absorb(piece);
        open
    }

    fn absorb(&mut self, piece: &Piece) {
        self.cell.blocks.end = piece.at + 1;
        self.cell.bytes += piece.bytes;
        self.tally[piece.kind.index()] += piece.bytes;
    }

    fn finish(self) -> Cell {
        let mut best = 0;
        for (at, bytes) in self.tally.iter().enumerate() {
            if *bytes > self.tally[best] {
                best = at;
            }
        }
        Cell {
            kind: KINDS[best],
            ..self.cell
        }
    }
}
