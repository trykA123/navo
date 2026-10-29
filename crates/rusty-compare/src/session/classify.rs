use crate::model::{ContentClass, EntryKind, Reason, Row, SideEntry, Status};

use super::CheckMode;

pub(super) fn classify(row: &mut Row, mode: CheckMode) {
    if matches!(row.raw_status, Status::TypeConflict | Status::CaseConflict) {
        row.display_status = row.raw_status;
        return;
    }
    let (Some(left), Some(right)) = (row.left.as_deref(), row.right.as_deref()) else {
        row.display_status = row.raw_status;
        return;
    };
    let status = match (left.kind, right.kind) {
        (EntryKind::File, EntryKind::File) => file_status(left, right, mode),
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

fn file_status(left: &SideEntry, right: &SideEntry, mode: CheckMode) -> Status {
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
