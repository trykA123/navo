use std::ops::Range;

pub const ROW_BYTES: u64 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Data { address: u64 },
    Gap { range: Range<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    first_row: u64,
    start: u64,
    rows: u64,
    gap: Option<Range<u64>>,
}

impl Block {
    fn end(&self) -> u64 {
        self.start + self.rows * ROW_BYTES
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    blocks: Vec<Block>,
    len: u64,
}

impl Layout {
    #[must_use]
    pub fn new(ranges: &[Range<u64>]) -> Self {
        let mut layout = Self::default();
        let mut previous_end = None;
        for range in ranges.iter().filter(|range| !range.is_empty()) {
            layout.add(range, previous_end);
            previous_end = Some(range.end);
        }
        layout
    }

    fn add(&mut self, range: &Range<u64>, previous_end: Option<u64>) {
        let start = range.start / ROW_BYTES * ROW_BYTES;
        let end = range.end.div_ceil(ROW_BYTES) * ROW_BYTES;
        if let Some(block) = self.blocks.last_mut()
            && start <= block.end()
        {
            let added = end.saturating_sub(block.end()) / ROW_BYTES;
            block.rows += added;
            self.len += added;
            return;
        }
        let gap = previous_end.map(|previous| previous..range.start);
        let first_row = self.len + u64::from(gap.is_some());
        let rows = (end - start) / ROW_BYTES;
        self.blocks.push(Block {
            first_row,
            start,
            rows,
            gap,
        });
        self.len = first_row + rows;
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn row(&self, index: u64) -> Option<Row> {
        let at = self
            .blocks
            .partition_point(|block| block.first_row <= index);
        let block = self.blocks.get(at.checked_sub(1)?)?;
        let within = index - block.first_row;
        if within < block.rows {
            return Some(Row::Data {
                address: block.start + within * ROW_BYTES,
            });
        }
        let next = self.blocks.get(at)?;
        let range = next.gap.clone()?;
        (index + 1 == next.first_row).then_some(Row::Gap { range })
    }

    #[must_use]
    pub fn row_of(&self, address: u64) -> Option<u64> {
        let at = self.blocks.partition_point(|block| block.start <= address);
        let block = self.blocks.get(at.checked_sub(1)?)?;
        let within = (address - block.start) / ROW_BYTES;
        if within < block.rows {
            return Some(block.first_row + within);
        }
        self.blocks.get(at).map(|next| next.first_row - 1)
    }
}

impl Layout {
    #[must_use]
    pub fn windows(&self, rows: Range<u64>) -> Vec<Range<u64>> {
        let mut windows: Vec<Range<u64>> = Vec::new();
        for row in rows {
            let Some(Row::Data { address }) = self.row(row) else {
                continue;
            };
            match windows.last_mut() {
                Some(last) if last.end == address => last.end += ROW_BYTES,
                _ => windows.push(address..address + ROW_BYTES),
            }
        }
        windows
    }
}
