//! Path bar crumbs: splits a location into clickable segments, and shows a location inside an
//! archive as the holding folders, the archive name and its inner folders.

use std::path::Path;

use gpui_kit::SharedString;
use navo_core::Location;
use navo_fs::archive;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathSegment {
    pub label: SharedString,
    pub location: Location,
}

const SEPARATORS: [char; 2] = ['/', '\\'];

fn root_label(text: &str) -> String {
    let trimmed = text.trim_end_matches(SEPARATORS);
    if trimmed.is_empty() {
        text.chars().take(1).collect()
    } else {
        trimmed.to_owned()
    }
}

fn label_below(parent: &str, child: &str) -> String {
    child
        .strip_prefix(parent)
        .unwrap_or(child)
        .trim_matches(SEPARATORS)
        .to_owned()
}

/// The crumbs of `location`, outermost first; each crumb opens its own location.
pub fn path_segments(location: &Location) -> Vec<PathSegment> {
    archive_segments(location).unwrap_or_else(|| chain_segments(location))
}

/// Crumbs for a location inside an archive: the holding folders, the archive (opening its root)
/// and the inner folders. None outside an archive or when a crumb location cannot be built.
fn archive_segments(location: &Location) -> Option<Vec<PathSegment>> {
    let (file, names) = archive::shown_parts(location)?;
    let path = file.path().display().into_owned();
    let mut segments = chain_segments(&file);
    segments.last_mut()?.location = archive::location(Path::new(&path), "").ok()?;
    let mut inner = String::new();
    for name in names {
        if !inner.is_empty() {
            inner.push('/');
        }
        inner.push_str(&name);
        let location = archive::location(Path::new(&path), &inner).ok()?;
        segments.push(PathSegment {
            label: name.into(),
            location,
        });
    }
    Some(segments)
}

/// Crumbs from the parent chain of `location`, labelled by the text each step adds.
fn chain_segments(location: &Location) -> Vec<PathSegment> {
    let mut chain = vec![location.clone()];
    while let Some(parent) = chain.last().and_then(Location::parent) {
        chain.push(parent);
    }
    chain.reverse();
    let mut segments = Vec::with_capacity(chain.len());
    let mut parent_text: Option<String> = None;
    for step in chain {
        let text = step.path().display().into_owned();
        let label = match &parent_text {
            Some(parent) => label_below(parent, &text),
            None => root_label(&text),
        };
        parent_text = Some(text);
        segments.push(PathSegment {
            label: label.into(),
            location: step,
        });
    }
    segments
}
