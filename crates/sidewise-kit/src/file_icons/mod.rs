//! File-name splitting and the icon and tone picked for an entry kind and extension.

use gpui_kit::Hsla;
use navo_core::EntryKind;

use crate::primitives::IconName;
use crate::tokens::Colors;

mod icons;

pub fn split_name(name: &str, kind: EntryKind) -> (&str, &str) {
    if kind == EntryKind::Folder {
        return (name, "");
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

pub fn icon(kind: EntryKind, extension: &str, colors: &Colors) -> (IconName, Hsla) {
    match kind {
        EntryKind::Folder => (IconName::Folder, colors.folder),
        EntryKind::Link => (IconName::ArrowRight, colors.muted),
        EntryKind::File => match icons::lookup(extension) {
            Some(group) => (group.icon, (group.tone)(colors)),
            None => (IconName::FileText, colors.muted),
        },
    }
}
