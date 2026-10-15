use rusty_core::paging::{ByteSource, LineIndex};

use super::lines::{PagedKind, end_top, row_start, step};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Move {
    Rows(i64),
    End,
    Fraction(f64),
    Offset(u64),
    Realign,
    Line(u64),
}

pub(super) struct Frame<'a, S: ?Sized> {
    pub source: &'a S,
    pub kind: PagedKind,
    pub count: u64,
    pub index: Option<&'a LineIndex>,
}

fn line_top<S: ByteSource + ?Sized>(frame: &Frame<'_, S>, line: u64) -> Result<u64, String> {
    let Some(index) = frame.index else {
        return Err("Not indexed yet.".to_owned());
    };
    match index.line_start(frame.source, line)? {
        Some(start) => Ok(start),
        None if index.is_done() => Err(format!("There is no line {line}.")),
        None => Err("Not indexed yet.".to_owned()),
    }
}

fn apply<S: ByteSource + ?Sized>(frame: &Frame<'_, S>, top: u64, to: Move) -> Result<u64, String> {
    let (source, kind) = (frame.source, frame.kind);
    match to {
        Move::Rows(rows) if rows > 0 => {
            let moved = step(source, kind, top, rows)?;
            Ok(moved.min(end_top(source, kind, frame.count)?).max(top))
        }
        Move::Rows(rows) => step(source, kind, top, rows),
        Move::End => end_top(source, kind, frame.count),
        Move::Fraction(fraction) => {
            let target = (source.len() as f64 * fraction) as u64;
            Ok(row_start(source, kind, target)?.min(end_top(source, kind, frame.count)?))
        }
        Move::Offset(offset) => row_start(source, kind, offset.min(source.len())),
        Move::Realign => row_start(source, kind, top.min(source.len())),
        Move::Line(line) => line_top(frame, line),
    }
}

pub(super) fn apply_moves<S: ByteSource + ?Sized>(
    frame: &Frame<'_, S>,
    top: u64,
    moves: &[Move],
) -> (u64, Option<String>) {
    moves.iter().fold((top, None), |(top, note), to| {
        match apply(frame, top, *to) {
            Ok(moved) => (moved, note),
            Err(error) => (top, Some(error)),
        }
    })
}
