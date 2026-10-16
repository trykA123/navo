mod block;
mod build;
mod scan;
mod table;
mod text;

use std::fmt;
use std::ops::Range;

pub const MAX_SOURCE: usize = 1024 * 1024;
pub const MAX_BLOCKS: usize = 200_000;
pub const MAX_SPANS: usize = 2_000_000;
pub const MAX_BLOCK_TEXT: usize = 4 * 1024;
pub const MAX_COLUMNS: usize = 64;
pub const MAX_DEPTH: u8 = 8;
pub const TABLES_AS_TEXT: &str = "Tables shown as text: too many cells";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    TooLarge,
    TooManyBlocks,
    TooManySpans,
    TooManyFootnotes,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "Markdown over 1 MB",
            Self::TooManyBlocks => "Markdown with over 200,000 blocks",
            Self::TooManySpans => "Markdown with over 2,000,000 styled runs",
            Self::TooManyFootnotes => "Markdown with over 2,000 footnote lines",
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Style(u8);

impl Style {
    pub const PLAIN: Self = Self(0);
    pub const BOLD: Self = Self(1);
    pub const ITALIC: Self = Self(1 << 1);
    pub const STRIKE: Self = Self(1 << 2);
    pub const CODE: Self = Self(1 << 3);
    pub const HTML: Self = Self(1 << 4);
    pub const IMAGE: Self = Self(1 << 5);
    pub const FOOTNOTE: Self = Self(1 << 6);

    #[must_use]
    pub fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    #[must_use]
    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

const NO_LINK: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    len: u32,
    link: u32,
    style: Style,
    cell: u8,
}

impl Span {
    #[must_use]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn style(&self) -> Style {
        self.style
    }

    #[must_use]
    pub fn link(&self) -> Option<u32> {
        (self.link != NO_LINK).then_some(self.link)
    }

    #[must_use]
    pub fn cell(&self) -> u8 {
        self.cell
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Bullet,
    Number(u32),
    Task(bool),
    Footnote(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Code { language: Option<u32> },
    Html,
    Row { table: u32, is_head: bool },
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub marker: Option<Marker>,
    pub quote: u8,
    pub list: u8,
    pub is_continued: bool,
    text: (u32, u32),
    spans: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct WordSpot {
    text: (u32, u32),
    chars: u16,
    is_mono: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnWord<'a> {
    pub text: &'a str,
    pub is_mono: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    alignments: Vec<Align>,
    widths: Vec<u16>,
    words: Vec<WordSpot>,
}

impl Table {
    #[must_use]
    pub fn alignments(&self) -> &[Align] {
        &self.alignments
    }

    #[must_use]
    pub fn widths(&self) -> &[u16] {
        &self.widths
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    target: Box<str>,
    block: u32,
}

impl Link {
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    #[must_use]
    pub fn block(&self) -> usize {
        self.block as usize
    }
}

#[derive(Debug, Default)]
pub struct Document {
    text: String,
    blocks: Vec<Block>,
    spans: Vec<Span>,
    links: Vec<Link>,
    tables: Vec<Table>,
    labels: Vec<Box<str>>,
    notes: Vec<&'static str>,
    lines: Vec<u32>,
}

fn range((start, end): (u32, u32)) -> Range<usize> {
    start as usize..end as usize
}

impl Document {
    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    #[must_use]
    pub fn text(&self, block: &Block) -> &str {
        self.text.get(range(block.text)).unwrap_or_default()
    }

    #[must_use]
    pub fn spans(&self, block: &Block) -> &[Span] {
        self.spans.get(range(block.spans)).unwrap_or_default()
    }

    #[must_use]
    pub fn spans_total(&self) -> usize {
        self.spans.len()
    }

    #[must_use]
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    #[must_use]
    pub fn link(&self, id: u32) -> Option<&Link> {
        self.links.get(id as usize)
    }

    #[must_use]
    pub fn table(&self, id: u32) -> Option<&Table> {
        self.tables.get(id as usize)
    }

    #[must_use]
    pub fn column_word(&self, table: u32, column: usize) -> Option<ColumnWord<'_>> {
        let spot = self.table(table)?.words.get(column)?;
        Some(ColumnWord {
            text: self.text.get(range(spot.text)).unwrap_or_default(),
            is_mono: spot.is_mono,
        })
    }

    #[must_use]
    pub fn line(&self, block: usize) -> Option<u32> {
        self.lines.get(block).copied()
    }

    #[must_use]
    pub fn block_at_line(&self, line: u32) -> Option<usize> {
        let after = self.lines.partition_point(|start| *start <= line);
        (!self.lines.is_empty()).then(|| after.saturating_sub(1))
    }

    #[must_use]
    pub fn label(&self, id: u32) -> Option<&str> {
        self.labels.get(id as usize).map(AsRef::as_ref)
    }

    #[must_use]
    pub fn notes(&self) -> &[&'static str] {
        &self.notes
    }

    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        let links: usize = self.links.iter().map(|link| link.target.len()).sum();
        let tables: usize = self
            .tables
            .iter()
            .map(|table| {
                table.alignments.capacity()
                    + table.widths.capacity() * 2
                    + table.words.capacity() * size_of::<WordSpot>()
            })
            .sum();
        let labels: usize = self.labels.iter().map(|label| label.len()).sum();
        self.text.capacity()
            + self.blocks.capacity() * size_of::<Block>()
            + self.spans.capacity() * size_of::<Span>()
            + self.links.capacity() * size_of::<Link>()
            + self.tables.capacity() * size_of::<Table>()
            + self.labels.capacity() * size_of::<Box<str>>()
            + self.lines.capacity() * size_of::<u32>()
            + links
            + tables
            + labels
    }
}

pub fn parse(source: &str) -> Result<Document, Refusal> {
    if source.len() > MAX_SOURCE {
        return Err(Refusal::TooLarge);
    }
    let source = text::normalise_line_ends(source);
    let plan = scan::plan(&source)?;
    let mut doc = build::Builder::run(&source, plan.options)?;
    doc.notes = plan.notes;
    Ok(doc)
}

#[cfg(test)]
mod tests;
