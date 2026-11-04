use navo_core::Location;

use super::{Sides, SyncBlock};
use crate::model::CaseRule;

/// Refuses roots that are one folder, nest, or have `.` or `..` segments.
///
/// The test is on text only. Mapped drives, links, junctions and different providers
/// for one place are not caught.
pub(super) fn check(sides: &Sides) -> Result<(), SyncBlock> {
    let (left, right) = (&sides.left.root, &sides.right.root);
    if [left, right].into_iter().any(has_dot_segment) {
        return Err(SyncBlock::RootNotPlain);
    }
    let fold = [sides.left.case, sides.right.case].contains(&CaseRule::Insensitive);
    if within(left, right, fold) || within(right, left, fold) {
        Err(SyncBlock::SameOrNestedRoots)
    } else {
        Ok(())
    }
}

fn has_dot_segment(place: &Location) -> bool {
    let text = place.path().to_text();
    let windows = is_windows(place, &text);
    text.split(|c| c == '/' || (windows && c == '\\'))
        .any(|segment| segment == "." || segment == "..")
}

fn is_windows(place: &Location, text: &str) -> bool {
    let mut chars = text.chars();
    place.is_local()
        && (text.starts_with(r"\\")
            || matches!((chars.next(), chars.next()), (Some(letter), Some(':')) if letter.is_ascii_alphabetic()))
}

/// Whether `inner` is `outer` or lies anywhere below it.
fn within(inner: &Location, outer: &Location, fold: bool) -> bool {
    let outer_key = place_key(outer, fold);
    std::iter::successors(Some(inner.clone()), Location::parent)
        .any(|place| place.same_place(outer) || place_key(&place, fold) == outer_key)
}

/// Provider and plain path text, lower-cased when `fold`.
///
/// Windows text uses `\`, loses a `\\?\` prefix (`\\?\UNC\` becomes `\\`) and repeated
/// separators; Unix text loses repeated `/`. Trailing separators go.
fn place_key(place: &Location, fold: bool) -> (String, String) {
    let text = place.path().to_text();
    let text = if is_windows(place, &text) {
        windows_text(&text)
    } else {
        collapse(&text, '/', 0)
    };
    let text = text.trim_end_matches(['/', '\\']);
    let text = if fold {
        text.to_lowercase()
    } else {
        text.to_owned()
    };
    (place.provider().as_str().to_owned(), text)
}

fn windows_text(text: &str) -> String {
    let text = text.replace('/', r"\");
    let text = match text.strip_prefix(r"\\?\") {
        Some(rest)
            if rest
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case(r"UNC\")) =>
        {
            format!(r"\\{}", &rest[4..])
        }
        Some(rest) => rest.to_owned(),
        None => text,
    };
    let keep = if text.starts_with(r"\\") { 2 } else { 0 };
    collapse(&text, '\\', keep)
}

/// Collapses runs of `separator`, keeping a leading run of `keep` characters.
fn collapse(text: &str, separator: char, keep: usize) -> String {
    let (head, tail) = text.split_at(keep);
    let mut out = String::from(head);
    let mut last_was_separator = false;
    for c in tail.chars() {
        if c == separator && last_was_separator {
            continue;
        }
        last_was_separator = c == separator;
        out.push(c);
    }
    out
}
