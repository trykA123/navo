use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemRef {
    Node(u32),
    Message(u32),
    Signal { message: u32, signal: u32 },
    Characteristic(u32),
    Measurement(u32),
    Compu(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    pub depth: u8,
    pub group: bool,
    pub item: Option<ItemRef>,
    pub label: Arc<str>,
    pub aux: Arc<str>,
    pub line: u64,
    pub search: Arc<str>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    pub rows: Vec<OutlineRow>,
}

impl Outline {
    #[must_use]
    pub fn item_at_line(&self, line: u64) -> Option<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.item.is_some() && row.line <= line)
            .max_by_key(|(_, row)| row.line)
            .map(|(index, _)| index)
    }

    #[must_use]
    pub fn parent(&self, row: usize) -> Option<usize> {
        let depth = self.rows.get(row)?.depth;
        self.rows[..row].iter().rposition(|row| row.depth < depth)
    }
}
