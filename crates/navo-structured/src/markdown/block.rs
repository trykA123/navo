use super::build::Builder;
use super::text::{clean, cut_point, prefix};
use super::{
    Block, BlockKind, MAX_BLOCK_TEXT, MAX_BLOCKS, MAX_SPANS, Marker, Refusal, Span, Style,
};

const ELLIPSIS: &str = "…";

#[derive(Debug, Clone, Copy)]
pub(super) struct Open {
    pub kind: BlockKind,
    pub marker: Option<Marker>,
    quote: u8,
    list: u8,
    is_continued: bool,
    is_cut: bool,
    text_start: usize,
    span_start: usize,
    line: u32,
}

impl Open {
    pub(super) fn new(
        kind: BlockKind,
        marker: Option<Marker>,
        (quote, list): (u8, u8),
        (text_start, span_start, line): (usize, usize, u32),
    ) -> Self {
        Self {
            kind,
            marker,
            quote,
            list,
            is_continued: false,
            is_cut: false,
            text_start,
            span_start,
            line,
        }
    }

    pub(super) fn is_row(&self) -> bool {
        matches!(self.kind, BlockKind::Row { .. })
    }

    fn prefers(&self) -> char {
        match self.kind {
            BlockKind::Code { .. } | BlockKind::Html => '\n',
            _ => ' ',
        }
    }
}

fn offset(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

pub(super) struct Piece {
    pub style: Style,
    pub link: u32,
    pub cell: u8,
}

impl Builder {
    pub(super) fn push_text(&mut self, text: &str, piece: &Piece) -> Result<(), Refusal> {
        let cleaned = clean(text);
        let mut rest: &str = &cleaned;
        while !rest.is_empty() {
            let Some(open) = self.open else {
                return Ok(());
            };
            let room = MAX_BLOCK_TEXT.saturating_sub(self.doc.text.len() - open.text_start);
            if rest.len() <= room {
                return self.append(rest, piece);
            }
            if open.is_row() {
                return self.cut_row(rest, room, piece);
            }
            let cut = cut_point(rest, room, open.prefers());
            self.append(&rest[..cut], piece)?;
            self.split()?;
            rest = &rest[cut..];
        }
        Ok(())
    }

    fn cut_row(&mut self, text: &str, room: usize, piece: &Piece) -> Result<(), Refusal> {
        let Some(open) = self.open.as_mut() else {
            return Ok(());
        };
        if open.is_cut {
            return Ok(());
        }
        open.is_cut = true;
        self.append(prefix(text, room), piece)?;
        self.append(ELLIPSIS, piece)
    }

    fn append(&mut self, text: &str, piece: &Piece) -> Result<(), Refusal> {
        let Some(open) = self.open else {
            return Ok(());
        };
        if text.is_empty() {
            return Ok(());
        }
        self.doc.text.push_str(text);
        let len = offset(text.len());
        if self.doc.spans.len() > open.span_start
            && let Some(last) = self.doc.spans.last_mut()
            && (last.style, last.link, last.cell) == (piece.style, piece.link, piece.cell)
        {
            last.len = last.len.saturating_add(len);
            return Ok(());
        }
        if self.doc.spans.len() >= MAX_SPANS {
            return Err(Refusal::TooManySpans);
        }
        self.doc.spans.push(Span {
            len,
            link: piece.link,
            style: piece.style,
            cell: piece.cell,
        });
        Ok(())
    }

    fn split(&mut self) -> Result<(), Refusal> {
        let Some(open) = self.open else {
            return Ok(());
        };
        self.finish()?;
        self.open = Some(Open {
            marker: None,
            is_continued: true,
            text_start: self.doc.text.len(),
            span_start: self.doc.spans.len(),
            ..open
        });
        Ok(())
    }

    fn trim_newlines(&mut self, open: &Open) {
        while self.doc.text.len() > open.text_start && self.doc.text.ends_with('\n') {
            self.doc.text.pop();
            let Some(last) = self.doc.spans.last_mut() else {
                break;
            };
            last.len = last.len.saturating_sub(1);
            if last.len == 0 && self.doc.spans.len() > open.span_start {
                self.doc.spans.pop();
            }
        }
    }

    pub(super) fn finish(&mut self) -> Result<(), Refusal> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        if matches!(open.kind, BlockKind::Code { .. } | BlockKind::Html) {
            self.trim_newlines(&open);
        }
        let is_empty = self.doc.text.len() == open.text_start;
        if is_empty && open.kind == BlockKind::Paragraph && open.marker.is_none() {
            return Ok(());
        }
        if self.doc.blocks.len() >= MAX_BLOCKS {
            return Err(Refusal::TooManyBlocks);
        }
        self.doc.blocks.push(Block {
            kind: open.kind,
            marker: open.marker,
            quote: open.quote,
            list: open.list,
            is_continued: open.is_continued,
            text: (offset(open.text_start), offset(self.doc.text.len())),
            spans: (offset(open.span_start), offset(self.doc.spans.len())),
        });
        self.doc.lines.push(open.line);
        Ok(())
    }
}
