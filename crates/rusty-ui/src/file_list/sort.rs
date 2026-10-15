#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Name,
    Ext,
    Size,
    Date,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SortKey {
    pub by: SortBy,
    pub descending: bool,
}
