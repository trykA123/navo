//! Notes mode lists: the active folder's note leads, and a placeholder stands in when the
//! folder has none.

use navo_core::Location;
use navo_l10n::tr;

use super::{ClipId, ClipKind, ClipRow, Origin, Scope};

/// Id of the placeholder row for a folder with no note. History ids start at 1 and saved and
/// note ids carry the top bit, so no stored row has it.
pub(super) const EMPTY_FOLDER_NOTE: ClipId = ClipId(0);

/// The key two spellings of one folder share, as the store compares them: `\` as `/`, no
/// trailing `/`, lower case.
pub(super) fn folder_key(folder: &Location) -> String {
    let lowered = folder.as_str().replace('\\', "/").to_lowercase();
    let trimmed = lowered.trim_end_matches('/');
    if trimmed.is_empty() && !lowered.is_empty() {
        return "/".to_owned();
    }
    trimmed.to_owned()
}

/// The last component of a folder; a root or drive keeps its whole path.
pub(super) fn folder_name(folder: &Location) -> String {
    let text = folder.as_str();
    let name = text
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    if name.is_empty() || name.ends_with(':') {
        text.to_owned()
    } else {
        name.to_owned()
    }
}

pub(super) fn is_folder_row(row: &ClipRow, folder: &Location) -> bool {
    row.kind == ClipKind::Note
        && matches!(&row.scope, Scope::Folder(at) if folder_key(at) == folder_key(folder))
}

/// True for the row that stands in for a folder with no note.
pub(super) fn is_placeholder(row: &ClipRow) -> bool {
    row.id == EMPTY_FOLDER_NOTE && row.kind == ClipKind::Note
}

fn placeholder(folder: &Location) -> ClipRow {
    ClipRow {
        id: EMPTY_FOLDER_NOTE,
        kind: ClipKind::Note,
        title: folder_name(folder),
        detail: tr("clip-note-empty"),
        source_app: None,
        when_ms: 0,
        scope: Scope::Folder(folder.clone()),
        from: Origin::Saved,
        ranges: Vec::new(),
    }
}

/// Puts the note of `folder` first. With no such note, `add_placeholder` puts an empty row in
/// its place; a search leaves it out, since the placeholder matches nothing.
pub(super) fn lead_with_folder(rows: &mut Vec<ClipRow>, folder: &Location, add_placeholder: bool) {
    match rows.iter().position(|row| is_folder_row(row, folder)) {
        Some(0) => {}
        Some(at) => {
            let row = rows.remove(at);
            rows.insert(0, row);
        }
        None if add_placeholder => rows.insert(0, placeholder(folder)),
        None => {}
    }
}
