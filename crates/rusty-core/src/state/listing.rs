use std::collections::HashMap;
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

    #[must_use]
    pub fn position(&self, name: &str) -> Option<u64> {
        self.entries
            .iter()
            .position(|entry| entry.name == name)
            .map(|row| row as u64)
    }

    pub(crate) fn follow(&self, row: u64, next: &Self) -> u64 {
        let Some(name) = self.get(row).map(|entry| entry.name.as_str()) else {
            return 0;
        };
        if next.get(row).is_some_and(|entry| entry.name == name) {
            return row;
        }
        let index: HashMap<&str, usize> = next
            .entries
            .iter()
            .enumerate()
            .map(|(at, entry)| (entry.name.as_str(), at))
            .collect();
        self.entries
            .iter()
            .skip(usize::try_from(row).unwrap_or(usize::MAX))
            .find_map(|entry| index.get(entry.name.as_str()))
            .map_or_else(|| next.len().saturating_sub(1), |&at| at as u64)
    }
}
