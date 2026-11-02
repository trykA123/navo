use super::SortBy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListParts {
    pub ext: bool,
    pub size: bool,
    pub date: bool,
    pub icons: bool,
}

impl Default for ListParts {
    fn default() -> Self {
        Self {
            ext: true,
            size: true,
            date: true,
            icons: true,
        }
    }
}

impl ListParts {
    #[must_use]
    pub fn shows(self, by: SortBy) -> bool {
        match by {
            SortBy::Name => true,
            SortBy::Ext => self.ext,
            SortBy::Size => self.size,
            SortBy::Date => self.date,
        }
    }
}
