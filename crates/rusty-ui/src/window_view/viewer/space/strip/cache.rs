use std::ops::Range;

use rusty_automotive::space::{Cell, Owners, Region, Scale, ScaleMode, cells, inner};

pub(super) const WIDTH_STEP: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    pub width: u32,
    pub mode: ScaleMode,
    pub zoom: Option<usize>,
}

impl Key {
    pub fn new(width_px: f32, mode: ScaleMode, zoom: Option<usize>) -> Option<Self> {
        let width = bucket(width_px);
        (width > 0).then_some(Self { width, mode, zoom })
    }
}

fn bucket(width_px: f32) -> u32 {
    let whole = if width_px.is_finite() {
        width_px.max(0.) as u32
    } else {
        0
    };
    whole / WIDTH_STEP * WIDTH_STEP
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Block(usize),
    Owner(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Over {
    One(Item),
    Sections { count: usize, bytes: u64 },
    Symbols { count: usize, bytes: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hit {
    pub block: usize,
    pub over: Over,
    pub x: Range<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Prev,
    Next,
    First,
    Last,
}

pub(super) struct Inner {
    pub block: usize,
    pub cells: Vec<Cell>,
}

pub(super) struct Layout {
    pub key: Key,
    pub scale: Scale,
    pub cells: Vec<Cell>,
    pub inner: Vec<Inner>,
}

fn find(cells: &[Cell], column: u32) -> Option<&Cell> {
    let at = cells.partition_point(|cell| cell.x1 <= column);
    cells.get(at).filter(|cell| cell.x0 <= column)
}

impl Layout {
    pub fn build(region: &Region, owners: Option<&Owners>, key: Key) -> Self {
        let view = key
            .zoom
            .and_then(|at| region.blocks.get(at))
            .map(|block| block.start..block.start.saturating_add(block.size));
        let scale = Scale::new(region, key.mode, view);
        let cells = cells(region, &scale, key.width);
        let inner = owners
            .map(|owners| {
                cells
                    .iter()
                    .filter(|cell| cell.blocks.len() == 1)
                    .filter_map(|cell| {
                        let block = cell.blocks.start;
                        let found = inner(owners, &region.blocks[block], &scale, key.width);
                        (!found.is_empty()).then_some(Inner {
                            block,
                            cells: found,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            key,
            scale,
            cells,
            inner,
        }
    }

    pub fn hit(&self, region: &Region, column: u32) -> Option<Hit> {
        let cell = find(&self.cells, column)?;
        let block = self.block_at(region, cell, column);
        let inner = self
            .inner
            .iter()
            .find(|inner| inner.block == block)
            .and_then(|inner| find(&inner.cells, column));
        if let Some(found) = inner {
            let over = match found.blocks.len() {
                1 => Over::One(Item::Owner(found.blocks.start)),
                count => Over::Symbols {
                    count,
                    bytes: found.bytes,
                },
            };
            let x = found.x0..found.x1;
            return Some(Hit { block, over, x });
        }
        let over = match cell.blocks.len() {
            1 => Over::One(Item::Block(block)),
            count => Over::Sections {
                count,
                bytes: cell.bytes,
            },
        };
        let x = cell.x0..cell.x1;
        Some(Hit { block, over, x })
    }

    fn block_at(&self, region: &Region, cell: &Cell, column: u32) -> usize {
        if cell.blocks.len() == 1 {
            return cell.blocks.start;
        }
        let fraction = (f64::from(column) + 0.5) / f64::from(self.key.width);
        let address = self.scale.offset(fraction);
        let candidates = cell
            .blocks
            .clone()
            .filter_map(|at| Some((at, region.blocks.get(at)?)));
        let mut largest = (cell.blocks.start, 0);
        for (at, block) in candidates {
            if address >= block.start && address - block.start < block.size {
                return at;
            }
            if block.size > largest.1 {
                largest = (at, block.size);
            }
        }
        largest.0
    }

    pub fn span_of(&self, block: usize) -> Option<Range<u32>> {
        let at = self.cells.partition_point(|cell| cell.blocks.end <= block);
        self.cells
            .get(at)
            .filter(|cell| cell.blocks.contains(&block))
            .map(|cell| cell.x0..cell.x1)
    }

    pub fn visible(&self) -> Range<usize> {
        match (self.cells.first(), self.cells.last()) {
            (Some(first), Some(last)) => first.blocks.start..last.blocks.end,
            _ => 0..0,
        }
    }

    pub fn step(&self, caret: Option<usize>, step: Step) -> Option<usize> {
        let visible = self.visible();
        let last = visible.end.checked_sub(1).filter(|_| !visible.is_empty())?;
        let caret = caret.filter(|at| visible.contains(at));
        Some(match (step, caret) {
            (Step::First, _) | (Step::Next, None) => visible.start,
            (Step::Last, _) | (Step::Prev, None) => last,
            (Step::Next, Some(at)) => (at + 1).min(last),
            (Step::Prev, Some(at)) => at.saturating_sub(1).max(visible.start),
        })
    }
}

pub(super) fn is_dim(lit: Option<&[bool]>, blocks: &Range<usize>) -> bool {
    lit.is_some_and(|lit| {
        !blocks
            .clone()
            .any(|at| lit.get(at).copied().unwrap_or(false))
    })
}
