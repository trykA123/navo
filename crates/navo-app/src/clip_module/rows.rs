//! Palette rows from history rows. Only the first line and a capped preview leave the snapshot.

use std::ops::Range;

use navo_clipboard::{Content, Field, Found, Item, Saved, SavedKind, Stored};
use navo_core::Location;
use navo_ui::clip_palette::{ClipId, ClipKind, ClipRow, Origin, Scope};

const TITLE_BYTES: usize = 200;
const DETAIL_BYTES: usize = 4096;

/// History and saved rows have separate id spaces; a saved row's palette id has this bit set.
const SAVED_BIT: u64 = 1 << 63;

/// The row a palette id names.
pub(super) enum Target {
    History(i64),
    Saved(i64),
}

pub(super) fn target(id: ClipId) -> Option<Target> {
    let row = i64::try_from(id.0 & !SAVED_BIT).ok()?;
    Some(if id.0 & SAVED_BIT == 0 {
        Target::History(row)
    } else {
        Target::Saved(row)
    })
}

pub(super) fn saved_id(row: i64) -> Option<ClipId> {
    Some(ClipId(u64::try_from(row).ok()? | SAVED_BIT))
}

/// The palette row of a history or saved match.
pub(super) fn row(found: &Found<'_>) -> Option<ClipRow> {
    match found.item {
        Item::History(stored) => history_row(found, stored),
        Item::Saved(saved) => saved_row(found, saved),
    }
}

/// A secret shows its title and user name only; its text is never in a snapshot.
fn saved_row(found: &Found<'_>, saved: &Saved) -> Option<ClipRow> {
    let detail = match saved.kind {
        SavedKind::Secret => saved.user.as_deref(),
        _ => saved.body.as_deref(),
    };
    let title = cut(&saved.title, TITLE_BYTES);
    let in_title = found.field == Some(Field::Title);
    let ranges = in_title.then(|| clip_ranges(&found.ranges, &(0..title.len())));
    Some(ClipRow {
        id: saved_id(saved.id)?,
        kind: saved_kind(saved.kind),
        title: title.to_owned(),
        detail: cut(detail.unwrap_or_default(), DETAIL_BYTES).to_owned(),
        source_app: None,
        when_ms: saved.updated_ms,
        scope: match &saved.scope {
            navo_clipboard::Scope::Global => Scope::Everywhere,
            navo_clipboard::Scope::Folder(path) => {
                location(path).map_or(Scope::Everywhere, Scope::Folder)
            }
        },
        from: Origin::Saved,
        ranges: ranges.unwrap_or_default(),
    })
}

fn saved_kind(kind: SavedKind) -> ClipKind {
    match kind {
        SavedKind::Path => ClipKind::Path,
        SavedKind::Secret => ClipKind::Secret,
        SavedKind::Snippet | SavedKind::Command | SavedKind::Note => ClipKind::Text,
    }
}

fn location(path: &std::path::Path) -> Option<Location> {
    Location::new(path.to_string_lossy().into_owned()).ok()
}

fn history_row(found: &Found<'_>, stored: &Stored) -> Option<ClipRow> {
    let (preview, kind) = preview(stored);
    let span = title_span(&preview);
    Some(ClipRow {
        id: ClipId(u64::try_from(stored.id).ok()?),
        kind,
        title: preview[span.clone()].to_owned(),
        detail: cut(&preview, DETAIL_BYTES).to_owned(),
        source_app: stored.entry.source_app.clone(),
        when_ms: stored.entry.last_ms,
        scope: Scope::Everywhere,
        from: Origin::Clipboard,
        ranges: clip_ranges(&found.ranges, &span),
    })
}

/// The searchable text of the row, capped where it is long, and its kind.
fn preview(stored: &Stored) -> (String, ClipKind) {
    match &stored.entry.content {
        Content::Text(text) => (cut(text, DETAIL_BYTES).to_owned(), text_kind(text)),
        Content::Paths { paths, .. } => {
            let mut joined = String::new();
            for path in paths {
                if !joined.is_empty() {
                    joined.push('\n');
                }
                joined.push_str(&path.to_string_lossy());
                if joined.len() >= DETAIL_BYTES {
                    break;
                }
            }
            (joined, ClipKind::Files)
        }
    }
}

fn text_kind(text: &str) -> ClipKind {
    let text = text.trim();
    let url = text.starts_with("http://") || text.starts_with("https://");
    if url && !text.contains(char::is_whitespace) {
        ClipKind::Link
    } else {
        ClipKind::Text
    }
}

fn cut(text: &str, max: usize) -> &str {
    &text[..text.floor_char_boundary(max)]
}

/// The first non-blank line, up to `TITLE_BYTES`, as a byte span of the text.
fn title_span(text: &str) -> Range<usize> {
    let start = text.len() - text.trim_start().len();
    let rest = &text[start..];
    let line = rest.find(['\n', '\r']).unwrap_or(rest.len());
    let line = cut(&rest[..line], TITLE_BYTES).trim_end();
    start..start + line.len()
}

/// Keeps the highlight ranges inside the title, moved to the title's start.
fn clip_ranges(ranges: &[Range<usize>], span: &Range<usize>) -> Vec<Range<usize>> {
    ranges
        .iter()
        .filter_map(|range| {
            let start = range.start.max(span.start);
            let end = range.end.min(span.end);
            (start < end).then(|| start - span.start..end - span.start)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_is_the_first_nonblank_line_and_ranges_follow_it() {
        let text = "\n  hello world\nsecond";
        let span = title_span(text);
        assert_eq!(&text[span.clone()], "hello world");
        let ranges = clip_ranges(&[3..8, 9..14, 15..21], &span);
        assert_eq!(ranges, vec![0..5, 6..11]);
    }

    #[test]
    fn a_long_title_is_cut_on_a_character_boundary() {
        let text = "é".repeat(TITLE_BYTES);
        let span = title_span(&text);
        assert!(span.len() <= TITLE_BYTES);
        assert!(text.is_char_boundary(span.end));
    }

    #[test]
    fn only_a_single_web_address_is_a_link() {
        assert_eq!(text_kind(" https://example.org/a "), ClipKind::Link);
        assert_eq!(text_kind("see https://example.org"), ClipKind::Text);
        assert_eq!(text_kind("https://example.org and more"), ClipKind::Text);
    }
}
