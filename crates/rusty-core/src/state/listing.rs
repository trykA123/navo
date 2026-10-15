use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    entries: Arc<[Entry]>,
}

impl Listing {
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries: entries.into(),
        }
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        self.entries.len() as u64
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn get(&self, row: u64) -> Option<&Entry> {
        usize::try_from(row).ok().and_then(|i| self.entries.get(i))
    }

    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}
