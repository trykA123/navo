//! Notes mode behind the hooks: one row per note, with counts and no text.

use navo_clipboard::{FailKind, NoteSummary, Scope};
use navo_core::Location;
use navo_ui::clip_palette::{ClipKind, ClipResults, ClipRow, Origin, Scope as UiScope};

use super::rows::{self, location};
use super::saved::{deleted, message, now_ms, wait};
use super::{ClipModule, View, folder_path};

const TITLE_BYTES: usize = 200;

impl ClipModule {
    /// The Notes results for a query. The service reads the text; only counts come back, and
    /// a slow or failed service shows as the notice with no rows.
    pub(super) fn note_results(
        &self,
        view: View,
        query: &str,
        folder: Option<&Location>,
    ) -> ClipResults {
        let View {
            live,
            notice,
            revision,
        } = view;
        let mut results = ClipResults {
            notice,
            revision,
            ..ClipResults::default()
        };
        let Some((service, snapshot)) = live else {
            return results;
        };
        results.kept = u32::try_from(snapshot.history.len()).unwrap_or(u32::MAX);
        if snapshot.notes.is_empty() {
            return results;
        }
        let global = navo_l10n::tr("clip-note-global");
        let search = service.search_notes(query.to_owned(), folder_path(folder), global);
        match wait(search) {
            Ok(found) => results.rows = found.iter().map(row).collect(),
            Err(error) => results.notice = Some(error),
        }
        results
    }

    /// Removes the note a row names. The id is matched against the snapshot's note headers.
    pub(super) fn delete_note(
        &self,
        service: &navo_clipboard::ClipService,
        hash: u64,
    ) -> Result<(), String> {
        let snapshot = service.snapshot();
        let named: Vec<_> = snapshot
            .notes
            .iter()
            .filter(|note| rows::note_hash(&note.scope) == hash)
            .collect();
        match named[..] {
            [note] => deleted(service.set_note(note.scope.clone(), String::new(), now_ms())),
            [] => Err(message(FailKind::NotFound)),
            _ => Err(message(FailKind::Refused)),
        }
    }
}

fn row(note: &NoteSummary) -> ClipRow {
    let title = &note.title[..note.title.floor_char_boundary(TITLE_BYTES)];
    ClipRow {
        id: rows::note_id(&note.scope),
        kind: ClipKind::Note,
        title: title.to_owned(),
        detail: navo_l10n::t!("clip-note-detail", lines = note.lines, chars = note.chars),
        source_app: None,
        when_ms: note.updated_ms,
        scope: match &note.scope {
            Scope::Global => UiScope::Everywhere,
            Scope::Folder(path) => location(path).map_or(UiScope::Everywhere, UiScope::Folder),
        },
        from: Origin::Saved,
        ranges: note
            .title_ranges
            .iter()
            .filter(|range| range.end <= title.len())
            .cloned()
            .collect(),
    }
}
