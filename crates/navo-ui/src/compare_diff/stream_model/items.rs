//! The Stream's item list and the small types its model shares (D25, D27).

use crate::browser::{CompareRow, RowStatus};

/// One file the Stream lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamItem {
    pub id: u64,
    pub path: String,
    pub status: RowStatus,
    /// False while the compare still checks the row.
    pub settled: bool,
}

/// The Stream's items in path order, plus how many same-after-rules files form the last group.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamList {
    pub items: Vec<StreamItem>,
    pub rules: usize,
}

/// A place J and K can stop: a hunk of a card, or a whole card without open hunks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stop {
    pub item: usize,
    pub hunk: Option<u32>,
}

/// What Esc closes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscStep {
    ClearSearch,
    CloseOutline,
    Leave,
}

/// Lists the file rows under `folder` that the Stream shows, keeping the order of `rows` (D25).
#[must_use]
pub fn stream_items(rows: &[CompareRow], folder: &str) -> StreamList {
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    };
    let mut list = StreamList::default();
    for row in rows.iter().filter(|row| row.path.starts_with(&prefix)) {
        if is_stream_item(row) {
            list.items.push(StreamItem {
                id: row.id,
                path: row.path.clone(),
                status: row.status,
                settled: row.status != RowStatus::Checking,
            });
        } else if row.status == RowStatus::SameAfterRules && is_file_row(row) {
            list.rules += 1;
        }
    }
    list
}

/// Whether `row` gets a card in the Stream: a file pair that differs, is checked or unavailable.
#[must_use]
pub fn is_stream_item(row: &CompareRow) -> bool {
    is_file_row(row) && row.status != RowStatus::SameAfterRules && listed(row.status)
}

/// Whether `row` has a file on a side and is not a folder pair.
fn is_file_row(row: &CompareRow) -> bool {
    row.has_file() && !row.is_folder_pair()
}

/// Whether a status gets a card.
fn listed(status: RowStatus) -> bool {
    matches!(status, RowStatus::Checking | RowStatus::Unavailable(_)) || status.is_difference()
}
