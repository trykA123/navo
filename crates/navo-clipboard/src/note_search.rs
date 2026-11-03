//! Searching notes. The note text is read here and stays here: a summary carries counts only.

use std::fmt;
use std::ops::Range;
use std::path::Path;

use crate::model::{NoteRow, Scope};
use crate::scope::{folder_name, same_folder};
use crate::search::{find_ranges, fold};

/// One matching note without its text.
#[derive(Clone, PartialEq, Eq)]
pub struct NoteSummary {
    pub scope: Scope,
    /// The folder's name, or the caller's title for the global note.
    pub title: String,
    /// Where the query matched inside `title`.
    pub title_ranges: Vec<Range<usize>>,
    pub updated_ms: i64,
    pub lines: usize,
    pub chars: usize,
}

impl fmt::Debug for NoteSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteSummary")
            .field("updated_ms", &self.updated_ms)
            .field("lines", &self.lines)
            .field("chars", &self.chars)
            .finish_non_exhaustive()
    }
}

/// The notes whose title or text holds `query`, ignoring case; an empty query keeps all. The
/// note of `folder` comes first, the rest keep their order.
pub fn search_notes(
    notes: &[NoteRow],
    query: &str,
    folder: Option<&Path>,
    global_title: &str,
) -> Vec<NoteSummary> {
    let needle = fold(query);
    let (mut here, other): (Vec<_>, Vec<_>) = notes
        .iter()
        .filter_map(|note| summary(note, &needle, global_title))
        .partition(|found| is_here(&found.scope, folder));
    here.extend(other);
    here
}

fn summary(note: &NoteRow, needle: &str, global_title: &str) -> Option<NoteSummary> {
    let title = match &note.scope {
        Scope::Global => global_title.to_owned(),
        Scope::Folder(path) => folder_name(path),
    };
    let title_ranges = find_ranges(&title, needle);
    if !needle.is_empty() && title_ranges.is_empty() && find_ranges(&note.body, needle).is_empty() {
        return None;
    }
    Some(NoteSummary {
        scope: note.scope.clone(),
        title,
        title_ranges,
        updated_ms: note.updated_ms,
        lines: note.body.lines().count(),
        chars: note.body.chars().count(),
    })
}

fn is_here(scope: &Scope, folder: Option<&Path>) -> bool {
    matches!((scope, folder), (Scope::Folder(path), Some(folder)) if same_folder(path, folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(scope: Scope, body: &str) -> NoteRow {
        NoteRow {
            scope,
            body: body.to_owned(),
            updated_ms: 7,
        }
    }

    fn folder(path: &str) -> Scope {
        Scope::Folder(path.into())
    }

    fn sample() -> Vec<NoteRow> {
        vec![
            note(Scope::Global, "remember the milk\nand eggs"),
            note(folder("/work/alpha"), "alpha secret-ish text"),
            note(folder("/work/Beta"), "line one\nline two\nline three"),
        ]
    }

    fn titles(found: &[NoteSummary]) -> Vec<&str> {
        found.iter().map(|note| note.title.as_str()).collect()
    }

    #[test]
    fn an_empty_query_lists_every_note_with_counts() {
        let found = search_notes(&sample(), "", None, "Global note");
        assert_eq!(titles(&found), ["Global note", "alpha", "Beta"]);
        assert_eq!((found[0].lines, found[0].chars), (2, 26));
        assert_eq!(found[2].lines, 3);
    }

    #[test]
    fn the_active_folder_note_comes_first_whatever_the_case() {
        let found = search_notes(&sample(), "", Some(Path::new(r"\work\beta\")), "G");
        assert_eq!(titles(&found), ["Beta", "G", "alpha"]);
    }

    #[test]
    fn the_query_matches_the_folder_name_or_the_text() {
        let by_name = search_notes(&sample(), "BET", None, "Global note");
        assert_eq!(titles(&by_name), ["Beta"]);
        let span: Vec<_> = by_name[0]
            .title_ranges
            .iter()
            .map(|r| (r.start, r.end))
            .collect();
        assert_eq!(span, [(0, 3)]);
        let by_text = search_notes(&sample(), "milk", None, "Global note");
        assert_eq!(titles(&by_text), ["Global note"]);
        assert!(by_text[0].title_ranges.is_empty());
        let by_global = search_notes(&sample(), "global", None, "Global note");
        assert_eq!(titles(&by_global), ["Global note"]);
        assert!(search_notes(&sample(), "nothing", None, "G").is_empty());
    }

    #[test]
    fn a_summary_never_shows_the_text() {
        let found = search_notes(&sample(), "milk", None, "G");
        let shown = format!("{found:?}");
        assert!(
            !shown.contains("milk") && !shown.contains("alpha"),
            "{shown}"
        );
    }
}
