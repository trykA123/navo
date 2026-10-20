use std::ops::Range;

pub const ROW_BYTES: u64 = 16;
const FOLD_UNDER: u64 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutOptions {
    pub row_bytes: u64,
    pub fold_under: u64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            row_bytes: ROW_BYTES,
            fold_under: FOLD_UNDER,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Data {
        address: u64,
        missing: Vec<Range<u64>>,
    },
    Gap {
        range: Range<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    first_row: u64,
    start: u64,
    rows: u64,
    gap: Option<Range<u64>>,
    folds: Vec<Range<u64>>,
}

impl Block {
    fn end(&self, row_bytes: u64) -> u64 {
        self.start + self.rows * row_bytes
    }

    fn missing(&self, address: u64, row_bytes: u64) -> Vec<Range<u64>> {
        let row_end = address + row_bytes;
        let from = self.folds.partition_point(|fold| fold.end <= address);
        self.folds[from..]
            .iter()
            .take_while(|fold| fold.start < row_end)
            .map(|fold| fold.start.max(address)..fold.end.min(row_end))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    blocks: Vec<Block>,
    len: u64,
    row_bytes: u64,
    fold_under: u64,
    folded: u64,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new(&[], LayoutOptions::default())
    }
}

impl Layout {
    #[must_use]
    pub fn new(ranges: &[Range<u64>], options: LayoutOptions) -> Self {
        let mut layout = Self {
            blocks: Vec::new(),
            len: 0,
            row_bytes: options.row_bytes.max(1),
            fold_under: options.fold_under,
            folded: 0,
        };
        let mut previous_end = None;
        for range in ranges.iter().filter(|range| !range.is_empty()) {
            layout.add(range, previous_end);
            previous_end = Some(range.end);
        }
        layout
    }

    fn add(&mut self, range: &Range<u64>, previous_end: Option<u64>) {
        let row_bytes = self.row_bytes;
        let start = range.start / row_bytes * row_bytes;
        let end = range.end.div_ceil(row_bytes) * row_bytes;
        let gap = previous_end.map(|previous| previous..range.start);
        let fold = gap
            .clone()
            .filter(|gap| gap.start < gap.end && gap.end - gap.start < self.fold_under);
        if let Some(block) = self.blocks.last_mut()
            && (fold.is_some() || start <= block.end(row_bytes))
        {
            let added = end.saturating_sub(block.end(row_bytes)) / row_bytes;
            block.rows += added;
            self.len += added;
            if let Some(fold) = fold {
                block.folds.push(fold);
                self.folded += 1;
            }
            return;
        }
        let first_row = self.len + u64::from(gap.is_some());
        let rows = (end - start) / row_bytes;
        self.blocks.push(Block {
            first_row,
            start,
            rows,
            gap,
            folds: Vec::new(),
        });
        self.len = first_row + rows;
    }

    #[must_use]
    pub fn row_bytes(&self) -> u64 {
        self.row_bytes
    }

    #[must_use]
    pub fn folded(&self) -> u64 {
        self.folded
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
            let address = block.start + within * self.row_bytes;
            return Some(Row::Data {
                address,
                missing: block.missing(address, self.row_bytes),
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
        let within = (address - block.start) / self.row_bytes;
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
            let Some(Row::Data { address, .. }) = self.row(row) else {
                continue;
            };
            match windows.last_mut() {
                Some(last) if last.end == address => last.end += self.row_bytes,
                _ => windows.push(address..address + self.row_bytes),
            }
        }
        windows
    }
}
