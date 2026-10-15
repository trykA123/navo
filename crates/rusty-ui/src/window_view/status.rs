use rusty_core::Selection;
use rusty_fs::{EntryKind, FsEntry};

use crate::status_bar::StatusBarProps;

pub(super) fn status_of(rows: &[FsEntry], marked: &Selection) -> StatusBarProps {
    let mut status = StatusBarProps::default();
    for (row, entry) in (0..).zip(rows) {
        let is_folder = entry.kind == EntryKind::Folder;
        status.folders += u64::from(is_folder);
        status.files += u64::from(!is_folder);
        status.bytes += entry.size;
        if marked.is_marked(row) {
            status.marked += 1;
            status.marked_bytes += entry.size;
        }
    }
    status
}

pub(super) const FKEYS: [(&str, &str); 9] = [
    ("F2", "Rename"),
    ("F3", "View"),
    ("F4", "Edit"),
    ("F5", "Copy"),
    ("F6", "Move"),
    ("F7", "New folder"),
    ("F8", "Delete"),
    ("F9", "Drawer"),
    ("F10", "Palette"),
];
