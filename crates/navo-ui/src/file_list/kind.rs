use navo_fs::{EntryKind, FsEntry};

pub(crate) use crate::file_icons::{icon, split_name};
use crate::format;

pub(super) fn size_text(entry: &FsEntry) -> String {
    match entry.kind {
        EntryKind::Folder if entry.size == 0 => "<DIR>".to_owned(),
        EntryKind::Folder => format::size(entry.size),
        EntryKind::Link => "<LNK>".to_owned(),
        EntryKind::File => format::size(entry.size),
    }
}

pub(super) fn date_text(entry: &FsEntry) -> String {
    entry.modified_ms.map(format::date).unwrap_or_default()
}

pub(super) fn spoken(entry: &FsEntry) -> String {
    let name = entry.name.display();
    let mut words = match entry.kind {
        EntryKind::Folder => format!("{name}, folder"),
        EntryKind::Link => format!("{name}, link"),
        EntryKind::File => format!("{name}, {} bytes", format::bytes(entry.size)),
    };
    if let (EntryKind::File, Some(date)) = (entry.kind, entry.modified_ms.map(format::date)) {
        words.push_str(", ");
        words.push_str(&date);
    }
    if entry.attributes.hidden {
        words.push_str(", hidden");
    }
    words
}
