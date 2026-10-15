use gpui_kit::Hsla;
use rusty_fs::{EntryKind, FsEntry};

use crate::format;
use crate::primitives::IconName;
use crate::tokens::Colors;

const CODE: [&str; 24] = [
    "rs", "toml", "json", "js", "ts", "tsx", "jsx", "py", "c", "h", "cpp", "hpp", "cs", "go",
    "java", "kt", "sh", "ps1", "bat", "cmd", "yml", "yaml", "xml", "html",
];
const IMAGE: [&str; 12] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "tif", "tiff", "heic", "avif",
];
const ARCHIVE: [&str; 11] = [
    "zip", "7z", "rar", "tar", "gz", "tgz", "xz", "zst", "bz2", "iso", "cab",
];
const APP: [&str; 5] = ["exe", "msi", "appimage", "deb", "rpm"];

pub(crate) fn split_name(name: &str, kind: EntryKind) -> (&str, &str) {
    if kind == EntryKind::Folder {
        return (name, "");
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

fn is_in(list: &[&str], extension: &str) -> bool {
    list.iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
}

pub(crate) fn icon(kind: EntryKind, extension: &str, colors: &Colors) -> (IconName, Hsla) {
    match kind {
        EntryKind::Folder => (IconName::Folder, colors.folder),
        EntryKind::Link => (IconName::ArrowRight, colors.muted),
        EntryKind::File if is_in(&CODE, extension) => (IconName::FileCode, colors.kind_code),
        EntryKind::File if is_in(&IMAGE, extension) => (IconName::FileImage, colors.kind_image),
        EntryKind::File if is_in(&ARCHIVE, extension) => {
            (IconName::FileArchive, colors.kind_archive)
        }
        EntryKind::File if is_in(&APP, extension) => (IconName::SquarePlus, colors.kind_app),
        EntryKind::File => (IconName::FileText, colors.muted),
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
