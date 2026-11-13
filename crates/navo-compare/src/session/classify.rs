//! Quick row classification before the session validates file contents.

use crate::content::Side;
use crate::model::{ContentClass, EntryKind, Reason, Row, SideSource, Status};

use super::CheckMode;

/// Classifies a row using its sources to validate sizes that may be unknown.
pub(super) fn classify(row: &mut Row, mode: CheckMode, sources: [&dyn SideSource; 2]) {
    if matches!(row.raw_status, Status::TypeConflict | Status::CaseConflict) {
        row.display_status = row.raw_status;
        return;
    }
    let (Some(left), Some(right)) = (row.left.as_deref(), row.right.as_deref()) else {
        row.display_status = row.raw_status;
        return;
    };
    let status = match (left.kind, right.kind) {
        (EntryKind::File, EntryKind::File) => file_status(
            Side {
                entry: left,
                source: sources[0],
            },
            Side {
                entry: right,
                source: sources[1],
            },
            mode,
        ),
        (EntryKind::Folder, EntryKind::Folder) => Status::Same,
        (EntryKind::Link, _) | (_, EntryKind::Link) => Status::Unavailable(Reason::Link),
        (EntryKind::Submodule, _) | (_, EntryKind::Submodule) => {
            Status::Unavailable(Reason::Submodule)
        }
        _ => Status::TypeConflict,
    };
    row.raw_status = status;
    row.display_status = status;
    row.content = ContentClass::Unknown;
}

/// Classifies file metadata after validating zero sizes; returns unavailable on stamp failure.
fn file_status(left: Side<'_>, right: Side<'_>, mode: CheckMode) -> Status {
    // Git listings use zero for unavailable blob sizes, so zero needs a validated stamp.
    for side in [left, right] {
        if side.entry.size != 0 {
            continue;
        }
        match side.source.stamp(&side.entry.rel) {
            Err(reason) => return Status::Unavailable(reason),
            Ok(stamp) if stamp.size != 0 || stamp.modified != side.entry.modified => {
                return Status::Unavailable(Reason::ChangedOnDisk);
            }
            Ok(_) => {}
        }
    }
    let (left, right) = (left.entry, right.entry);
    if left.size != right.size {
        return Status::Different {
            first_difference: None,
        };
    }
    match mode {
        CheckMode::SizeTime if left.modified == right.modified => Status::SameSizeTime,
        CheckMode::SizeTime => Status::Different {
            first_difference: None,
        },
        CheckMode::Content => Status::Checking,
    }
}
