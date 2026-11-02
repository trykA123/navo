use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescRow {
    pub depth: u8,
    pub cells: Vec<Arc<str>>,
    pub search: Arc<str>,
}

impl DescRow {
    pub(crate) fn new(depth: u8, cells: Vec<Arc<str>>) -> Self {
        let search = cells.join(" ").to_lowercase().into();
        Self {
            depth,
            cells,
            search,
        }
    }
}
