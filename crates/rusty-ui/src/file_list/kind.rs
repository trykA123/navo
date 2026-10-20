use gpui_kit::Hsla;
use rusty_fs::{EntryKind, FsEntry};

use super::icons;
use crate::format;
use crate::primitives::IconName;
use crate::tokens::Colors;

pub(crate) fn split_name(name: &str, kind: EntryKind) -> (&str, &str) {
    if kind == EntryKind::Folder {
        return (name, "");
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

pub(crate) fn icon(kind: EntryKind, extension: &str, colors: &Colors) -> (IconName, Hsla) {
    match kind {
        EntryKind::Folder => (IconName::Folder, colors.folder),
        EntryKind::Link => (IconName::ArrowRight, colors.muted),
        EntryKind::File => match icons::lookup(extension) {
            Some(group) => (group.icon, (group.tone)(colors)),
            None => (IconName::FileText, colors.muted),
        },
    }
}

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
