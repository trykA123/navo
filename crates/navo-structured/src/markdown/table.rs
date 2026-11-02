use super::block::Piece;
use super::build::Builder;
use super::{Align, BlockKind, MAX_COLUMNS, NO_LINK, Refusal, Span, Style, Table, WordSpot};

const ELLIPSIS: &str = "…";
const MAX_CELL_WIDTH: usize = 40;
const MAX_WORD: usize = 40;
const MAX_LAST_COLUMN: u8 = (MAX_COLUMNS - 1) as u8;

#[derive(Default)]
pub(super) struct TableState {
    id: u32,
    cell: usize,
    cell_start: usize,
    span_start: usize,
    is_cut: bool,
}

fn align(align: pulldown_cmark::Alignment) -> Align {
    match align {
        pulldown_cmark::Alignment::None => Align::None,
        pulldown_cmark::Alignment::Left => Align::Left,
        pulldown_cmark::Alignment::Center => Align::Center,
        pulldown_cmark::Alignment::Right => Align::Right,
    }
}

fn offset(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

fn is_mono_at(spans: &[Span], at: usize) -> bool {
    let mut end = 0;
    for span in spans {
        end += span.len();
        if at < end {
            return span.style().has(Style::CODE) || span.style().has(Style::HTML);
        }
    }
    false
}

fn longest_words(cell: &str, cell_start: usize, spans: &[Span]) -> [WordSpot; 2] {
    let mut best = [WordSpot::default(); 2];
    let mut at = 0;
    for word in cell.split([' ', '\n']) {
        let chars = word.chars().take(MAX_WORD).count();
        let is_mono = chars > 0 && is_mono_at(spans, at);
        let slot = &mut best[usize::from(is_mono)];
        if chars > usize::from(slot.chars) {
            let end = word
                .char_indices()
                .nth(MAX_WORD)
                .map_or(word.len(), |(cut, _)| cut);
            *slot = WordSpot {
                text: (offset(cell_start + at), offset(cell_start + at + end)),
                chars: u16::try_from(chars).unwrap_or(u16::MAX),
            };
        }
        at += word.len() + 1;
    }
    best
}

impl Builder {
    pub(super) fn start_table(
        &mut self,
        alignments: &[pulldown_cmark::Alignment],
    ) -> Result<(), Refusal> {
        self.finish()?;
        let alignments: Vec<Align> = alignments
            .iter()
            .take(MAX_COLUMNS)
            .map(|a| align(*a))
            .collect();
        let id = u32::try_from(self.doc.tables.len()).unwrap_or(u32::MAX);
        self.doc.tables.push(Table {
            widths: vec![1; alignments.len()],
            words: vec![[WordSpot::default(); 2]; alignments.len()],
            alignments,
        });
        self.table = Some(TableState {
            id,
            ..TableState::default()
        });
        Ok(())
    }

    pub(super) fn start_row(&mut self, is_head: bool) -> Result<(), Refusal> {
        let Some(table) = self.table.as_mut() else {
            return Ok(());
        };
        let id = table.id;
        table.cell = usize::MAX;
        table.is_cut = false;
        self.begin(BlockKind::Row { table: id, is_head })
    }

    pub(super) fn start_cell(&mut self) {
        let (text_len, spans_len) = (self.doc.text.len(), self.doc.spans.len());
        let Some(table) = self.table.as_mut() else {
            return;
        };
        table.cell = table.cell.wrapping_add(1);
        table.cell_start = text_len;
        table.span_start = spans_len;
    }

    pub(super) fn mark_cut_row(&mut self) -> Result<(), Refusal> {
        let Some(table) = self.table.as_mut() else {
            return Ok(());
        };
        if table.is_cut {
            return Ok(());
        }
        table.is_cut = true;
        let piece = Piece {
            style: Style::PLAIN,
            link: NO_LINK,
            cell: MAX_LAST_COLUMN,
        };
        self.push_text(ELLIPSIS, &piece)
    }

    pub(super) fn end_cell(&mut self) {
        let Some(table) = self.table.as_ref() else {
            return;
        };
        if table.cell >= MAX_COLUMNS {
            return;
        }
        let shown = self.doc.text.get(table.cell_start..).unwrap_or_default();
        let width = u16::try_from(shown.chars().take(MAX_CELL_WIDTH).count()).unwrap_or(1);
        let spans = self.doc.spans.get(table.span_start..).unwrap_or_default();
        let words = longest_words(shown, table.cell_start, spans);
        let Some(found) = self.doc.tables.get_mut(table.id as usize) else {
            return;
        };
        if let Some(column) = found.widths.get_mut(table.cell) {
            *column = (*column).max(width);
        }
        let Some(longest) = found.words.get_mut(table.cell) else {
            return;
        };
        for (longest, word) in longest.iter_mut().zip(words) {
            if word.chars > longest.chars {
                *longest = word;
            }
        }
    }

    pub(super) fn cell(&self) -> Option<u8> {
        match &self.table {
            Some(table) if self.open.is_some_and(|open| open.is_row()) => u8::try_from(table.cell)
                .ok()
                .filter(|cell| usize::from(*cell) < MAX_COLUMNS),
            _ => Some(0),
        }
    }
}
