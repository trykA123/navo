//! Side cells of the folder tree: what each side shows, the newer side and the gutter tag.
//! Pure data; the Folders view turns it into text and colour.

use std::collections::HashMap;

use sidewise_ui::browser::{CompareKind, CompareRow, CompareSide, RowStatus};

/// What one side of a row shows: size, time and, for a folder, its direct item count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideCell {
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub items: Option<u32>,
}

/// The side whose copy of a changed file is newer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Newer {
    Left,
    Right,
}

/// The word the centre gutter shows for a file row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GutTag {
    Changed,
    OnlyLeft,
    OnlyRight,
    Conflict,
    SameAfterRules,
    Checking,
}

/// Direct children per folder path (`""` for the roots), as `[left, right]` presence counts.
#[must_use]
pub fn child_counts(rows: &[CompareRow]) -> HashMap<&str, [u32; 2]> {
    let mut counts: HashMap<&str, [u32; 2]> = HashMap::new();
    for row in rows {
        let entry = counts.entry(row.parent()).or_default();
        entry[0] += u32::from(row.left.is_some());
        entry[1] += u32::from(row.right.is_some());
    }
    counts
}

/// The cell for one present side; `items` is the folder's child count on that side.
#[must_use]
pub fn side_cell(side: &CompareSide, items: Option<u32>) -> SideCell {
    SideCell {
        size: side.size,
        modified_ms: side.modified_ms,
        items: items.filter(|_| side.kind == CompareKind::Folder),
    }
}

/// The newer side of a changed file; `None` unless both times are known and differ.
#[must_use]
pub fn newer_of(row: &CompareRow) -> Option<Newer> {
    if row.status != RowStatus::Different || !row.has_file() {
        return None;
    }
    let (left, right) = (row.left?.modified_ms?, row.right?.modified_ms?);
    match left.cmp(&right) {
        std::cmp::Ordering::Less => Some(Newer::Right),
        std::cmp::Ordering::Greater => Some(Newer::Left),
        std::cmp::Ordering::Equal => None,
    }
}

/// The gutter tag of a file status; `None` when the gutter stays empty.
#[must_use]
pub fn gut_of(status: RowStatus) -> Option<GutTag> {
    match status {
        RowStatus::Different => Some(GutTag::Changed),
        RowStatus::OnlyLeft => Some(GutTag::OnlyLeft),
        RowStatus::OnlyRight => Some(GutTag::OnlyRight),
        RowStatus::TypeConflict | RowStatus::CaseConflict => Some(GutTag::Conflict),
        RowStatus::SameAfterRules => Some(GutTag::SameAfterRules),
        RowStatus::Checking | RowStatus::NotChecked(_) => Some(GutTag::Checking),
        RowStatus::Same | RowStatus::SameSizeTime | RowStatus::Unavailable(_) => None,
    }
}
