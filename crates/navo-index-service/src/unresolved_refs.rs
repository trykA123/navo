//! Counts one batch's unresolved journal changes by kind for rebuild decisions and log lines,
//! naming the first few references so a VM log identifies them.

use std::fmt;

use navo_index::ntfs::UnresolvedChange;
use navo_index::table::TableChange;

/// Unresolved changes of one journal batch, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Unresolved {
    blocked_add: u64,
    blocked_rename: u64,
    attributes: u64,
    size: u64,
    other: u64,
    first: UnresolvedRefs,
}

impl Unresolved {
    /// Counts the changes by kind and keeps the first three references.
    pub fn count(changes: &[UnresolvedChange]) -> Self {
        let mut counts = Self {
            first: UnresolvedRefs::first(changes),
            ..Self::default()
        };
        for unresolved in changes {
            let count = match unresolved.change {
                // A skipped folder added back names itself: its children are missing, not its parent.
                TableChange::Add { file_id, .. } if file_id == unresolved.missing_reference => {
                    &mut counts.other
                }
                TableChange::Add { .. } => &mut counts.blocked_add,
                TableChange::Rename { .. } => &mut counts.blocked_rename,
                TableChange::Attributes { .. } => &mut counts.attributes,
                TableChange::Size { .. } => &mut counts.size,
                TableChange::Remove { .. } | TableChange::JournalReset => &mut counts.other,
            };
            *count += 1;
        }
        counts
    }

    /// Attribute and size changes for files outside the index change nothing in it.
    pub fn needs_rebuild(&self) -> bool {
        self.blocked_add + self.blocked_rename + self.other > 0
    }

    pub(super) fn total(&self) -> u64 {
        self.blocked_add + self.blocked_rename + self.attributes + self.size + self.other
    }
}

impl fmt::Display for Unresolved {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "blocked_add={} blocked_rename={} attributes={} size={} other={}{}",
            self.blocked_add,
            self.blocked_rename,
            self.attributes,
            self.size,
            self.other,
            self.first
        )
    }
}

/// The first three `(missing_reference, file_id)` pairs of one batch's unresolved changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct UnresolvedRefs([Option<(u128, u128)>; 3]);

impl UnresolvedRefs {
    /// Keeps the references of the first three changes, in report order.
    pub fn first(changes: &[UnresolvedChange]) -> Self {
        let mut refs = Self::default();
        for (slot, unresolved) in refs.0.iter_mut().zip(changes) {
            *slot = Some((
                unresolved.missing_reference,
                change_file_id(&unresolved.change),
            ));
        }
        refs
    }
}

impl fmt::Display for UnresolvedRefs {
    /// Writes ` first_missing:file=777:901,777:902`, or nothing when no change was kept.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut refs = self.0.iter().flatten();
        if let Some((missing, file)) = refs.next() {
            write!(formatter, " first_missing:file={missing}:{file}")?;
        }
        refs.try_for_each(|(missing, file)| write!(formatter, ",{missing}:{file}"))
    }
}

/// Names the file an unresolved change is about; a journal reset names none (0).
fn change_file_id(change: &TableChange) -> u128 {
    match change {
        TableChange::Add { file_id, .. }
        | TableChange::Rename { file_id, .. }
        | TableChange::Attributes { file_id, .. }
        | TableChange::Size { file_id, .. }
        | TableChange::Remove { file_id } => *file_id,
        TableChange::JournalReset => 0,
    }
}
