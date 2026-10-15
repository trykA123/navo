use rusty_core::paging::{ByteSource, HEX_WIDTH, LineIndex};

use super::PagedView;
use super::lines::{HexLine, PagedKind, Screen, TextLine, hex_lines, text_lines};
use super::moves::{Frame, Move, apply_moves};
use crate::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Want {
    pub top: u64,
    pub move_seq: u64,
    pub count: usize,
    pub kind: PagedKind,
    pub index_rev: u64,
    pub source_rev: u64,
}

enum Drawn {
    Hex(Screen<HexLine>),
    Text(Screen<TextLine>),
}

struct Filled {
    want: Want,
    drawn: Result<Drawn, String>,
}

struct FillResult {
    top: u64,
    note: Option<String>,
    consumed: usize,
    drawn: Result<Drawn, String>,
}

#[derive(Default)]
pub(super) struct RowCache {
    filled: Option<Filled>,
    pending: Option<(Want, gpui_kit::Task<()>)>,
}

pub(super) enum Shown<'a> {
    Hex(&'a [HexLine]),
    Text(&'a [TextLine]),
    Failed(&'a str),
}

pub(super) struct Screenful<'a> {
    pub shown: Shown<'a>,
    pub missing: usize,
    pub end: u64,
}

struct Rows<'a, L> {
    lines: &'a [L],
    missing: usize,
    end: u64,
}

fn from_top<L>(
    lines: &[L],
    want: Want,
    len: u64,
    span: impl Fn(&L) -> (u64, u64),
) -> Option<Rows<'_, L>> {
    let at = lines.iter().position(|line| span(line).0 == want.top)?;
    let lines = &lines[at..lines.len().min(at + want.count)];
    let end = lines.last().map_or(want.top, |line| span(line).1);
    let missing = if end < len {
        want.count - lines.len()
    } else {
        0
    };
    Some(Rows {
        lines,
        missing,
        end,
    })
}

fn empty(want: Want, len: u64) -> Screenful<'static> {
    let shown = match want.kind {
        PagedKind::Hex => Shown::Hex(&[]),
        PagedKind::Text => Shown::Text(&[]),
    };
    let missing = if want.top < len { want.count } else { 0 };
    Screenful {
        shown,
        missing,
        end: want.top,
    }
}

fn hex_span(len: u64) -> impl Fn(&HexLine) -> (u64, u64) {
    move |line| (line.start, (line.start + HEX_WIDTH).min(len))
}

fn text_span(line: &TextLine) -> (u64, u64) {
    (line.start, line.next)
}

impl RowCache {
    pub(super) fn screen(&self, want: Want, len: u64) -> Screenful<'_> {
        let Some(filled) = self.filled.as_ref().filter(|filled| {
            filled.want.kind == want.kind && filled.want.source_rev == want.source_rev
        }) else {
            return empty(want, len);
        };
        let same_top = filled.want.top == want.top;
        let found = match &filled.drawn {
            Err(error) if same_top => {
                return Screenful {
                    shown: Shown::Failed(error),
                    missing: 0,
                    end: want.top,
                };
            }
            Err(_) => None,
            Ok(Drawn::Hex(screen)) => from_top(&screen.lines, want, len, hex_span(len))
                .map(|rows| (Shown::Hex(rows.lines), rows.missing, rows.end)),
            Ok(Drawn::Text(screen)) => from_top(&screen.lines, want, len, text_span)
                .map(|rows| (Shown::Text(rows.lines), rows.missing, rows.end)),
        };
        match found {
            Some((shown, missing, end)) => Screenful {
                shown,
                missing,
                end,
            },
            None if same_top => empty(want, 0),
            None => empty(want, len),
        }
    }
}

fn fill<S: ByteSource + ?Sized>(
    source: &S,
    want: Want,
    moves: &[Move],
    index: Option<&LineIndex>,
) -> FillResult {
    let frame = Frame {
        source,
        kind: want.kind,
        count: want.count as u64,
        index,
    };
    let (top, note) = apply_moves(&frame, want.top, moves);
    let drawn = match want.kind {
        PagedKind::Hex => hex_lines(source, top, want.count).map(Drawn::Hex),
        PagedKind::Text => text_lines(source, top, want.count, index).map(Drawn::Text),
    };
    FillResult {
        top,
        note,
        consumed: moves.len(),
        drawn,
    }
}

impl PagedView {
    pub(super) fn want(&self) -> Want {
        Want {
            top: self.top,
            move_seq: self.move_seq,
            count: usize::try_from(self.rows).unwrap_or(usize::MAX),
            kind: self.kind,
            index_rev: self.index_rev,
            source_rev: self.source_rev,
        }
    }

    pub(super) fn request_rows(&mut self, cx: &mut Context<Self>) {
        let want = self.want();
        let cache = &self.cache;
        let is_filled = cache
            .filled
            .as_ref()
            .is_some_and(|filled| filled.want == want && self.moves.is_empty());
        let is_pending = cache
            .pending
            .as_ref()
            .is_some_and(|(wanted, _)| *wanted == want);
        if is_filled || is_pending {
            return;
        }
        let source = self.source.clone();
        let index = self.index.clone();
        let moves = self.moves.clone();
        let task = cx.spawn(async move |this, cx| {
            let filled = cx
                .background_spawn(async move { fill(&*source, want, &moves, index.as_deref()) })
                .await;
            let _ = this.update(cx, |view, cx| view.rows_filled(want, filled, cx));
        });
        self.cache.pending = Some((want, task));
    }

    fn rows_filled(&mut self, want: Want, filled: FillResult, cx: &mut Context<Self>) {
        let is_current = self
            .cache
            .pending
            .as_ref()
            .is_some_and(|(wanted, _)| *wanted == want);
        if !is_current {
            return;
        }
        self.cache.pending = None;
        self.top = filled.top;
        self.moves.drain(..filled.consumed.min(self.moves.len()));
        if let Some(note) = filled.note {
            self.note = Some(note.into());
        }
        let want = Want {
            top: filled.top,
            ..want
        };
        self.cache.filled = Some(Filled {
            want,
            drawn: filled.drawn,
        });
        if !self.moves.is_empty() {
            self.request_rows(cx);
        }
        cx.notify();
    }
}
