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

#[allow(unused_variables, reason = "stubs until the state is implemented")]
impl Listing {
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        todo!()
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        todo!()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    #[must_use]
    pub fn get(&self, row: u64) -> Option<&Entry> {
        todo!()
    }

    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        todo!()
    }
}
