//! Quick row classification before the session validates file contents.

use crate::content::{Side, probe_zero_size};
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

/// Classifies file metadata after validating zero sizes that may be unknown; returns unavailable on probe failure.
fn file_status(left: Side<'_>, right: Side<'_>, mode: CheckMode) -> Status {
    // Only sources that map unknown sizes to zero (e.g. Git) need a walk-time stamp probe.
    for side in [left, right] {
        if let Err(reason) = probe_zero_size(side) {
            return Status::Unavailable(reason);
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
