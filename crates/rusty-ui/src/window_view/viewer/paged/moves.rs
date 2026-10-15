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

impl Move {
    fn is_absolute(self) -> bool {
        matches!(
            self,
            Self::End | Self::Fraction(_) | Self::Offset(_) | Self::Line(_)
        )
    }
}

#[derive(Debug, Default)]
pub(super) struct MoveQueue {
    queued: Vec<(u64, Move)>,
    next: u64,
    sent: Option<u64>,
}

impl MoveQueue {
    pub(super) fn push(&mut self, to: Move) {
        if to.is_absolute() {
            self.queued.clear();
        }
        let sent = self.sent;
        if let (Some((seq, Move::Rows(rows))), Move::Rows(more)) = (self.queued.last_mut(), to)
            && sent.is_none_or(|sent| *seq > sent)
        {
            *rows = rows.saturating_add(more);
            return;
        }
        self.next += 1;
        self.queued.push((self.next, to));
    }

    pub(super) fn send(&mut self) -> (Vec<Move>, u64) {
        self.sent = Some(self.next);
        (self.queued.iter().map(|(_, to)| *to).collect(), self.next)
    }

    pub(super) fn land(&mut self, through: u64) {
        self.queued.retain(|(seq, _)| *seq > through);
        self.sent = None;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{Move, MoveQueue};

    #[test]
    fn rows_merge_until_they_are_sent() {
        let mut queue = MoveQueue::default();
        queue.push(Move::Rows(1));
        queue.push(Move::Rows(2));
        let (sent, through) = queue.send();
        assert_eq!(sent, [Move::Rows(3)]);
        queue.push(Move::Rows(4));
        queue.land(through);
        assert_eq!(queue.send().0, [Move::Rows(4)]);
    }

    #[test]
    fn an_absolute_move_drops_what_was_queued_before_it() {
        let mut queue = MoveQueue::default();
        queue.push(Move::Rows(5));
        let (_, through) = queue.send();
        queue.push(Move::Rows(1));
        queue.push(Move::End);
        queue.push(Move::Rows(-1));
        queue.land(through);
        assert_eq!(queue.send().0, [Move::End, Move::Rows(-1)]);
    }
}
